import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ChevronRight } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ChampionIcon } from "../runes/ChampionIcon";
import type { ChampionOption } from "../runes/types";
import { GuideSourceButton } from "./GuideSourceButton";
import { useChampionArt } from "./art";
import { loadMatchupVods } from "./loadMatchupVods";
import { GUIDE_SECTIONS, type GuideSectionId, type MatchupVod } from "./matchupGuide";
import { championsInMoment, formatVodTime, vodUrl } from "./matchupVodModel";
import { MomentIcon } from "./MomentIcon";
import { VodPlayer } from "./VodPlayer";
import { YoutubeVodPlayer, type VodSeek } from "./YoutubeVodPlayer";
import { Skeleton } from "./ui/States";
import "./matchup-vods.css";

function VodArt({ id }: { id: number }) {
  const art = useChampionArt(id, "splash");
  return art ? <img src={art} alt="" /> : <span className="mv-art-fallback" aria-hidden />;
}

function VodFacts({ vod }: { vod: MatchupVod }) {
  return <span className="mv-facts">{vod.result && <span className={`mv-result is-${vod.result === "W" ? "win" : "loss"}`} aria-label={vod.result === "W" ? "Win" : "Loss"}>{vod.result}</span>}
    <span>Patch {vod.patch}</span><span>{formatVodTime(vod.durationSec)}</span><span>{vod.moments.length} key moments</span>
  </span>;
}

