import { useMemo, useState } from "react";
import { ChevronDown } from "lucide-react";
import { ChampionIcon } from "../runes/ChampionIcon";
import { RoleIcon } from "../runes/RoleIcon";
import { ANSWERS, CURRENT_PATCH, championId, type Answer } from "./compsData";
import { type TeamPlayer } from "./compsModel";
import { ANSWER_TABS, filterRows, groupAnswers, partitionRowAnswers, runePageSummary, tabCounts, trendOf, whoPlays, type AnswerTab, type RankedAnswer, type ThreatRow } from "./answersModel";
import { RuneIcon } from "../runes/RuneIcon";
import { SpellIcon } from "../runes/SpellIcon";
import { presetName, presetSelection, type ProRunePage } from "./proRunePages";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ChampionArt } from "./MatchupStrip";
import type { VodRequest } from "./MatchupVodEntry";
import "./comps-answers.css";

const TAB_LABEL: Record<AnswerTab, string> = { top: "Top", jungle: "Jungle", mid: "Mid", bot: "Bot", support: "Support", draft: "Draft" };
const shortName = (p: TeamPlayer) => p.isSelf ? "You" : (p.riotId && p.riotId !== p.puuid ? p.riotId.split("#")[0] : "Player");
const profileIcon = (id: number) => `https://cdn.communitydragon.org/latest/profile-icon/${id}`;
const pct = (n: number) => `${Math.round(n * 100)}%`;
const fmt = (n: number) => n >= 1000 ? `${(n / 1000).toFixed(1)}k` : `${n}`;

function parseVod(url: string | null): { videoId: string; startSec?: number } | null {
  if (!url) return null;
  const id = url.match(/(?:youtu\.be\/|v=)([\w-]{6,})/)?.[1];
  if (!id) return null;
  const t = Number(url.match(/[?&]t=(\d+)/)?.[1]);
  return { videoId: id, ...(t ? { startSec: t } : {}) };
}


