import { useEffect, useRef, useState } from "react";
import { ArrowRight, Search } from "lucide-react";
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

export default function RemoteApp() {
  const [status, setStatus] = useState<RemoteStatus | null>(null);
  const [live, setLive] = useState(false);
  const [game, setGame] = useState<GameSnapshot | null>(null);
  const [champions, setChampions] = useState<Champion[]>([]);
  const [catalogError, setCatalogError] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [position, setPosition] = useState("ALL");
  const [showUnavailable, setShowUnavailable] = useState(false);
  const [prepickId, setPrepickId] = useState<number | null>(null);
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
        setLive(true);
      };
      socket.onmessage = (event) => {
        if (cancelled) return;
        try {
          const payload = JSON.parse(String(event.data)) as { type?: string; status?: RemoteStatus };
          if (payload.type === "status" && payload.status) setStatus(payload.status);
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

  const ready = live && status?.state === "available" && status.tailscaleRunning && status.lcuConnected;
  const phase = game?.phase;
  const select = game?.championSelect;
  const showCatalog = Boolean(status?.lcuConnected && phase === "ChampSelect" && select);
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
      <header className="remote-topbar">
        <div className="remote-topline">
          <div className="remote-brand">
            <span className="remote-mark"><ArrowRight size={15} /><ArrowRight size={15} className="remote-mark-flip" /></span>
            <div className="remote-brand-copy"><strong>Swapper</strong><span>REMOTE</span></div>
          </div>
          <span className={`remote-ready ${ready ? "is-ready" : "is-bad"}`}>
            <span className="remote-ready-dot" />{ready ? "Ready" : "Check connection"}
          </span>
        </div>
        <div className="remote-status-line">
          <span className={status?.tailscaleRunning && live ? "is-good" : "is-bad"}><i />Tailscale {status?.tailscaleRunning && live ? "connected" : "offline"}</span>
          <span className={status?.lcuConnected ? "is-good" : "is-bad"}><i />League {status?.lcuConnected ? "connected" : "offline"}</span>
        </div>
      </header>

      <main className={`remote-main ${showCatalog ? "has-catalog" : ""}`}>
        {status?.message && <p className="remote-inline-error" role="alert">{status.message}</p>}
        {game?.message && <p className="remote-inline-error" role="alert">{game.message}</p>}
        {actionError && <p className="remote-inline-error" role="alert">{actionError}</p>}

        {showCatalog ? (
          <section className="remote-picker" aria-label="Champion catalog">
            <div className="remote-picker-head">
              <p className="remote-eyebrow">CHAMPION SELECT</p>
              <h1>{title}</h1>
              <p>{select?.actionKind === "ban" ? "Choose a champion to ban, then confirm." : select?.actionKind === "pick" ? "Select a champion, then lock in." : select?.canPrepick ? "Choose a champion to show your pick intent in League." : "The champion list is ready for your next turn."}</p>
            </div>

            <div className="remote-picker-tools">
              <label className="remote-search"><Search size={17} /><input type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search champions" aria-label="Search champions" /></label>
              <div className="remote-roles" role="group" aria-label="Filter champions by position">
                {positions.map((entry) => <button key={entry.value} className={position === entry.value ? "is-active" : ""} onClick={() => setPosition(entry.value)}>{entry.label}</button>)}
              </div>
              <div className="remote-availability">
                <span>{available.size} available for {select?.actionKind === "ban" ? "banning" : "picking"}</span>
                <button onClick={() => setShowUnavailable((value) => !value)}>{showUnavailable ? "Hide unavailable" : "Show all champions"}</button>
              </div>
            </div>

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