export function MatchupVods({ champion, enemy, lane, onGuideSection, active = true, onReopen, onPlaybackChange }: {
  champion: ChampionOption; enemy: ChampionOption; lane: string; onGuideSection: (section: GuideSectionId) => void;
  active?: boolean; onReopen?: () => void;
  onPlaybackChange?: (open: boolean) => void;
}) {
  const [response, setResponse] = useState<{ key: string; vods: MatchupVod[] } | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [seek, setSeek] = useState<VodSeek | null>(null);
  const [activeMoment, setActiveMoment] = useState(-1);
  const [champions, setChampions] = useState<ChampionOption[]>([]);
  useEffect(() => {
    let live = true;
    void invoke<ChampionOption[]>("get_champion_list").then(list => { if (live) setChampions(list); }).catch(() => {});
    return () => { live = false; };
  }, []);
  const momentList = useRef<HTMLOListElement>(null);
  const lastOpened = useRef("");
  const sourceRow = useRef<HTMLButtonElement>(null);
  const backButton = useRef<HTMLButtonElement>(null);
  const key = `${champion.id}-${enemy.id}-${lane}`;
  useEffect(() => {
    let live = true;
    void loadMatchupVods(champion.id, enemy.id, lane).then(vods => { if (live) setResponse({ key, vods }); })
      .catch(() => { if (live) setResponse({ key, vods: [] }); });
    return () => { live = false; };
  }, [champion.id, enemy.id, lane, key]);
  useEffect(() => {
    if (!active) return;
    if (selectedId) backButton.current?.focus({ preventScroll: true });
    else sourceRow.current?.focus();
  }, [selectedId, active]);
  const vods = response?.key === key ? response.vods : null;
  const selected = vods?.find(vod => vod.videoId === selectedId);
  const playable = !!selected && !!vodUrl(selected);
  // Keep the playing moment in view by scrolling the feed only, never the page around the video.
  useEffect(() => {
    const list = momentList.current, row = list?.children[activeMoment] as HTMLElement | undefined;
    if (!list || !row || list.scrollHeight <= list.clientHeight) return;
    if (row.offsetTop < list.scrollTop || row.offsetTop + row.offsetHeight > list.scrollTop + list.clientHeight)
      list.scrollTo({ top: row.offsetTop - (list.clientHeight - row.offsetHeight) / 2, behavior: "smooth" });
  }, [activeMoment]);
  useEffect(() => {
    onPlaybackChange?.(playable);
    return () => onPlaybackChange?.(false);
  }, [playable, onPlaybackChange]);
  if (!active && !selectedId) return null;
  if (!vods) return <Skeleton label="Loading matchup VODs" rows={2} height={70} />;
  if (!vods.length) return <p className="mg-no-written-guide">No VODs for this matchup yet</p>;
  if (!selected) return <div className="mv-library">
    <header className="mv-list-toolbar"><h2>{vods.length} games as {champion.name}</h2>{vods.some(vod => vod.fixture) && <span className="mv-preview">Preview data</span>}</header>
    <ul className="mv-vod-list" aria-label={`${champion.name} matchup VODs`}>{vods.map(vod => <li key={vod.videoId}>
      <button type="button" className="mv-vod-row" data-vod-id={vod.videoId} ref={vod.videoId === lastOpened.current ? sourceRow : undefined} aria-label={`Watch ${vod.player ?? vod.rankClaim}: ${vod.title}`} onClick={() => { lastOpened.current = vod.videoId; setSelectedId(vod.videoId); }}>
        <VodPlayer vod={vod} />
        <span className="mv-row-description"><span className="mv-row-title">{vod.title}</span><VodFacts vod={vod} /></span>
        <span className="mv-thumbnail"><VodArt id={champion.id} /></span>
      </button>
    </li>)}</ul>
  </div>;
  return <div className={`mv-player-view${active ? "" : " is-mini"}`} aria-label={active ? "VOD player" : "Mini player"} onKeyDown={event => {
    if (event.key !== "Escape" || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault(); event.stopPropagation(); setSelectedId(null); setSeek(null);
  }}>
    <button type="button" className="mv-back" ref={backButton} onClick={() => { setSelectedId(null); setSeek(null); }}><ArrowLeft size={16} aria-hidden />All VODs</button>
    <div className="mv-mini-actions"><button type="button" onClick={onReopen}>Reopen VOD</button><button type="button" aria-label="Close mini player" onClick={() => { setSelectedId(null); setSeek(null); }}>Close</button></div>
    <div className="mv-player-layout">
      <article className="mv-watch" aria-label="Selected VOD">
        <YoutubeVodPlayer key={selected.videoId} vod={selected} seek={seek} mini={!active} onMomentChange={setActiveMoment} />
        {vodUrl(selected) && <GuideSourceButton url={vodUrl(selected)!} className="mv-youtube-link" label="Open on YouTube">Open on YouTube</GuideSourceButton>}
        <div className="mv-description"><VodPlayer key={selected.videoId} vod={selected} /><h2 className="mv-title">{selected.title}</h2>
          <VodFacts vod={selected} />
        </div>
      </article>
      <section className="mv-chapters" aria-label="Key moments"><header><h3>Key moments</h3><small>{selected.moments.length} moments</small></header>
          <ol className="mv-moments" ref={momentList}>{selected.moments.map((moment, i) => {
            const section = GUIDE_SECTIONS.find(([id]) => id === moment.section);
            const portraits = championsInMoment(moment.label, champions);
            return <li key={`${selected.videoId}-${i}`} className={i === activeMoment ? "is-active" : undefined}>
            <button type="button" disabled={!playable} className="mv-moment" aria-current={i === activeMoment || undefined} aria-label={`Watch ${moment.label} at ${formatVodTime(moment.atSec)}`} onClick={() => setSeek({ moment, serial: (seek?.serial ?? 0) + 1 })}>
              <MomentIcon kind={moment.kind} size={24} /><span className="mv-moment-time">{formatVodTime(moment.atSec)}</span>
              <span className="mv-moment-kind">{moment.kind && moment.kind !== "other" ? moment.kind : "Key moment"}</span>
              <span className="mv-moment-body">{portraits.length > 0 && <span className="mv-portraits" aria-hidden>{portraits.map(named => <ChampionIcon key={named.id} id={named.id} name={named.name} mode="desktop" />)}</span>}<span className="mv-moment-label">{moment.label}</span></span>
            </button>
            {section && <button type="button" className="mv-section-link" onClick={() => onGuideSection(section[0])} aria-label={`Open ${section[1]} guide section`}>{section[1]}<ChevronRight size={12} aria-hidden /></button>}
          </li>; })}</ol>
      </section>
    </div>
  </div>;
}