export function CompsAnswers({ team, onOpenVods }: { team: TeamPlayer[]; onOpenVods?: (r: VodRequest) => void }) {
  const [tab, setTab] = useState<AnswerTab>("mid");
  const [q, setQ] = useState("");
  const [openKey, setOpenKey] = useState<string | null>(null);
  const [selId, setSelId] = useState<string | null>(null);
  const [more, setMore] = useState<Set<string>>(new Set());
  const [hideRest, setHideRest] = useState(false);
  const tailored = team.some(p => p.included);
  const canJudge = team.some(p => p.pool && p.pool.length > 0);
  const hide = canJudge && hideRest;
  const counts = useMemo(() => tabCounts(ANSWERS, q, hide, team), [q, hide, team]);
  const allRows = useMemo(() => filterRows(groupAnswers(ANSWERS, tab), q, false, team), [tab, q, team]);
  const parts = useMemo(() => new Map(allRows.map(r => [r.key, partitionRowAnswers(r, team)])), [allRows, team]);
  const rows = hide ? allRows.filter(r => (parts.get(r.key)?.playable.length ?? 0) > 0) : allRows;
  const gone = hide ? allRows.filter(r => (parts.get(r.key)?.playable.length ?? 0) === 0) : [];
  const total = allRows.reduce((s, r) => s + r.answers.length, 0);
  const playableTotal = allRows.reduce((s, r) => s + (parts.get(r.key)?.playable.length ?? 0), 0);
  const elsewhere = ANSWER_TABS.filter(t => t !== tab && counts[t] > 0);

  return <>
    <div className="ans-bar">
      <div className="ans-roles" role="tablist" aria-label="Counter roles">
        {ANSWER_TABS.map(t => <span key={t} className="ans-role-wrap">{t === "draft" && <i aria-hidden="true" />}
          <button role="tab" aria-selected={tab === t} className={tab === t ? "on" : ""} onClick={() => { setTab(t); setOpenKey(null); setSelId(null); }}>
            {t !== "draft" && <RoleIcon role={t === "bot" ? "adc" : t} size={16} mode="desktop" />}{TAB_LABEL[t]}<em>{counts[t]}</em></button></span>)}
      </div>
      {canJudge && <button type="button" className="f-switch" role="switch" aria-checked={hideRest} onClick={() => setHideRest(!hideRest)}><i />Only counters we play<em>{playableTotal}</em></button>}
      <label className="ans-search"><input placeholder="Search a champion" value={q} onChange={e => setQ(e.target.value)} aria-label="Search threats and counters" />{q && <button type="button" onClick={() => setQ("")} aria-label="Clear search">✕</button>}</label>
    </div>
    <div className="ans-list">
      {rows.map(row => <Row key={row.key} row={row} team={team} tailored={tailored} canJudge={canJudge} hide={hide}
        playableIds={new Set((parts.get(row.key)?.playable ?? []).map(a => a.id))} hiddenCount={parts.get(row.key)?.rest.length ?? 0}
        open={openKey === row.key} selId={selId} showAll={more.has(row.key)} onOpenVods={onOpenVods}
        onToggle={() => { setOpenKey(openKey === row.key ? null : row.key); setSelId(null); }}
        onPick={(a) => { setOpenKey(row.key); setSelId(a.id); }}
        onMore={() => setMore(prev => new Set(prev).add(row.key))}
        onShowAll={() => setHideRest(false)} />)}
      {hide && gone.length > 0 && <div className="f-empty wide"><span><b>{gone.length} {TAB_LABEL[tab]} {gone.length === 1 ? "threat is" : "threats are"} hidden:</b> nobody on your roster plays a counter to {gone.map(r => r.threat.join(" + ")).join(" or ")}.</span><button type="button" className="cp-btn" onClick={() => setHideRest(false)}>Show all {total}</button></div>}
      {!rows.length && !gone.length && (q.trim()
        ? <div className="ans-empty ui-card"><span>⌕</span><h3>No threat or counter matches “{q.trim()}”{elsewhere.length ? ` in ${TAB_LABEL[tab]}` : ""}</h3>
            <p>{elsewhere.length ? "It shows up under another tab:" : "Counters use pro data from the last 4 patches. Try another champion."}</p>
            <div>{elsewhere.map(t => <button key={t} type="button" className="cp-btn primary" onClick={() => setTab(t)}>{TAB_LABEL[t]} · {counts[t]}</button>)}<button type="button" className="cp-btn" onClick={() => setQ("")}>Clear search</button></div></div>
        : <div className="ans-empty ui-card"><span>◇</span><h3>No {TAB_LABEL[tab]} counters yet</h3><p>Counters cover champions with pro counter data from the last 4 patches.</p></div>)}
    </div>
    <p className="cp-muted">Pro: Leaguepedia, patches up to {CURRENT_PATCH}, lane opponents with 8+ games and 55%+ win rate; draft counters picked after the threat at 1.5×+ their usual rate.</p>
  </>;
}

