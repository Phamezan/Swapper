import { useState } from "react";
import { TeamLogo } from "../runes/TeamLogo";
import { RankCrest } from "../runes/RankCrest";
import type { MatchupVod } from "./matchupGuide";
import { vodPlayerName, crestTier, playerInitials, teamName } from "./matchupVodModel";
import { usePlayerPhoto } from "./PlayerPhoto";
import { Tooltip } from "./ui/Tooltip";

/** One-line identity: photo, initials or rank crest, the name and a pro's team. Rank and region sit in the facts beside it. */
export function VodPlayer({ vod }: { vod: MatchupVod }) {
  const [failedImage, setFailedImage] = useState<string | null>(null);
  const name = vodPlayerName(vod.player);
  const image = usePlayerPhoto(name);
  const player = name ?? `${vod.rankClaim} replay`;
  const initials = playerInitials(player);
  return <span className="mv-player">
    <span className="mv-avatar" aria-hidden>{image && image !== failedImage
      ? <img src={image} alt="" onError={() => setFailedImage(image)} /> : name ? initials : crestTier(vod.rankClaim) ? <RankCrest tier={crestTier(vod.rankClaim)!} size={26} mode="desktop" /> : null}</span>
    <Tooltip label={player}><strong className="mv-player-name">{player}</strong></Tooltip>
    {vod.team && <span className="mv-affiliation"><TeamLogo key={vod.team} team={teamName(vod.team)} size={16} mode="desktop" /><span>{vod.team}</span></span>}
    {vod.format === "stream" && <span className="mv-stream-tag">Streamer{vod.cam && <Tooltip label="Facecam"><span className="mv-cam-dot" role="img" aria-label="Facecam" /></Tooltip>}</span>}
  </span>;
}
