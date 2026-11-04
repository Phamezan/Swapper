import { useState } from "react";
import { TeamLogo } from "../runes/TeamLogo";
import { RankCrest } from "../runes/RankCrest";
import type { MatchupVod } from "./matchupGuide";
import { vodPlayerName, crestTier } from "./matchupVodModel";
import { usePlayerPhoto } from "./PlayerPhoto";

/** One-line identity: photo, initials or rank crest, the name and a pro's team. Rank and region sit in the facts beside it. */
export function VodPlayer({ vod }: { vod: MatchupVod }) {
  const [failedImage, setFailedImage] = useState<string | null>(null);
  const name = vodPlayerName(vod.player);
  const image = usePlayerPhoto(name);
  const player = name ?? `${vod.rankClaim} replay`;
  const initials = player.trim().split(/\s+/).slice(0, 2).map(word => Array.from(word)[0] ?? "").join("").toUpperCase() || "?";
  return <span className="mv-player">
    <span className="mv-avatar" aria-hidden>{image && image !== failedImage
      ? <img src={image} alt="" onError={() => setFailedImage(image)} /> : name ? initials : crestTier(vod.rankClaim) ? <RankCrest tier={crestTier(vod.rankClaim)!} size={26} mode="desktop" /> : null}</span>
    <strong className="mv-player-name" title={player}>{player}</strong>
    {vod.team && <span className="mv-affiliation"><TeamLogo key={vod.team} team={vod.team} size={16} mode="desktop" /><span>{vod.team}</span></span>}
    {vod.format === "stream" && <span className="mv-stream-tag">Streamer{vod.cam && <span className="mv-cam-dot" role="img" aria-label="Facecam" title="Facecam" />}</span>}
  </span>;
}
