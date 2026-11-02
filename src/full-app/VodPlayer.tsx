import { useState } from "react";
import { TeamLogo } from "../runes/TeamLogo";
import { RankCrest } from "../runes/RankCrest";
import type { MatchupVod } from "./matchupGuide";
import { vodPlayerName } from "./matchupVodModel";

export function VodPlayer({ vod }: { vod: MatchupVod }) {
  const [failedImage, setFailedImage] = useState<string | null>(null);
  const name = vodPlayerName(vod.player);
  const image = name ? vod.playerImage?.trim() : undefined;
  const player = name ?? `${vod.rankClaim} replay`;
  const initials = player.trim().split(/\s+/).slice(0, 2).map(word => Array.from(word)[0] ?? "").join("").toUpperCase() || "?";
  return <span className="mv-player">
    <span className="mv-avatar" aria-hidden>{image && image !== failedImage
      ? <img src={image} alt="" onError={() => setFailedImage(image)} /> : name ? initials : <RankCrest tier={vod.rankClaim.toLowerCase()} size={40} mode="desktop" />}</span>
    <span className="mv-player-info"><strong className="mv-player-name" title={player}>{player}</strong>
      <span className="mv-affiliation">{vod.team
        ? <><TeamLogo key={vod.team} team={vod.team} size={20} mode="desktop" /><span>{vod.team}</span></>
        : name ? <><RankCrest key={vod.rankClaim} tier={vod.rankClaim.toLowerCase()} size={20} mode="desktop" /><span>{vod.rankClaim}</span></> : <span>Ranked match · player unlisted</span>}</span>
    </span>
  </span>;
}
