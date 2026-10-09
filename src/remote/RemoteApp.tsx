import { useEffect, useRef, useState } from "react";
import { Network, Search, Swords } from "lucide-react";
import { BrandIcon } from "@/components/BrandIcon";
import { RoleIcon } from "../runes/RoleIcon";
import { RunesPanel } from "../runes/RunesPanel";
import type { RunesView, ProBuildsView, KeystoneBuildView, MatchupView, ChampionApi, Selection } from "../runes/types";
import "./remote.css";

type RemoteState = "disabled" | "starting" | "notInstalled" | "disconnected" | "available" | "failed";

type RemoteStatus = {
  state: RemoteState;
  message: string | null;
  tailscaleInstalled: boolean;
  tailscaleRunning: boolean;
  leagueRunning: boolean;
  lcuConnected: boolean;
};

type Champion = { id: number; name: string; positions: string[] };

type ChampionSelect = {
  gameId: number;
  actionKind: "pick" | "ban" | null;
  selectedChampionId: number | null;
  prepickChampionId: number | null;
  canComplete: boolean;
  canPrepick: boolean;
  availableChampionIds: number[];
};

type GameSnapshot = {
  phase: string;
  queueTimeSeconds: number | null;
  readyCheck: boolean;
  championSelect: ChampionSelect | null;
  message: string | null;
};

const positions = [
  { label: "All", value: "ALL" },
  { label: "Top", value: "TOP" },
  { label: "Jungle", value: "JUNGLE" },
  { label: "Mid", value: "MIDDLE" },
  { label: "Bot", value: "BOTTOM" },
  { label: "Support", value: "UTILITY" },
] as const;

function formatQueueTime(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remaining = Math.floor(seconds % 60);
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(remaining).padStart(2, "0")}`
    : `${minutes}:${String(remaining).padStart(2, "0")}`;
}

// Once the phone has switched (or declined), do not ask again on this origin.
const HANDOFF_DISMISSED_KEY = "swapper_local_handoff_dismissed";
const LOCAL_HOSTNAME = "swapper.local";

function isIpv4Hostname(hostname: string): boolean {
  return /^\d{1,3}(\.\d{1,3}){3}$/.test(hostname);
}

// The stable address on the same LAN port the phone reached over the IP.
function localRemoteOrigin(): string {
  return `http://${LOCAL_HOSTNAME}${window.location.port ? `:${window.location.port}` : ""}`;
}

// A short rising two-tone cue for a ready check. Autoplay rules or missing
// support make it a silent no-op; the vibration API is Android-only.
function playReadyCheckCue() {
  const CueAudio = window.AudioContext
    ?? (window as typeof window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!CueAudio) return;
  const ctx = new CueAudio();
  if (ctx.state === "suspended") {
    void ctx.close();
    return;
  }
  const now = ctx.currentTime;
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = "sine";
  osc.frequency.setValueAtTime(880, now);
  osc.frequency.setValueAtTime(1174, now + 0.16);
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(0.18, now + 0.02);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.36);
  osc.connect(gain);
  gain.connect(ctx.destination);
  osc.start();
  osc.stop(now + 0.38);
  osc.onended = () => void ctx.close();
}

