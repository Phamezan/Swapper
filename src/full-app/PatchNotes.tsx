import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChampionIcon } from "../runes/ChampionIcon";
import { ItemIcon } from "../runes/ItemIcon";
import type { ChampionOption } from "../runes/types";
import { loadIcon } from "../runes/iconLoader";
import { useChampionArt } from "./art";
import type { Abilities } from "./cooldownModel";
import { RefreshIcon, WrenchIcon } from "./ui/icons";
import { EmptyState, Notice, Skeleton } from "./ui/States";
import {
  abilityHeading, bugfixLines, championIdFor, championIdInText, changeSegments, classifyChange, formatPatchDate, patchCounts, riftSections, sectionId,
  type ChangeKind, type PatchAbility, type PatchEntry, type PatchNotes as Notes,
} from "./PatchNotesModel";
import "./patch-notes.css";
import { PatchOverview } from "./PatchOverview";

const KIND_LABEL: Record<ChangeKind, string> = { buff: "Buff", nerf: "Nerf", adjust: "Adjustment" };
const KIND_SYMBOL: Record<ChangeKind, string> = { buff: "↗", nerf: "↘", adjust: "↔" };
type Checkpoint = { id: string; label: string; championId: number | null; kind?: ChangeKind; itemIds?: number[] };
const entryId = (section: string, index: number) => `${sectionId(section)}-${index}`;