function Row({ row, team, tailored, canJudge, hide, playableIds, hiddenCount, open, selId, showAll, onToggle, onPick, onMore, onShowAll, onOpenVods }: {
  row: ThreatRow; team: TeamPlayer[]; tailored: boolean; canJudge: boolean; hide: boolean; playableIds: Set<string>; hiddenCount: number; open: boolean; selId: string | null; showAll: boolean;
  onToggle: () => void; onPick: (a: RankedAnswer) => void; onMore: () => void; onShowAll: () => void; onOpenVods?: (r: VodRequest) => void;
}) {
  const shown = hide ? row.answers.filter(a => playableIds.has(a.id)) : row.answers;
  const visible = showAll ? shown : shown.slice(0, 3);
  const cur = open ? row.answers.find(a => a.id === selId) ?? visible[0] ?? row.answers[0] : null;
  const draft = row.tab === "draft";
  return <article className={`ans-row ui-card${open ? " open" : ""}`}>
    <div className="ans-head" onClick={onToggle}>
      <div className={`ans-threat${row.threat.length > 1 ? " duo" : ""}`}>
        {row.threat.map(n => <span key={n} className="ans-art"><ChampionArt id={championId(n)} name={n} kind="splash" /></span>)}
        <em>{draft ? "Draft threat" : `${TAB_LABEL[row.tab]} threat`}</em><strong>{row.threat.join(" + ")}</strong>
        <small>{draft ? `${row.answers.length} draft counters` : `${row.answers.length} counters · top counter ${row.answers[0]?.games ?? 0} games`}</small>
      </div>
      <div className="ans-tiles">{visible.map(a => <Tile key={a.id} row={row} a={a} team={team} tailored={tailored} dim={canJudge && !hide && !playableIds.has(a.id)} selected={cur?.id === a.id} onPick={x => cur?.id === x.id ? onToggle() : onPick(x)} />)}
        {!showAll && shown.length > 3 && <button type="button" className="ans-more" onClick={e => { e.stopPropagation(); onMore(); }}>+{shown.length - 3} more</button>}
        {hide && hiddenCount > 0 && <button type="button" className="f-hid" onClick={e => { e.stopPropagation(); onShowAll(); }}>+{hiddenCount} hidden · show</button>}
      </div>
      <span className="ans-chev" aria-hidden="true"><ChevronDown size={18} /></span>
    </div>
    {open && cur && <Detail row={row} a={cur} team={team} onOpenVods={onOpenVods} />}
  </article>;
}

function Tile({ row, a, team, tailored, dim, selected, onPick }: { row: ThreatRow; a: RankedAnswer; team: TeamPlayer[]; tailored: boolean; dim: boolean; selected: boolean; onPick: (a: RankedAnswer) => void }) {
  const { active, subs } = whoPlays(a, team);
  const trend = trendOf(a);
  const draft = row.tab === "draft";
  const glyph = trend.dir === "up" ? "▲" : trend.dir === "down" ? "▼" : "●";
  return <button type="button" className={`ans-tile${tailored && active.length ? " mine" : tailored && subs.length ? " sub" : ""}${dim ? " dim" : ""}${selected ? " sel" : ""}${a.olderMeta ? " old" : ""}`}
    onClick={e => { e.stopPropagation(); onPick(a); }}>
    <span className="ans-ics"><span className="ans-rank">{a.rank}</span>{a.answer.map(n => <ChampionIcon key={n} id={championId(n)} name={n} mode="desktop" />)}</span>
    <span className="ans-name"><strong>{a.answer.join(" + ")}</strong><small>{a.source === "soloq" ? <><b className="ans-src">SoloQ</b> {fmt(a.games)} games</> : `${a.games} pro games${a.olderMeta ? ` · older meta ${a.patch}` : ""}`}</small></span>
    <span className="ans-stat"><b>{draft && a.lift ? `${a.lift}×` : pct(a.winRate)}</b><small>{draft ? "picked after" : "win rate"} <span className={trend.dir}>{glyph}</span></small></span>
    {tailored && <span className="ans-team">{[...active.map(w => [w, false] as const), ...subs.map(w => [w, true] as const)].map(([w, sub]) => {
        const label = sub ? `${shortName(w.player)} (sub)` : shortName(w.player);
        return w.player.profileIconId != null
          ? <img key={w.player.puuid} className={sub ? "sub" : undefined} src={profileIcon(w.player.profileIconId)} alt={label} title={label} />
          : <i key={w.player.puuid} className={sub ? "sub" : undefined} title={label}>{shortName(w.player).charAt(0)}</i>;
      })}</span>}
  </button>;
}

