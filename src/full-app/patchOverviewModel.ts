import { championIdFor, classifyChange, sectionId, type ChangeKind, type PatchSection } from "./PatchNotesModel.ts";

/** The anchor of a change card; matches the ids PatchNotes gives its cards. */
export const entryAnchor = (section: string, index: number) => `${sectionId(section)}-${index}`;

export type OverviewChampion = { anchor: string; name: string; championId: number | null; kind: ChangeKind };
export type OverviewLink = { anchor: string; name: string; itemId?: number | null };
export type PatchOverview = {
  nerfs: OverviewChampion[];
  buffs: OverviewChampion[];
  adjusted: OverviewChampion[];
  items: OverviewLink[];
  /** Other kept sections (Runes, Systems...), as section jumps. */
  systems: OverviewLink[];
};

/** Riot's "patch at a glance": champions grouped by verdict, then items and other sections. */
export function patchOverview(sections: PatchSection[], champions: { id: number; name: string }[]): PatchOverview {
  const overview: PatchOverview = { nerfs: [], buffs: [], adjusted: [], items: [], systems: [] };
  for (const section of sections) {
    if (section.title === "Champions") {
      section.entries.forEach((entry, i) => {
        if (!entry.name) return;
        const kind = classifyChange(entry);
        const champion = { anchor: entryAnchor(section.title, i), name: entry.name, championId: championIdFor(entry.name, champions), kind };
        (kind === "nerf" ? overview.nerfs : kind === "buff" ? overview.buffs : overview.adjusted).push(champion);
      });
    } else if (section.title === "Items") {
      section.entries.forEach((entry, i) => { if (entry.name) overview.items.push({ anchor: entryAnchor(section.title, i), name: entry.name, itemId: entry.itemId ?? null }); });
    } else if (!/bug ?fix/i.test(section.title)) {
      overview.systems.push({ anchor: sectionId(section.title), name: section.title });
    }
  }
  return overview;
}