export default function PatchNotes() {
  const [notes, setNotes] = useState<Notes | null>(null);
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  const [error, setError] = useState("");
  const [refreshing, setRefreshing] = useState(false);
  const mounted = useRef(false);
  const page = useRef<HTMLDivElement>(null);

  const load = useCallback(async (force: boolean, version?: string) => {
    setRefreshing(true);
    try {
      const value = await invoke<Notes>("get_latest_patch_notes", { force, version });
      if (mounted.current) {
        setNotes({ ...value, sections: riftSections(value.sections) });
        setError("");
      }
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      if (mounted.current) setRefreshing(false);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void load(false);
    void invoke<ChampionOption[]>("get_champion_list").then(list => {
      if (mounted.current) setChampions(list);
    }).catch(() => {});
    return () => { mounted.current = false; };
  }, [load]);
  useEffect(() => {
    page.current?.closest<HTMLElement>(".dashboard-main")?.scrollTo({ top: 0, behavior: "instant" });
  }, [notes?.patch]);

  const counts = useMemo(() => notes ? patchCounts(notes) : null, [notes]);
  const checkpoints = useMemo(() => (notes?.sections ?? []).flatMap<Checkpoint>(section =>
    section.title === "Champions"
      ? section.entries.map((entry, i) => ({ id: entryId(section.title, i), label: entry.name,
        championId: championIdFor(entry.name, champions), kind: classifyChange(entry) }))
      : [{ id: sectionId(section.title), label: section.title, championId: null,
        itemIds: section.title === "Items" ? section.entries.flatMap(entry => entry.itemId ?? []).slice(0, 4) : undefined }]
  ), [notes, champions]);

  if (!notes) return <div className="pn-page">
    {error ? <EmptyState title="Patch notes unavailable"><p>{error}</p>
      <button className="pn-refresh" onClick={() => void load(true)} disabled={refreshing}>Try again</button>
    </EmptyState> : <Skeleton label="Loading patch notes" rows={3} height={170} />}
  </div>;

  const date = formatPatchDate(notes.date);
  return <div ref={page} className="pn-page">
    <header className="pn-header">
      <div className="pn-header-copy">
        <div className="pn-patch-line"><h2>Patch {notes.patch}</h2>{date && <time className="pn-edition" dateTime={notes.date ?? undefined}>{date}</time>}</div>
        <p className="pn-summary">
          {counts && <span className="pn-summary-counts">
            <span className="is-buff">{counts.buffs} buffs</span><span className="is-nerf">{counts.nerfs} nerfs</span><span>{counts.adjusts} adjustments</span>
          </span>}
        </p>
      </div>
      <div className="pn-header-actions">
        <button type="button" className="pn-refresh" aria-label="Refresh patch notes" title="Refresh patch notes" onClick={() => void load(true, notes.patch)} disabled={refreshing}>
          <RefreshIcon size={16} aria-hidden />
        </button>
      </div>
    </header>
    <div className="pn-patch-switch" role="group" aria-label="Patch version">
      {notes.availablePatches.map(({ version }, i) => <PatchCard key={version} version={version} current={i === 0}
        selected={version === notes.patch} disabled={refreshing} onSelect={() => { if (version !== notes.patch) void load(false, version); }} />)}
    </div>
    {notes.stale && <Notice tone="warn">Showing the last downloaded copy. Live notes could not be refreshed.</Notice>}
    {error && <Notice tone="warn">Could not refresh patch notes. {error}</Notice>}

    <div key={notes.patch} className="pn-layout">
      <div className="pn-content">
        <PatchOverview sections={notes.sections} champions={champions} />
        {notes.sections.map(section => {
          if (section.title === "Bugfixes") return <details key={section.title} id={sectionId(section.title)} className="pn-bugfixes" tabIndex={-1}>
            <summary>Bugfixes</summary>
            <ul className="pn-bugfix-list">{bugfixLines(section).map((text, i) => {
              const championId = championIdInText(text, champions);
              return <li key={i}>{championId != null
                ? <ChampionIcon id={championId} mode="desktop" />
                : <WrenchIcon size={18} className="pn-bugfix-wrench" />}<span>{text}</span></li>;
            })}</ul>
          </details>;
          const body = <>
            {section.paragraphs.map((para, i) => <p key={i} className="pn-prose">{para}</p>)}
            <div className="pn-cards">{section.entries.map((entry, i) => <EntryCard key={i} id={entryId(section.title, i)}
              entry={entry} championId={section.title === "Champions" ? championIdFor(entry.name, champions) : null}
              itemId={section.title === "Items" ? entry.itemId : null} />)}</div>
          </>;
          return <section key={section.title} id={sectionId(section.title)} className="pn-section" aria-labelledby={`${sectionId(section.title)}-title`} tabIndex={-1}>
              <h2 id={`${sectionId(section.title)}-title`} className="pn-section-title">{section.title}<span>{section.entries.filter(e => e.name).length || ""}</span></h2>
              {body}
            </section>;
        })}
        {!notes.sections.length && <EmptyState title="No Rift changes found">Read Riot’s full notes for this patch.</EmptyState>}
        {notes.sections.length > 0 && <p className="pn-end">You’re up to date. <span>See you on the Rift.</span></p>}
      </div>
      {checkpoints.length > 0 && <ScrollRail checkpoints={checkpoints} />}
    </div>
  </div>;
}

/** One patch in the picker: Riot's key art for that patch behind its number. */
function PatchCard({ version, current, selected, disabled, onSelect }: { version: string; current: boolean; selected: boolean; disabled: boolean; onSelect: () => void }) {
  const [art, setArt] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    void invoke<string | null>("patch_banner", { version }).then(url => { if (live) setArt(url); }).catch(() => {});
    return () => { live = false; };
  }, [version]);
  return <div className="pn-patch-option">
    <span className={`pn-patch-label${selected ? " is-selected" : ""}`} aria-hidden>{current ? "Current" : "Previous"}</span>
    <button type="button" className={`pn-patch-card${art ? " has-art" : ""}`} aria-pressed={selected} disabled={disabled} onClick={onSelect}
      aria-label={`Patch ${version}, ${current ? "Current" : "Previous"}`} style={art ? { "--pn-art": `url("${art}")` } as CSSProperties : undefined}>
      {art && <span className="pn-patch-art" aria-hidden />}
      <strong>{version}</strong>
    </button>
  </div>;
}

