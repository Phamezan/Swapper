import { RankCrest } from "../runes/RankCrest";
import { crestForTier } from "./buildLogic";
import type { Mmr } from "./types";

const titleCase = (value: string) => value.charAt(0) + value.slice(1).toLowerCase();

/** Swapper's MMR estimate: the average Solo/Duo rank of recent lobbies, with its trend. */
export function MmrLine({ mmr, explain }: { mmr: Mmr; explain: string }) {
  return <div className="mmr-line" title={explain}>
    <span className="mmr-line-label">MMR<small>est.</small></span>
    <span className="mmr-line-rank"><RankCrest tier={crestForTier(mmr.tier)} size={24} mode="desktop" />{`${titleCase(mmr.tier)} ${mmr.division}`.trim()}<b>{mmr.lp} LP</b></span>
    {mmr.trend ? <span className={`mmr-line-trend ${mmr.trend > 0 ? "up" : "down"}`}>{mmr.trend > 0 ? "▲" : "▼"} {Math.abs(mmr.trend)} LP</span> : null}
  </div>;
}