export default function RemoteApp() {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [live, setLive] = useState(false);
  // True once the phone has reached the PC at least once, so the "can't reach"
  // guidance only appears after a real disconnect, not during the first load.
  const [everConnected, setEverConnected] = useState(false);
  const [game, setGame] = useState<GameSnapshot | null>(null);
  const [champions, setChampions] = useState<Champion[]>([]);
  const [catalogError, setCatalogError] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [position, setPosition] = useState("ALL");
  const [showUnavailable, setShowUnavailable] = useState(false);
  const [prepickId, setPrepickId] = useState<number | null>(null);
  const [tab, setTab] = useState<"pick" | "runes">("pick");
  const [showHandoff, setShowHandoff] = useState(false);
  const [handoffBusy, setHandoffBusy] = useState(false);
  const [runes, setRunes] = useState<RunesView | null>(null);
  const [runesBusy, setRunesBusy] = useState(false);
  const [runesError, setRunesError] = useState<string | null>(null);
  const runePosition = useRef<string | undefined>(undefined);
  const runeChampion = useRef(0);
  const runeRequest = useRef(0);
  const socketRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    let cancelled = false;
    let attempt = 0;
    let retry: number | undefined;

    fetch("/api/status", { cache: "no-store" })
      .then((response) => response.json())
      .then((next: RemoteStatus) => { if (!cancelled) setStatus(next); })
      .catch(() => undefined);

    function connect() {
      if (cancelled) return;
      const scheme = window.location.protocol === "https:" ? "wss" : "ws";
      const socket = new WebSocket(`${scheme}://${window.location.host}/ws`);
      socketRef.current = socket;
      socket.onopen = () => {
        if (cancelled) return;
        attempt = 0;
        setEverConnected(true);
        setLive(true);
      };
      socket.onmessage = (event) => {
        if (cancelled) return;
        try {
          const payload = JSON.parse(String(event.data)) as { type?: string; status?: RemoteStatus };
          if (payload.type === "status" && payload.status) setStatus(payload.status);
          if (payload.type === "runes") void loadRunes();
        } catch { /* Ignore malformed updates. */ }
      };
      socket.onclose = () => {
        if (cancelled) return;
        setLive(false);
        attempt += 1;
        retry = window.setTimeout(connect, Math.min(15_000, 750 * 2 ** attempt));
      };
      socket.onerror = () => socket.close();
    }

    connect();
    return () => {
      cancelled = true;
      if (retry) window.clearTimeout(retry);
      socketRef.current?.close();
    };
  }, []);

  // Only when the phone reached Swapper over the LAN IP: after a moment, check
  // whether the stable `swapper.local` name also answers here. If it does,
  // offer a one-time switch so the paired cookie survives a future IP change.
  // If it does not resolve, nothing is shown and the IP flow is unchanged.
  useEffect(() => {
    if (window.location.protocol !== "http:" || !isIpv4Hostname(window.location.hostname)) return;
    try {
      if (window.localStorage.getItem(HANDOFF_DISMISSED_KEY) === "1") return;
    } catch { return; }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      const controller = new AbortController();
      const abort = window.setTimeout(() => controller.abort(), 1500);
      fetch(`${localRemoteOrigin()}/handoff/ping`, { mode: "no-cors", cache: "no-store", signal: controller.signal })
        .then(() => { if (!cancelled) setShowHandoff(true); })
        .catch(() => undefined)
        .finally(() => window.clearTimeout(abort));
    }, 1500);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, []);

  function dismissHandoff() {
    try { window.localStorage.setItem(HANDOFF_DISMISSED_KEY, "1"); } catch { /* ignore */ }
    setShowHandoff(false);
  }

  async function startHandoff() {
    setHandoffBusy(true);
    try {
      const response = await fetch("/handoff/token", { method: "POST", headers: { "X-Swapper-Action": "1" } });
      const result = response.ok ? await response.json() as { token?: string } : null;
      if (!result?.token) throw new Error("handoff unavailable");
      try { window.localStorage.setItem(HANDOFF_DISMISSED_KEY, "1"); } catch { /* ignore */ }
      window.location.assign(`${localRemoteOrigin()}/handoff?token=${encodeURIComponent(result.token)}`);
    } catch {
      setHandoffBusy(false);
      setShowHandoff(false);
    }
  }

  useEffect(() => {
    let cancelled = false;
    async function poll() {
      try {
        const response = await fetch("/api/game", { cache: "no-store" });
        if (response.ok && !cancelled) setGame(await response.json() as GameSnapshot);
      } catch {
        if (!cancelled) setGame(null);
      }
    }
    void poll();
    const timer = window.setInterval(() => void poll(), 1500);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, []);

  // One vibration/sound cue per ready check, only while this page is open.
  const readyCheckActive = game?.phase === "ReadyCheck" && Boolean(game.readyCheck);
  const wasReadyCheckActive = useRef(false);
  useEffect(() => {
    if (readyCheckActive && !wasReadyCheckActive.current) {
      try { navigator.vibrate?.([180, 90, 180]); } catch { /* Unsupported. */ }
      try { playReadyCheckCue(); } catch { /* Blocked by autoplay rules. */ }
    }
    wasReadyCheckActive.current = readyCheckActive;
  }, [readyCheckActive]);

  useEffect(() => {
    if (!status?.lcuConnected || game?.phase !== "ChampSelect" || champions.length > 0) return;
    let cancelled = false;
    async function loadCatalog() {
      try {
        const response = await fetch("/api/champions");
        if (!response.ok) throw new Error("Catalog unavailable");
        const entries = await response.json() as Champion[];
        if (!cancelled) { setChampions(entries); setCatalogError(false); }
      } catch {
        if (!cancelled) setCatalogError(true);
      }
    }
    void loadCatalog();
    const retry = window.setInterval(() => void loadCatalog(), 8000);
    return () => { cancelled = true; window.clearInterval(retry); };
  }, [status?.lcuConnected, game?.phase, champions.length]);

  useEffect(() => {
    const select = game?.championSelect;
    if (!select?.canPrepick || select.actionKind === "pick" || !prepickId || select.prepickChampionId === prepickId) return;
    let cancelled = false;
    let pending = false;
    async function applyPrepick() {
      if (pending) return;
      pending = true;
      try {
        const response = await fetch("/api/champion/prepick", {
          method: "POST",
          headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
          body: JSON.stringify({ championId: prepickId }),
        });
        if (!cancelled) {
          if (response.ok) setActionError(null);
          else {
            const result = await response.json() as { message?: string };
            setActionError(result.message ?? "Could not apply the prepick.");
          }
        }
      } catch {
        if (!cancelled) setActionError("Could not apply the prepick. Waiting to retry.");
      } finally {
        pending = false;
      }
    }
    void applyPrepick();
    const retry = window.setInterval(() => void applyPrepick(), 2500);
    return () => { cancelled = true; window.clearInterval(retry); };
  }, [game?.championSelect?.gameId, game?.championSelect?.actionKind, game?.championSelect?.canPrepick, game?.championSelect?.prepickChampionId, prepickId]);

  useEffect(() => {
    if (game?.phase !== "ChampSelect") setPrepickId(null);
  }, [game?.phase]);

  useEffect(() => {
    if (game?.phase !== "ChampSelect") setTab("pick");
  }, [game?.phase]);

  useEffect(() => {
    if (tab !== "runes" || game?.phase !== "ChampSelect") return;
    const selectedChampion = game.championSelect?.selectedChampionId ?? game.championSelect?.prepickChampionId ?? 0;
    if (selectedChampion > 0 && runeChampion.current !== selectedChampion) runePosition.current = undefined;
    void loadRunes();
  }, [tab, game?.phase, game?.championSelect?.selectedChampionId, game?.championSelect?.prepickChampionId]);

  async function refreshGame() {
    const response = await fetch("/api/game", { cache: "no-store" });
    if (response.ok) setGame(await response.json() as GameSnapshot);
  }

  async function sendAction(path: string, body?: object) {
    setActionBusy(true);
    setActionError(null);
    try {
      const response = await fetch(path, {
        method: "POST",
        headers: { "X-Swapper-Action": "1", ...(body ? { "Content-Type": "application/json" } : {}) },
        body: body ? JSON.stringify(body) : undefined,
      });
      const result = await response.json() as { ok: boolean; message: string | null };
      if (!response.ok || !result.ok) setActionError(result.message ?? "League rejected the action.");
      await refreshGame();
    } catch {
      setActionError("Could not reach Swapper. Check your connection and try again.");
    } finally {
      setActionBusy(false);
    }
  }

  function chooseChampion(champion: Champion) {
    const action = game?.championSelect?.actionKind;
    if (action) {
      void sendAction("/api/champion/select", { championId: champion.id });
    } else if (game?.championSelect?.canPrepick) {
      setPrepickId(champion.id);
    }
  }

  async function loadRunes(position?: string) {
    if (position) runePosition.current = position;
    const selectedPosition = position ?? runePosition.current;
    const request = ++runeRequest.current;
    try {
      const query = selectedPosition ? `?position=${encodeURIComponent(selectedPosition)}` : "";
      const response = await fetch(`/api/runes${query}`, { cache: "no-store" });
      if (response.ok) {
        const next = await response.json() as RunesView;
        if (request === runeRequest.current) {
          setRunes(next);
          runeChampion.current = next.championId;
        }
      }
    } catch {
      if (request === runeRequest.current) setRunesError("Could not load runes.");
    }
  }

  async function importItemBuild(
    championId: number,
    championName: string,
    source: string,
    items: number[],
  ) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const response = await fetch("/api/items/import", {
        method: "POST",
        headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
        body: JSON.stringify({ championId, championName, source, items }),
      });
      const result = await response.json() as { ok: boolean; message: string | null };
      if (!response.ok || !result.ok) throw new Error(result.message ?? "Could not add the item set.");
    } catch (reason) {
      setRunesError(String(reason));
      throw reason;
    } finally {
      setRunesBusy(false);
    }
  }

  async function applyRunes(
    selection: Selection,
    presetIndex: number | null,
    spells: number[] | null,
    enemyChampionId?: number | null,
  ) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const response = await fetch("/api/runes/apply", {
        method: "POST",
        headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
        body: JSON.stringify({
          selection,
          presetIndex,
          spells,
          position: runes?.position ?? null,
          enemyChampionId: enemyChampionId ?? null,
        }),
      });
      const result = await response.json() as { ok: boolean; message: string | null };
      if (!response.ok || !result.ok) setRunesError(result.message ?? "League rejected the rune page.");
      else await loadRunes();
    } catch {
      setRunesError("Could not reach Swapper. Check your connection and try again.");
    } finally {
      setRunesBusy(false);
    }
  }

  async function pickSpell(slot: "d" | "f", spellId: number) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const response = await fetch("/api/runes/spells", {
        method: "POST",
        headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
        body: JSON.stringify({ slot, spellId }),
      });
      const result = await response.json() as { ok: boolean; message: string | null };
      if (!response.ok || !result.ok) setRunesError(result.message ?? "League rejected the summoner spell.");
      else await loadRunes();
    } catch {
      setRunesError("Could not reach Swapper. Check your connection and try again.");
    } finally {
      setRunesBusy(false);
    }
  }

  async function toggleImportItems(enabled: boolean) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const response = await fetch("/api/runes/items-setting", {
        method: "POST",
        headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      if (!response.ok) setRunesError("Could not update the setting.");
      else await loadRunes();
    } catch {
      setRunesError("Could not update the setting.");
    } finally {
      setRunesBusy(false);
    }
  }

  async function toggleSpellsWithRunes(enabled: boolean) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const response = await fetch("/api/runes/spells-setting", {
        method: "POST",
        headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      if (!response.ok) setRunesError("Could not update the setting.");
      else await loadRunes();
    } catch {
      setRunesError("Could not update the setting.");
    } finally {
      setRunesBusy(false);
    }
  }

  async function loadProBuilds(championId: number, position: string, page: number): Promise<ProBuildsView> {
    const response = await fetch(
      `/api/runes/pro-builds?championId=${championId}&position=${encodeURIComponent(position)}&page=${page}`,
      { cache: "no-store" },
    );
    if (!response.ok) throw new Error("Could not load pro builds.");
    return await response.json() as ProBuildsView;
  }

  async function loadKeystoneBuild(
    championId: number,
    position: string,
    tier: string,
    keystone: number,
  ): Promise<KeystoneBuildView | null> {
    const params = new URLSearchParams({
      championId: String(championId),
      position,
      tier,
      keystone: String(keystone),
    });
    const response = await fetch(`/api/runes/keystone-build?${params.toString()}`, { cache: "no-store" });
    if (!response.ok) return null;
    return await response.json() as KeystoneBuildView | null;
  }

  async function loadMatchup(
    championId: number,
    enemyChampionId: number,
    position: string,
    tier: string,
  ): Promise<MatchupView | null> {
    const params = new URLSearchParams({
      championId: String(championId),
      enemyChampionId: String(enemyChampionId),
      position,
      tier,
    });
    const response = await fetch(`/api/runes/matchup?${params.toString()}`, { cache: "no-store" });
    if (!response.ok) return null;
    return await response.json() as MatchupView | null;
  }

  async function getJson<T>(path: string, params: Record<string, string | null>): Promise<T> {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) if (value) query.set(key, value);
    const response = await fetch(`${path}?${query.toString()}`, { cache: "no-store" });
    if (!response.ok) throw new Error("Could not load champion data.");
    return await response.json() as T;
  }

  const championApi: ChampionApi = {
    list: () => getJson("/api/runes/champions", {}),
    counters: (championId, position, tier) =>
      getJson("/api/runes/counters", { championId: String(championId), position, tier }),
    overview: (championId, position, tier) =>
      getJson("/api/runes/overview", { championId: String(championId), position, tier }),
    tierList: (position, tier) => getJson("/api/runes/tierlist", { position, tier }),
  };

  async function toggleAutoApply(enabled: boolean) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      await fetch("/api/runes/auto-apply", {
        method: "POST",
        headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      await loadRunes();
    } catch {
      setRunesError("Could not update the setting.");
    } finally {
      setRunesBusy(false);
    }
  }

  async function setTier(tier: string) {
    setRunesBusy(true);
    setRunesError(null);
    try {
      const response = await fetch("/api/runes/tier", {
        method: "POST",
        headers: { "X-Swapper-Action": "1", "Content-Type": "application/json" },
        body: JSON.stringify({ tier }),
      });
      const result = await response.json() as { ok: boolean; message: string | null };
      if (!response.ok || !result.ok) setRunesError(result.message ?? "Could not update the rank filter.");
      else await loadRunes();
    } catch {
      setRunesError("Could not reach Swapper. Check your connection and try again.");
    } finally {
      setRunesBusy(false);
    }
  }

  const ready = live && status?.state === "available" && status.lcuConnected;
  const swapperConnected = live && status?.state === "available";
  const phase = game?.phase;
  const select = game?.championSelect;
  const inChampSelect = Boolean(status?.lcuConnected && phase === "ChampSelect");
  const showCatalog = Boolean(inChampSelect && select);
  const showRunes = inChampSelect;
  const available = new Set(select?.availableChampionIds ?? []);
  const selected = select?.actionKind ? select.selectedChampionId : prepickId ?? select?.prepickChampionId;
  const filtered = champions.filter((champion) =>
    (showUnavailable || available.has(champion.id))
    && (position === "ALL" || champion.positions.includes(position))
    && champion.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()),
  );
  const title = select?.actionKind === "ban" ? "Choose your ban" : select?.actionKind === "pick" ? "Choose your champion" : select?.canPrepick ? "Prepick a champion" : "Waiting for your turn";

  return (
    <div className="remote-shell">
      <header className={`remote-topbar ${inChampSelect ? "is-compact" : ""}`}>
        <div className="remote-topline">
          <div className="remote-brand">
            <BrandIcon className="remote-mark" size={33} />
            <div className="remote-brand-copy"><strong>Swapper</strong>{!inChampSelect && <span>REMOTE</span>}</div>
          </div>
          {inChampSelect ? (
            <div className="remote-mini-status">
              <span
                className={`remote-chip ${swapperConnected ? "is-good" : "is-bad"}`}
                title={`Swapper on your PC ${swapperConnected ? "connected" : "offline"}`}
                aria-label={`Swapper on your PC ${swapperConnected ? "connected" : "offline"}`}
              >
                <Network size={13} aria-hidden />
                <i />
                {!swapperConnected && <b>PC offline</b>}
              </span>
              <span
                className={`remote-chip ${status?.lcuConnected ? "is-good" : "is-bad"}`}
                title={`League ${status?.lcuConnected ? "connected" : "offline"}`}
                aria-label={`League ${status?.lcuConnected ? "connected" : "offline"}`}
              >
                <Swords size={13} aria-hidden />
                <i />
                {!status?.lcuConnected && <b>League offline</b>}
              </span>
              <span className={`remote-ready ${ready ? "is-ready" : "is-bad"}`}>
                <span className="remote-ready-dot" />{ready ? "Ready" : "Check"}
              </span>
            </div>
          ) : ready ? (
            <span className="remote-ready is-ready">
              <span className="remote-ready-dot" />Ready
            </span>
          ) : null}
        </div>
        {!inChampSelect && (
          <div className="remote-status-line">
            <span className={swapperConnected ? "is-good" : "is-bad"}><i />PC {swapperConnected ? "connected" : "offline"}</span>
            <span className={status?.lcuConnected ? "is-good" : "is-bad"}><i />League {status?.lcuConnected ? "connected" : "offline"}</span>
          </div>
        )}
      </header>

      <main className={`remote-main ${showCatalog || showRunes ? "has-catalog" : ""}`}>
        {status?.message && <p className="remote-inline-error" role="alert">{status.message}</p>}
        {/* League being closed is already the status line and the hero below. */}
        {game?.message && status?.lcuConnected && <p className="remote-inline-error" role="alert">{game.message}</p>}
        {actionError && <p className="remote-inline-error" role="alert">{actionError}</p>}
        {everConnected && !swapperConnected && (
          <p className="remote-offline-help" role="alert">Can't reach Swapper on your PC. If your PC's network changed, open Swapper on your PC → Remote and scan the new QR code.</p>
        )}

        {showHandoff && (
          <div className="remote-handoff" role="status">
            <p>Use <strong>{LOCAL_HOSTNAME}</strong> so this phone keeps working when the PC IP changes.</p>
            <div className="remote-handoff-actions">
              <button disabled={handoffBusy} onClick={() => void startHandoff()}>{handoffBusy ? "Switching…" : "Use swapper.local"}</button>
              <button disabled={handoffBusy} onClick={dismissHandoff}>Not now</button>
            </div>
          </div>
        )}

        {showCatalog || showRunes ? (
          <section className="remote-picker" aria-label="Champion select">
            {tab === "pick" && (
              <div className="remote-picker-head compact">
                <h1>{title}</h1>
              </div>
            )}
            <div className="remote-draft-bar">
              <div className="remote-select-tabs" role="tablist">
                <button type="button" role="tab" aria-selected={tab === "pick"} className={tab === "pick" ? "is-active" : ""} onClick={() => setTab("pick")}>Pick</button>
                <button type="button" role="tab" aria-selected={tab === "runes"} className={tab === "runes" ? "is-active" : ""} onClick={() => setTab("runes")}>Runes</button>
              </div>
              {tab === "pick" && (
                <div className="remote-picker-tools">
                  <label className="remote-search"><Search size={16} /><input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search champions" aria-label="Search champions" /></label>
                  <div className="remote-role-row">
                    <div className="remote-roles" role="group" aria-label="Filter champions by position">
                      {positions.map((entry) => (
                        <button
                          key={entry.value}
                          className={position === entry.value ? "is-active" : ""}
                          title={entry.label}
                          aria-label={entry.label}
                          aria-pressed={position === entry.value}
                          onClick={() => setPosition(entry.value)}
                        >
                          <RoleIcon role={entry.value === "ALL" ? "all" : entry.value} size={18} mode="remote" />
                        </button>
                      ))}
                    </div>
                    <div className="remote-availability">
                      <span>{available.size} available</span>
                      <button onClick={() => setShowUnavailable((value) => !value)}>{showUnavailable ? "Hide locked" : "Show locked"}</button>
                    </div>
                  </div>
                </div>
              )}
            </div>

            {tab === "runes" ? (
              <RunesPanel
                mode="remote"
                view={runes}
                loading={false}
                busy={runesBusy}
                error={runesError}
                onApply={(selection, presetIndex, spells, enemy) => void applyRunes(selection, presetIndex, spells, enemy)}
                onToggleAutoApply={(enabled) => void toggleAutoApply(enabled)}
                onToggleSpellsWithRunes={(enabled) => void toggleSpellsWithRunes(enabled)}
                onToggleImportItems={(enabled) => void toggleImportItems(enabled)}
                onPickSpell={(slot, spellId) => void pickSpell(slot, spellId)}
                onPositionChange={(nextPosition) => void loadRunes(nextPosition)}
                onImportItems={importItemBuild}
                onLoadProBuilds={loadProBuilds}
                onLoadBuild={loadKeystoneBuild}
                onLoadMatchup={loadMatchup}
                championApi={championApi}
                onTierChange={(tier) => void setTier(tier)}
              />
            ) : (
              <>
                <div className="remote-grid-wrap">
                  {catalogError && champions.length === 0 && <p className="remote-empty">Champion data is unavailable. Waiting for League to reconnect.</p>}
                  {!catalogError && champions.length === 0 && <p className="remote-empty">Loading champions…</p>}
                  {champions.length > 0 && filtered.length === 0 && <p className="remote-empty">No available champions match this filter.</p>}
                  <div className="remote-champion-grid">
                    {filtered.map((champion) => {
                      const disabled = !select || (!select.actionKind && !select.canPrepick) || !available.has(champion.id);
                      return <button
                        key={champion.id}
                        className={`remote-champion ${selected === champion.id ? "is-selected" : ""}`}
                        disabled={actionBusy || disabled}
                        onClick={() => chooseChampion(champion)}
                        aria-label={`${champion.name}${disabled ? ", unavailable" : ""}`}
                      >
                        <img src={`/api/champion/icon/${champion.id}`} alt="" loading="lazy" />
                        <span>{champion.name}</span>
                      </button>;
                    })}
                  </div>
                </div>

                {select?.actionKind && (
                  <div className="remote-picker-footer">
                    <span>{select.selectedChampionId ? champions.find((champion) => champion.id === select.selectedChampionId)?.name ?? "Selected" : "Select a champion"}</span>
                    <button disabled={actionBusy || !select.canComplete} onClick={() => void sendAction("/api/champion/lock")}>{select.actionKind === "ban" ? "Confirm ban" : "Lock in"}</button>
                  </div>
                )}
              </>
            )}
          </section>
        ) : (
          <section className="remote-waiting">
            {status?.lcuConnected && phase === "Matchmaking" ? (
              <>
                <p className="remote-eyebrow">MATCHMAKING</p>
                <h1>Searching for a match</h1>
                <p>Queue time</p>
                <strong className="remote-queue-time" role="timer">{game?.queueTimeSeconds == null ? "—:—" : formatQueueTime(game.queueTimeSeconds)}</strong>
                {game?.queueTimeSeconds == null && <p>League has not reported a queue timer yet.</p>}
              </>
            ) : status?.lcuConnected && phase === "ReadyCheck" ? (
              <>
                <p className="remote-eyebrow">READY CHECK</p>
                <h1>Match found</h1>
                {game?.readyCheck ? (
                  <>
                    <p>Accept the match to enter champion select.</p>
                    <button className="remote-accept" disabled={actionBusy} onClick={() => void sendAction("/api/ready-check/accept")}>Accept match</button>
                  </>
                ) : <p className="remote-confirmed">Accepted. Waiting for the other players.</p>}
              </>
            ) : status?.lcuConnected && phase === "Lobby" ? (
              <>
                <p className="remote-eyebrow">LEAGUE LOBBY</p>
                <h1>Waiting for queue</h1>
                <p>Start matchmaking in League. Queue time and match acceptance will appear here.</p>
              </>
            ) : status?.lcuConnected && phase === "ChampSelect" ? (
              <>
                <p className="remote-eyebrow">CHAMPION SELECT</p>
                <h1>Loading the draft</h1>
                <p>The champion list will appear when League sends the draft session.</p>
              </>
            ) : (
              <>
                <p className="remote-eyebrow">LEAGUE CONTROL</p>
                <h1>{status?.lcuConnected ? "Waiting for League" : "League is offline"}</h1>
                <p>{status?.lcuConnected ? "Open a lobby or start matchmaking in League." : "Start League Client on this PC. The controls will appear when it connects."}</p>
              </>
            )}
          </section>
        )}
      </main>
    </div>
  );
}