/** The rune page pros actually ran on the answer champion, with a one-click preset save (desktop only). */
function ProRunes({ a, threat }: { a: RankedAnswer; threat: string[] }) {
  const summary = runePageSummary(a);
  // Save state per page: 0 = most used, 1 = runner-up.
  const [save, setSave] = useState<Record<number, "saving" | "saved" | "error">>({});
  const [saveError, setSaveError] = useState("");
  if (!summary) return null;
  const { primary, runnerUp, totalGames } = summary;
  const icons = (page: ProRunePage, cls: string) => <>
    <RuneIcon id={page.runes[0]} mode="desktop" className={`${cls} key`} />
    <span className="xa-runes-set">{page.runes.slice(1, 4).map(id => <RuneIcon key={id} id={id} mode="desktop" className={cls} />)}</span>
    <span className="xa-runes-set">{page.runes.slice(4, 6).map(id => <RuneIcon key={id} id={id} mode="desktop" className={cls} />)}</span>
    <span className="xa-runes-set shards">{page.runes.slice(6, 9).map(id => <RuneIcon key={id} id={id} mode="desktop" className={cls} />)}</span>
    <span className="xa-runes-set">{page.spells.map(id => <SpellIcon key={id} id={id} mode="desktop" className={cls} />)}</span>
  </>;
  const onSave = (page: ProRunePage, i: number) => {
    setSave(prev => ({ ...prev, [i]: "saving" })); setSaveError("");
    void invoke("save_rune_preset", { championId: championId(a.answer[0]), name: presetName(threat.join(" + ") + (i ? " alt" : "")), selection: presetSelection(page) })
      .then(() => setSave(prev => ({ ...prev, [i]: "saved" })))
      .catch(() => { setSave(prev => ({ ...prev, [i]: "error" })); setSaveError("Couldn't save that preset. Try again."); });
  };
  const saveButton = (page: ProRunePage, i: number) => !isTauri() ? null : save[i] === "saved"
    ? <span className="xa-runes-saved">Saved as preset</span>
    : <button type="button" className="cp-btn" disabled={save[i] === "saving"} onClick={() => onSave(page, i)}>{save[i] === "saving" ? "Saving…" : "Save as preset"}</button>;
  return <div className="xa-runes">
    <div className="xa-runes-head"><h4>Pro rune page</h4><small>{a.answer[0]} in this matchup</small></div>
    <div className="xa-runes-main">{icons(primary, "xa-rune")}
      <span className="xa-runes-note"><b>used in {primary.games} of {totalGames} games · {primary.wins} wins</b>
        {saveButton(primary, 0)}
        {Object.values(save).includes("error") && <span className="xa-runes-err" role="alert">{saveError || "Couldn't save that preset. Try again."}</span>}
      </span></div>
    {runnerUp && <div className="xa-runes-alt">{icons(runnerUp, "xa-rune sm")}<span className="xa-runes-note"><small>also used in {runnerUp.games} games · {runnerUp.wins} wins</small>{saveButton(runnerUp, 1)}</span></div>}
  </div>;
}

