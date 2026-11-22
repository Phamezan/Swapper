export type LibraryMod = {
  id: string;
  name: string;
  author: string;
  version: string;
  category: string | null;
  champion: string | null;
  slotLabel: string;
  enabled: boolean;
  source?: string | null;
  licence?: string | null;
  pageUrl?: string | null;
  runeforgeModId?: string | null;
  /** LTK Manager's health check: "healthy", "repairable", "unrepairable", or null before one ran. */
  health?: string | null;
  fatals?: number;
  errors?: number;
};

/** LTK's check found a problem: fatal findings or no fix ("broken"), or fixable findings ("repairable"). */
export type ModHealthState = { broken: boolean; repairable: boolean };

export function modHealth(mod: LibraryMod): ModHealthState | null {
  const broken = (mod.fatals ?? 0) > 0 || mod.health === "unrepairable";
  const repairable = mod.health === "repairable";
  return broken || repairable ? { broken, repairable } : null;
}

/** Turning this on asks first: LTK found fatal errors for the current patch. */
export function needsEnableConfirm(mod: LibraryMod): boolean {
  return (mod.fatals ?? 0) > 0;
}

export type ModsView = {
  mods: LibraryMod[];
  regionBlocked: boolean;
  inGame: boolean;
  appliesNextGame: boolean;
  /** A change waits for the user to close LTK Manager. */
  needsClose: boolean;
};

/** Korea/China gating, mirroring the backend check on the active account. */
export function isRegionBlocked(platform: string | null, region: string | null): boolean {
  const hit = (value: string | null) => {
    if (!value) return false;
    const v = value.trim().toUpperCase();
    return v === "KR" || v === "CN" || v.startsWith("CN") || v === "KOREA" || v === "CHINA";
  };
  return hit(platform) || hit(region);
}

const NEXT_GAME = "Applies next game.";
const CLOSE_TO_APPLY = "Close LTK Manager to apply this.";

/** What a change waits for, shown after it. */
export function changeNotice(view: ModsView): string | null {
  if (view.appliesNextGame) return NEXT_GAME;
  if (view.needsClose) return CLOSE_TO_APPLY;
  return null;
}

/** The notice after a new view: what a change waits for, else the previous
 *  notice (e.g. a slot conflict) unless that was a wait that is now over. */
export function nextNotice(prev: string | null, view: ModsView): string | null {
  return changeNotice(view) ?? (prev === NEXT_GAME || prev === CLOSE_TO_APPLY ? null : prev);
}

/** Short line in a list row (the author has its own line). */
export function modMeta(mod: LibraryMod): string {
  const parts = [mod.slotLabel, mod.version ? `v${mod.version}` : ""].filter(Boolean);
  if (mod.licence) parts.push(mod.licence);
  return parts.join(" · ");
}

// ---- RuneForge browse ----

export type RfMod = {
  id: string;
  slug: string;
  name: string;
  author: string;
  category: string | null;
  champions: string[];
  status: string | null;
  thumbnailUrl: string | null;
  downloadCount: number;
  viewCount: number;
  likeCount: number;
  publishedAt: string | null;
  updatedAt: string | null;
  licence: string | null;
  pageUrl: string;
  tags: string[];
};

/** `runeforge_mod` result: the list fields plus the preview images (full URLs, in display order). */
export type RfDetail = RfMod & { gallery: string[] };

export type RfChampion = { id: number; name: string };

export type RfList = {
  mods: RfMod[];
  total: number;
  page: number;
  champions: RfChampion[];
};

export type BrowseSort = "trending" | "downloads" | "newest" | "updated";

/** RuneForge category values with short labels (All = no filter). */
export const RF_CATEGORIES: { value: string; label: string }[] = [
  { value: "", label: "All" },
  { value: "champion_skin", label: "Skins" },
  { value: "map_skin", label: "Maps" },
  { value: "ui", label: "HUD" },
  { value: "sfx", label: "Sounds" },
  { value: "vfx", label: "VFX" },
  { value: "font", label: "Fonts" },
  { value: "loading_screen", label: "Loading screens" },
];

export function categoryLabel(value: string | null): string {
  if (!value) return "Other";
  return RF_CATEGORIES.find((c) => c.value === value)?.label ?? value;
}

export type BrowseQuery = {
  page: number;
  search: string;
  category: string;
  championId: number | null;
  sort: BrowseSort;
};

