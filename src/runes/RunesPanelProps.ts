import type { RunesView, Selection, ShopNotice, ProBuildsView, PresetBuildView, MatchupView, ChampionApi } from "./types";

export type RunesPanelProps = {
  mode: "desktop" | "remote";
  view: RunesView | null;
  loading: boolean;
  busy: boolean;
  error: string | null;
  /** Shop-import result from the `items_imported` event;
   *  separate from the rune-apply confirmation, which is the applied badge. */
  shopNotice?: ShopNotice | null;
  onApply: (
    selection: Selection,
    presetIndex: number | null,
    spells: number[] | null,
    enemyChampionId?: number | null,
    highestWin?: boolean,
  ) => void | Promise<void>;
  onToggleAutoApply: (enabled: boolean) => void;
  onToggleSpellsWithRunes: (enabled: boolean) => void;
  onToggleImportItems: (enabled: boolean) => void;
  onPickSpell: (slot: "d" | "f", spellId: number) => void;
  onPositionChange: (position: string) => void;
  onImportItems: (
    championId: number,
    championName: string,
    items: number[],
  ) => Promise<void>;
  /** Loads one page of pros' solo-queue games for the champion and role. */
  onLoadProBuilds: (
    championId: number,
    position: string,
    page: number,
    otp: boolean,
  ) => Promise<ProBuildsView>;
  /** Loads the 6-item build for one preset's keystone, from lolalytics. */
  onLoadBuild: (
    championId: number,
    position: string,
    tier: string,
    keystone: number,
    mostPlayed?: Selection,
  ) => Promise<PresetBuildView | null>;
  /** Loads the lane matchup build against the enemy laner, from lolalytics. */
  onLoadMatchup: (
    championId: number,
    enemyChampionId: number,
    position: string,
    tier: string,
  ) => Promise<MatchupView | null>;
  /** Loaders for the Champion tab. */
  championApi: ChampionApi;
  onTierChange: (tier: string) => void;
  /** Changes when a game ends, so the Champion tab drops its search. */
  sessionKey: string;
};

