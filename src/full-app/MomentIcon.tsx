import { ArrowLeftRight, Bug, CircleDot, Crown, Eye, Flame, Footprints, Skull, Swords, TowerControl, Users, type LucideIcon } from "lucide-react";
import type { MomentKind } from "./matchupGuide";

const ICONS: Record<MomentKind, LucideIcon> = {
  kill: Swords, death: Skull, teamfight: Users, dragon: Flame, baron: Crown, herald: Eye,
  grubs: Bug, tower: TowerControl, trade: ArrowLeftRight, roam: Footprints, other: CircleDot,
};

/** Key-moment icon; moments without a known kind get the neutral dot. CSS colours it by `data-kind`. */
export function MomentIcon({ kind, size = 14 }: { kind?: MomentKind; size?: number }) {
  const Icon = (kind && ICONS[kind]) || CircleDot;
  return <Icon size={size} strokeWidth={2.2} aria-hidden className="mv-kind-icon" data-kind={kind ?? "none"} />;
}