function ScrollRail({ checkpoints }: { checkpoints: Checkpoint[] }) {
  const rail = useRef<HTMLElement>(null);
  const marker = useRef<HTMLSpanElement>(null);
  const [active, setActive] = useState(0);
  useEffect(() => {
    const nav = rail.current;
    const page = nav?.closest<HTMLElement>(".pn-page");
    const scroller = nav?.closest<HTMLElement>(".dashboard-main");
    const track = nav?.querySelector<HTMLElement>(".pn-rail-track");
    const viewport = nav?.querySelector<HTMLElement>(".pn-rail-viewport");
    if (!page || !scroller || !track || !viewport) return;
    const targets = checkpoints.map(point => document.getElementById(point.id));
    const buttons = Array.from(track.querySelectorAll<HTMLButtonElement>("button"));
    let frame = 0;
    let lastActive = -1;
    const update = () => {
      frame = 0;
      const readingLine = scroller.getBoundingClientRect().top + scroller.clientTop + scroller.clientHeight * .4;
      const tops = targets.map(target => (target?.getBoundingClientRect().top ?? Infinity) - readingLine);
      let index = 0;
      tops.forEach((top, i) => { if (top <= 0) index = i; });
      const atStart = scroller.scrollTop <= 1;
      if (atStart) index = 0;
      else if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 2) index = checkpoints.length - 1;
      const fraction = !atStart && index < tops.length - 1 ? Math.max(0, Math.min(1, -tops[index] / Math.max(1, tops[index + 1] - tops[index]))) : 0;
      const start = buttons[index].offsetTop + buttons[index].offsetHeight / 2;
      const next = buttons[index + 1];
      const y = start + (next ? next.offsetTop + next.offsetHeight / 2 - start : 0) * fraction;
      if (marker.current) marker.current.style.transform = `translateY(${y - 5}px)`;
      if (index !== lastActive) {
        setActive(index);
        lastActive = index;
      }
      // Keep the current checkpoint in view even on patches with many champions.
      if (y < viewport.scrollTop + 12 || y > viewport.scrollTop + viewport.clientHeight - 12) {
        viewport.scrollTop = y - viewport.clientHeight / 2;
      }
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(update); };
    const resize = new ResizeObserver(schedule);
    resize.observe(page);
    resize.observe(scroller);
    scroller.addEventListener("scroll", schedule, { passive: true });
    schedule();
    return () => { resize.disconnect(); scroller.removeEventListener("scroll", schedule); cancelAnimationFrame(frame); };
  }, [checkpoints]);

  const jump = (point: Checkpoint, keyboard: boolean) => {
    const target = document.getElementById(point.id);
    const scroller = rail.current?.closest<HTMLElement>(".dashboard-main");
    if (!target || !scroller) return;
    if (target instanceof HTMLDetailsElement) target.open = true;
    if (keyboard) target.focus({ preventScroll: true });
    const top = scroller.scrollTop + target.getBoundingClientRect().top - scroller.getBoundingClientRect().top - scroller.clientTop - scroller.clientHeight * .27;
    scroller.scrollTo({ top: Math.max(0, Math.min(scroller.scrollHeight - scroller.clientHeight, top)),
      behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
  };
  return <nav ref={rail} className="pn-rail" aria-label="Navigate patch changes">
    <p className="pn-rail-heading">In this patch</p>
    <div className="pn-rail-viewport"><div className="pn-rail-track">
      <span ref={marker} className="pn-rail-marker" aria-hidden />
      {checkpoints.map((point, i) => <button key={point.id} type="button" className={`pn-checkpoint${i === active ? " is-current" : ""}`}
        aria-label={`Read ${point.label} changes`} aria-controls={point.id} aria-current={i === active ? "location" : undefined}
        title={point.label} onClick={event => jump(point, event.detail === 0)}>
        <CheckpointArt point={point} />
        <span className="pn-checkpoint-name">{point.label}</span>
        <i className={`pn-checkpoint-tick${point.kind ? ` is-${point.kind}` : ""}`} aria-hidden />
      </button>)}
    </div></div>
    <p className="pn-rail-position"><span>{String(active + 1).padStart(2, "0")}</span> / {String(checkpoints.length).padStart(2, "0")}</p>
  </nav>;
}

function CheckpointArt({ point }: { point: Checkpoint }) {
  const art = useChampionArt(point.championId, "tile");
  return <span className="pn-checkpoint-art" aria-hidden>
    {point.itemIds?.length ? <span className="pn-item-mosaic">{point.itemIds.map(id => <ItemIcon key={id} id={id} name="" mode="desktop" />)}</span>
      : art ? <img src={art} alt="" draggable={false} />
      : point.championId != null ? <ChampionIcon id={point.championId} name={point.label} mode="desktop" />
      : <span className="pn-section-mark">{point.label === "Bugfixes" ? <WrenchIcon size={18} /> : point.label.slice(0, 1)}</span>}
  </span>;
}

function EntryCard({ id, entry, championId, itemId }: { id: string; entry: PatchEntry; championId: number | null; itemId?: number | null }) {
  const art = useChampionArt(championId, "splash");
  const [abilities, setAbilities] = useState<PatchAbility[]>([]);
  const keys = entry.groups.map(group => abilityHeading(group.title)?.key);
  const hasSpells = keys.some(key => key && key !== "P");
  const hasPassive = keys.includes("P");
  useEffect(() => {
    let live = true;
    setAbilities([]);
    if (championId == null || (!hasSpells && !hasPassive)) return;
    const spells = hasSpells ? invoke<PatchAbility[]>("full_app_abilities", { championId }).catch(() => []) : Promise.resolve([]);
    // full_app_abilities contains Q/W/E/R; the existing sheet supplies passives.
    const passive = hasPassive ? invoke<Abilities>("champion_abilities", { championId }).then(async sheet => ({
      key: "P", name: sheet.passive.name,
      icon: sheet.passive.iconPath ? await loadIcon(`ability:${sheet.passive.iconPath}`,
        () => invoke<string>("ability_icon", { path: sheet.passive.iconPath })).catch(() => null) : null,
    })).catch(() => null) : Promise.resolve(null);
    void Promise.all([spells, passive]).then(([spells, passive]) => {
      if (live) setAbilities(passive ? [...spells, passive] : spells);
    });
    return () => { live = false; };
  }, [championId, hasSpells, hasPassive]);
  const kind = classifyChange(entry);
  return <article id={id} className={`pn-entry is-${kind}${championId != null ? " is-champion" : ""}`} tabIndex={-1} aria-label={entry.name || "Bugfixes"}>
    {(entry.name || entry.context) && <header className="pn-entry-head">
      {art && <img className="pn-entry-art" src={art} alt="" draggable={false} />}
      <div className="pn-entry-identity">
        {championId != null && <ChampionIcon id={championId} name={entry.name} mode="desktop" />}
        {championId == null && itemId != null && <ItemIcon id={itemId} name={entry.name} mode="desktop" />}
        {championId == null && itemId == null && <span className="pn-entry-letter" aria-hidden>{entry.name.slice(0, 1)}</span>}
        <h3>{entry.name}</h3>
        {entry.name && <span className={`pn-tag is-${kind}`}><span aria-hidden>{KIND_SYMBOL[kind]}</span> {KIND_LABEL[kind]}</span>}
      </div>
      {entry.context && <p className="pn-context">{entry.context}</p>}
    </header>}
    <div className="pn-entry-body">
      {entry.groups.map((group, gi) => {
        const ability = abilityHeading(group.title, abilities);
        return <div key={gi} className="pn-group">
          {group.title && <h4>{ability?.icon && <img className="pn-ability-icon" src={ability.icon} alt="" draggable={false} />}<span>{ability?.text ?? group.title}</span></h4>}
          <ul>{group.lines.map((line, li) => <li key={li} className="pn-line">
            {line.label && <span className="pn-line-label">{line.label}</span>}
            {line.before != null || line.after != null ? <span className="pn-change">
              {line.before != null && <span className="pn-before"><span className="pn-sr">Before: </span>{line.before}</span>}
              {line.before != null && line.after != null && <span className="pn-arrow" aria-hidden>→</span>}
              {line.after != null && <strong className="pn-after"><span className="pn-sr">After: </span>{changeSegments(line).map((part, i) =>
                <span key={i} className={`pn-number is-${part.tone}`} title={part.tone === "buff" ? "Better" : part.tone === "nerf" ? "Worse" : undefined}>{part.text}</span>
              )}</strong>}
            </span> : <span className="pn-line-text">{line.text}</span>}
          </li>)}</ul>
        </div>;
      })}
      {entry.notes.length > 0 && <details className="pn-extra"><summary>More context from Riot</summary>{entry.notes.map((note, i) => <p key={i} className="pn-note">{note}</p>)}</details>}
    </div>
  </article>;
}