function Detail({ row, a, team, onOpenVods }: { row: ThreatRow; a: RankedAnswer; team: TeamPlayer[]; onOpenVods?: (r: VodRequest) => void }) {
  const draft = row.tab === "draft";
  const solo = a.source === "soloq";
  const wins = Math.round(a.games * a.winRate);
  const trend = trendOf(a);
  const glyph = trend.dir === "up" ? "▲" : trend.dir === "down" ? "▼" : "●";
  const games = a.proGames.slice(0, 4);
  const vodGame = games.find(g => parseVod(g.vod));
  const vod = vodGame ? parseVod(vodGame.vod) : null;
  const { active, subs } = whoPlays(a, team);
  const people = [...active.map(w => ({ ...w, on: true })), ...subs.map(w => ({ ...w, on: false }))];
  const maxTrend = Math.max(...a.trend.map(t => t.winRate), 0.01);
  const pipCount = Math.min(a.games, 24);
  const pipWins = Math.round(pipCount * a.winRate);
  const playVod = (g: Answer["proGames"][number]) => {
    const v = parseVod(g.vod);
    if (!v || !onOpenVods) return;
    onOpenVods({ champion: null, play: { videoId: v.videoId, title: g.teams, rankClaim: "Pro play", patch: g.patch, durationSec: 0, startSec: v.startSec, moments: [] } });
  };
  return <div className="ans-detail">
    <div className="xa-l">
      <div className="xa-top"><h3>{a.answer.join(" + ")} {draft ? "after" : "into"} {row.threat.join(" + ")}</h3>
        <span className="xa-kinds"><span className={`cp-kind${draft ? " draft" : ""}`}>{draft ? "Draft counter" : "Lane counter"}</span><span className="cp-kind">{solo ? "SoloQ" : "Pro play"} · Patch {a.patch}</span>{a.olderMeta && <span className="cp-kind">older meta</span>}</span></div>
      <div className="xa-stats">
        <div><b className="cp-win">{draft && a.lift ? `${a.lift}×` : pct(a.winRate)}</b><small>{draft ? "picked after, vs usual" : solo ? "win rate" : "win rate"}</small></div>
        {solo ? <div><b>{fmt(a.games)}</b><small>SoloQ games</small></div>
          : <div><span className="xa-pips" title={`${wins} wins, ${a.games - wins} losses`}>{Array.from({ length: pipCount }, (_, i) => <i key={i} className={i < pipWins ? "" : "l"} />)}</span><small>{wins} wins, {a.games - wins} losses in pro play</small></div>}
        <div><span className="ans-bars">{a.trend.map((t, i) => <i key={t.patch} className={i === a.trend.length - 1 ? "now" : ""} style={{ height: `${Math.max(8, (t.winRate / maxTrend) * 100)}%` }} title={`${t.patch}: ${pct(t.winRate)}`} />)}</span><small><span className={trend.dir}>{glyph}</span> {trend.label} over 4 patches</small></div>
      </div>
      <p className="xa-lead">{a.why || "Why this works arrives with the data pass."}</p>
      {a.tips.length ? <ol className="xa-tips">{a.tips.map(t => <li key={t}>{t}</li>)}</ol> : <p className="cp-muted">Tips for this matchup arrive with the data pass.</p>}
      <ProRunes a={a} threat={row.threat} />
    </div>
    <div className="xa-r">
      {vodGame && vod && onOpenVods && <button type="button" className="xa-thumb" onClick={() => playVod(vodGame)}>
        <img src={`https://i.ytimg.com/vi/${vod.videoId}/mqdefault.jpg`} alt="" loading="lazy" /><span className="play">▶</span>
        <span className="cap"><strong>Watch this matchup</strong><small>{vodGame.teams} · opens in the in-app player{vod.startSec ? ` at ${Math.floor(vod.startSec / 60)}:${String(vod.startSec % 60).padStart(2, "0")}` : ""}</small></span></button>}
      <h4>Pro games <small>{solo ? "" : `${a.games} on record`}</small></h4>
      {solo ? <p className="cp-muted">No pro counter to {row.threat.join(" + ")} on the last 4 patches. These numbers are SoloQ.</p>
        : <div>{games.map(g => <div key={g.gameId} className="ans-game"><span className={`ans-res ${g.result === "Win" ? "w" : "l"}`}>{g.result}</span><span><strong>{g.teams}</strong><small>{g.tournament} · Patch {g.patch}</small></span>{g.vod && onOpenVods ? <button type="button" className="cp-link" onClick={() => playVod(g)}>▶ VOD</button> : null}</div>)}</div>}
    </div>
    <div className="xa-foot"><h4>On your roster</h4>
      {people.length ? <span className="xa-who">{people.map(w => <span key={`${w.player.puuid}-${w.champion}`} className={`xa-chip${w.on ? " on" : ""}`} title={w.player.riotId}>
        {w.player.profileIconId != null ? <img src={profileIcon(w.player.profileIconId)} alt="" /> : <i>{shortName(w.player).charAt(0)}</i>}
        {shortName(w.player)}<small>{w.entry.masteryPoints ? `${Math.round(w.entry.masteryPoints / 1000)}k` : "—"}{w.entry.masteryLevel ? ` · M${w.entry.masteryLevel}` : ""}</small></span>)}</span>
        : <span className="cp-muted">Nobody on your roster plays {a.answer.join(" or ")} yet</span>}
    </div>
  </div>;
}