/** Args for the `runeforge_list` command. */
export function browseArgs(query: BrowseQuery): Record<string, unknown> {
  return {
    page: Math.max(1, query.page),
    search: query.search.trim(),
    category: query.category || null,
    championId: query.championId,
    sort: query.sort,
  };
}

/** "Working on current patch only": keep mods RuneForge marks working. */
export function visibleMods(mods: RfMod[], workingOnly: boolean): RfMod[] {
  if (!workingOnly) return mods;
  return mods.filter((m) => (m.status ?? "").toLowerCase() === "working");
}

/** The library mod installed from RuneForge mod `mod`, if any. */
export function installedMod(mod: RfMod, library: LibraryMod[]): LibraryMod | undefined {
  return library.find((m) => m.runeforgeModId === mod.id);
}

export function isInstalled(mod: RfMod, library: LibraryMod[]): boolean {
  return installedMod(mod, library) !== undefined;
}

/** RuneForge flags it as not working on the current patch. */
export function notWorking(mod: RfMod): boolean {
  return Boolean(mod.status) && mod.status!.toLowerCase() !== "working";
}

/** "Nov 11, 2026" (UTC, so it never shifts a day), or "" for a missing or bad date. */
export function formatModDate(iso: string | null): string {
  const time = iso ? Date.parse(iso) : NaN;
  if (Number.isNaN(time)) return "";
  return new Date(time).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric", timeZone: "UTC" });
}

/** Placeholder tile letter for a mod without a thumbnail. */
export function modInitial(name: string): string {
  return (name.trim().match(/[\p{L}\p{N}]/u)?.[0] ?? "?").toUpperCase();
}

// ---- Library filters ----

export type LibraryTab = "all" | "on" | "off" | "clashes" | "problems";

export type LibraryFilter = {
  search: string;
  /** LTK tag, "" = any. */
  category: string;
  /** Champion name, "" = any. */
  champion: string;
  tab: LibraryTab;
};

/** Mods sharing an exclusive slot (one skin per champion, one map, HUD and
 *  loading screen) with another mod in the library. "Other" never clashes. */
export function clashIds(mods: LibraryMod[]): Set<string> {
  const bySlot = new Map<string, string[]>();
  for (const m of mods) {
    const key = m.slotLabel.toLowerCase();
    if (key === "other") continue;
    bySlot.set(key, [...(bySlot.get(key) ?? []), m.id]);
  }
  return new Set([...bySlot.values()].filter((ids) => ids.length > 1).flat());
}

export function libraryCounts(mods: LibraryMod[]): Record<LibraryTab, number> {
  const on = mods.filter((m) => m.enabled).length;
  return { all: mods.length, on, off: mods.length - on, clashes: clashIds(mods).size, problems: mods.filter((m) => modHealth(m)).length };
}

/** LTK tags read like RuneForge categories ("champion-skin" is "Skins"). */
export function libraryCategoryLabel(tag: string): string {
  const value = tag.toLowerCase().replace(/-/g, "_");
  return categoryLabel(value === "hud" ? "ui" : value);
}

const byText = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

/** The distinct categories and champions in the library, for the filters. */
export function libraryOptions(mods: LibraryMod[]): { categories: string[]; champions: string[] } {
  const distinct = (values: (string | null)[]) => [...new Set(values.filter((v): v is string => Boolean(v)))].sort(byText);
  return { categories: distinct(mods.map((m) => m.category)), champions: distinct(mods.map((m) => m.champion)) };
}

/** The mods a filter shows, by name. Search matches name or author. */
export function filterLibrary(mods: LibraryMod[], filter: LibraryFilter): LibraryMod[] {
  const needle = filter.search.trim().toLowerCase();
  const clashes = filter.tab === "clashes" ? clashIds(mods) : null;
  return mods
    .filter((m) => !needle || m.name.toLowerCase().includes(needle) || m.author.toLowerCase().includes(needle))
    .filter((m) => !filter.category || m.category === filter.category)
    .filter((m) => !filter.champion || m.champion === filter.champion)
    .filter((m) => {
      switch (filter.tab) {
        case "all": return true;
        case "on": return m.enabled;
        case "off": return !m.enabled;
        case "clashes": return clashes!.has(m.id);
        case "problems": return modHealth(m) !== null;
      }
    })
    .sort((a, b) => byText(a.name, b.name));
}
