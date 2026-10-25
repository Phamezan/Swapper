/** Patch notes as `runes::patch_notes` serializes them, plus the pure helpers
 *  the page shares with its test. No React or Tauri imports. */

export type PatchLine = { label: string | null; before: string | null; after: string | null; text: string | null };
export type PatchGroup = { title: string | null; lines: PatchLine[] };
export type PatchEntry = { name: string; itemId?: number | null; context: string | null; groups: PatchGroup[]; notes: string[] };
export type PatchSection = { title: string; entries: PatchEntry[]; paragraphs: string[] };
export type PatchNotes = {
  patch: string; title: string; description: string; date: string | null;
  /** Current, then previous, each with the listing's key art when Riot has one. */
  availablePatches: { version: string; bannerUrl: string | null }[];
  bannerUrl: string | null; articleUrl: string; intro: string[];
  sections: PatchSection[]; stale: boolean; fetchedAt: number | null;
};

export type ChangeKind = "buff" | "nerf" | "adjust";
export type ChangeSegment = { text: string; tone: "buff" | "nerf" | "neutral" };
export type PatchAbility = { key: string; name: string; icon: string | null };

/** Client metadata first, then Riot's heading, then the key alone. */
export function abilityHeading(title: string | null, abilities: PatchAbility[] = []) {
  const match = title?.trim().match(/^(Passive|[PQWER])(?:\s*[-–—]\s*(.*))?$/i);
  if (!match) return null;
  const normalizeKey = (key: string) => key.toUpperCase() === "PASSIVE" ? "P" : key.toUpperCase();
  const key = normalizeKey(match[1]);
  const ability = abilities.find(value => normalizeKey(value.key) === key);
  const name = ability?.name.trim() || match[2]?.trim();
  const label = key === "P" ? "Passive" : key;
  return { key, text: name ? `${label} - ${name}` : label, icon: ability?.icon || null };
}

const LOWER_IS_BETTER = /\b(?:cooldowns?|costs?|cast(?:ing)? time|wind[ -]?up|delays?|recharges?|lockouts?|time to)\b/i;
const HIGHER_IS_BETTER = /\b(?:damage|ad|ap|ratios?|heal(?:s|ing)?|shield(?:s|ing)?|health|mana|energy|armou?r|magic resist(?:ance)?|mr|speed|range|duration|slow|stun|growth|scaling|monsters?|camps?|mod(?:ifier)?s?|bonus|regeneration|regen|penetration|lethality|crit|critical strike|radius|width|area|lifesteal|omnivamp|tenacity|haste|ability power)\b/i;
const ENEMY_AMPLIFICATION = /\b(?:(?:increased|bonus) damage taken|damage amplification|amplified|vulnerability)\b/i;
const AMBIGUOUS_STAT = /\b(?:penalty|decay|falloff|channel|requirement|required)\b/i;

/** Stack counts need their purpose; null leaves ordinary stat/divisor rules in charge. */
function stackDirection(line: PatchLine): -1 | 0 | 1 | null {
  const text = [line.label, line.before, line.after].filter(Boolean).join(" ");
  const perYield = /\bper\s+(?:stack|soul|mark|chime)\b/i.test(text);
  // An explicit numeric denominator is not itself a stack count or yield label.
  const counts = text.replace(/\bper\s+[+−-]?\s*(?:\d[\d,]*(?:\.\d+)?|\.\d+)%?\s+stacks?\b/gi, "");
  if (!/\bstack(?:s|ing|ed)?\b/i.test(counts)) return perYield ? 1 : null;
  if (/\bmax(?:imum)?\s+stacks?\b/i.test(counts)) return 0;
  const required = /\bstacks?\s+(?:required|needed|to\s+(?:unlock|empower|transform|evolve))\b/i.test(counts)
    || /\b(?:thresholds?|checkpoints?)\b/i.test(counts)
    || /\bat\s+\d[\d\s/.,–—−-]*\s+stacks?\b/i.test(counts);
  const earned = perYield || /\bstacks?\s+(?:gained|granted|per\s+(?:kill|cannon|minion|hit|takedown)s?)\b/i.test(counts);
  return required === earned ? 0 : required ? -1 : 1;
}

function statDirection(line: PatchLine): -1 | 0 | 1 {
  const stacks = stackDirection(line);
  if (stacks != null) return stacks;
  const label = line.label;
  if (!label) return 0;
  if (/\brefund\b/i.test(label)) return 1;
  if (/\b(?:charges(?:\s+(?:count|limit|capacity))?|charge\s+(?:count|limit|capacity))(?:\s+(?:per|at|based on)\b.*)?$/i.test(label) || AMBIGUOUS_STAT.test(label)) return 0;
  if (/\bself\b/i.test(label) && !/\bself[ -]+(?:heal(?:ing)?|shield(?:ing)?|damage)\b/i.test(label)) return 0;
  const threshold = /\bthreshold\b/i.test(label);
  const executeThreshold = threshold && /\bexecute\b/i.test(label);
  if (threshold && !executeThreshold) return 0;
  const amplification = ENEMY_AMPLIFICATION.test(label);
  if (/\bdamage taken\b/i.test(label) && !amplification && !/\breduction\b/i.test(label)) return 0;
  const lower = LOWER_IS_BETTER.test(label) || /\bself[ -]+damage\b/i.test(label);
  const higher = HIGHER_IS_BETTER.test(label) || executeThreshold || amplification;
  if (lower && higher && /[/&]|\b(?:and|or)\b/i.test(label)) return 0;
  if (lower) return /\b(?:reduction|refund|discount)\b/i.test(label) ? 1 : -1;
  return higher ? 1 : 0;
}

function numericValues(text: string) {
  const numbers: { text: string; index: number; value: number; divisor: boolean }[] = [];
  let shape = "", cursor = 0;
  for (const match of text.matchAll(/\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d+(?:\.\d+)?|\.\d+/g)) {
    let index = match.index!;
    const prefix = text.slice(0, index);
    const sign = prefix.match(/[+−-]\s*$/);
    // A dash following another value is a range separator, not a negative sign.
    if (sign && !/[\d%)]/.test(prefix.slice(0, sign.index).trimEnd().slice(-1))) index -= sign[0].length;
    const number = text.slice(index, match.index! + match[0].length);
    numbers.push({ text: number, index, value: Number(number.replace(/[\s,]/g, "").replace("−", "-")),
      divisor: /\bper\s+$/i.test(text.slice(0, index)) });
    shape += text.slice(cursor, index) + "#";
    cursor = match.index! + match[0].length;
  }
  shape += text.slice(cursor);
  return { numbers, shape: shape.toLowerCase().replace(/\bseconds?\b/g, "s").replace(/[–—−]/g, "-").replace(/\s+/g, "") };
}

function matchingShapes(before: string, after: string): boolean {
  if (before === after) return true;
  // Numeric parentheses are formulas, so only text-only notes may be added/removed.
  const stripNotes = (shape: string) => shape.replace(/\([^()#]*\)/g, "");
  if (stripNotes(before) !== stripNotes(after)) return false;
  const beforeNotes = before.match(/\([^()#]*\)/g) ?? [];
  const afterNotes = after.match(/\([^()#]*\)/g) ?? [];
  const contains = (notes: string[], subset: string[]) => {
    const remaining = [...notes];
    return subset.every(note => {
      const index = remaining.indexOf(note);
      if (index < 0) return false;
      remaining.splice(index, 1);
      return true;
    });
  };
  return contains(beforeNotes, afterNotes) || contains(afterNotes, beforeNotes);
}

const SCALING_QUALIFIER = /\blevel[\s-]+based\b|\bscales?\s+with\b|\bbased\s+on\s+level\b/i;

/** Simple slash ranks may share a trailing unit or repeat it on each rank. */
function rankUnit(shape: string): string | null {
  const units = shape.split("/").map(rank => rank.match(/^#([a-z%]*)$/)?.[1]);
  if (units.some(unit => unit == null)) return null;
  const unit = units.find(Boolean) ?? "";
  return units.every(value => !value || value === unit) ? unit : null;
}

function compareChange(line: PatchLine) {
  const direction = statDirection(line);
  if (!direction || line.before == null || line.after == null) return null;
  const before = numericValues(line.before), after = numericValues(line.after);
  const beforeUnit = rankUnit(before.shape), afterUnit = rankUnit(after.shape);
  const broadcast = before.numbers.length !== after.numbers.length && Math.min(before.numbers.length, after.numbers.length) === 1
    && beforeUnit != null && beforeUnit === afterUnit;
  if (!before.numbers.length || (!broadcast && (before.numbers.length !== after.numbers.length || !matchingShapes(before.shape, after.shape)))
    || /,|#e[+-]?#|##/.test(after.shape)
    || [...before.numbers, ...after.numbers].some(number => !Number.isFinite(number.value))) return null;
  const deltas = Array.from({ length: Math.max(before.numbers.length, after.numbers.length) }, (_, index) => {
    const old = before.numbers[before.numbers.length === 1 ? 0 : index];
    const next = after.numbers[after.numbers.length === 1 ? 0 : index];
    return (next.value - old.value) * direction * (next.divisor ? -1 : 1);
  });
  const improved = deltas.some(delta => delta > 0), worsened = deltas.some(delta => delta < 0);
  return {
    before, after, deltas,
    // A single After represents every old rank, so its colour uses their consensus.
    afterDeltas: after.numbers.length === 1 ? [improved === worsened ? 0 : improved ? 1 : -1] : deltas,
    scaling: Number(SCALING_QUALIFIER.test(line.after)) - Number(SCALING_QUALIFIER.test(line.before)),
  };
}

/** Preserve the after text and colour every comparable number independently. */
export function changeSegments(line: PatchLine): ChangeSegment[] {
  const text = line.after ?? "";
  const comparison = compareChange(line);
  if (!comparison) return [{ text, tone: "neutral" }];
  const segments: ChangeSegment[] = [];
  let cursor = 0;
  comparison.after.numbers.forEach((number, index) => {
    if (number.index > cursor) segments.push({ text: text.slice(cursor, number.index), tone: "neutral" });
    const delta = comparison.afterDeltas[index];
    segments.push({ text: number.text, tone: delta > 0 ? "buff" : delta < 0 ? "nerf" : "neutral" });
    cursor = number.index + number.text.length;
  });
  if (cursor < text.length) segments.push({ text: text.slice(cursor), tone: "neutral" });
  return segments;
}

function lineSignals(line: PatchLine) {
  const comparison = compareChange(line);
  if (!comparison) return [];
  const { before, after, deltas, scaling } = comparison;
  const range = deltas.length >= 2 && /^#(?:%|s)?-#/.test(before.shape) && /^#(?:%|s)?-#/.test(after.shape)
    && before.numbers[0].value <= before.numbers[1].value && after.numbers[0].value <= after.numbers[1].value;
  // The upper endpoint decides a range; independent ratios and scaling still contribute.
  const signals = range ? [deltas[1] || deltas[0], ...deltas.slice(2), scaling] : [...deltas, scaling];
  return signals.filter(delta => delta !== 0).map(delta => delta > 0 ? "buff" : "nerf");
}

/** The reader's single content boundary: Rift gameplay, including mixed bugfix lists.
 * Unknown editorial sections stay in the linked Riot article. */
export function riftSections(sections: PatchSection[]): PatchSection[] {
  const modes = /\b(?:aram|mayhem|arena|urf|arurf|nexus blitz|swarm|doom bots|ultimate spellbook|tft|teamfight tactics)\b/i;
  const gameplay = (text: string | null) => !modes.test(text ?? "");
  const buckets = new Map<string, PatchSection>();
  for (const section of sections) {
    const title = section.title.toLowerCase().replace(/[’']/g, "").trim();
    if (!gameplay(title)) continue;
    let category: string;
    if (/^champions?(?: (?:balance )?changes)?$/.test(title)) category = "Champions";
    else if (/^items?(?: (?:balance )?changes)?$/.test(title)) category = "Items";
    else if (/^runes?(?: (?:balance )?changes)?$/.test(title)) category = "Runes";
    else if (/^(?:(?:core )?systems?|summoners rift|summoner spells|jungle|monsters|objectives|turrets|minions)(?: changes| adjustments| updates)?$/.test(title)) category = "Systems";
    else if (/^bug\s?fixes?(?:\b|$)/.test(title)) category = "Bugfixes";
    else continue;

    const entries = section.entries.filter(entry => gameplay(entry.name)).map(entry => ({
      ...entry,
      context: gameplay(entry.context) ? entry.context : null,
      notes: entry.notes.filter(gameplay),
      groups: entry.groups.filter(group => gameplay(group.title)).map(group => ({
        ...group,
        lines: group.lines.filter(line => gameplay([line.label, line.before, line.after, line.text].filter(Boolean).join(" "))),
      })).filter(group => group.lines.length > 0),
    })).filter(entry => entry.groups.length > 0 || entry.notes.length > 0);
    const paragraphs = section.paragraphs.filter(gameplay);
    if (!entries.length && !paragraphs.length) continue;
    const bucket = buckets.get(category) ?? { title: category, entries: [], paragraphs: [] };
    bucket.entries.push(...entries);
    bucket.paragraphs.push(...paragraphs);
    buckets.set(category, bucket);
  }
  return ["Champions", "Items", "Runes", "Systems", "Bugfixes"].flatMap(title => buckets.get(title) ?? []);
}

/** Riot's explicit wording takes priority; otherwise aggregate line signals,
 * including range endpoints and scaling notes. Mixed or unknown means Adjustment. */
export function classifyChange(entry: PatchEntry): ChangeKind {
  // "Reverting the buff" is a nerf and "reverting the nerf" a buff: flip those before reading the wording.
  const text = (entry.context ?? "").toLowerCase().replace(/\brevert\w*\b(?:(?!\band\b)[^.,;]){0,30}?\b(buff|nerf)/g, (_, word) => word === "buff" ? "nerf" : "buff");
  const buff = /\bbuff/.test(text);
  const nerf = /\bnerf/.test(text);
  if (buff && !nerf) return "buff";
  if (nerf && !buff) return "nerf";
  // Mixed or no wording (e.g. "with the Hubris nerf … a couple buffs"): count the lines. Each line is a buff
  // or a nerf by its own numbers (a line pulling both ways counts as neither); the majority decides, a tie is an adjustment.
  let score = 0;
  for (const line of entry.groups.flatMap(group => group.lines)) {
    const tones = new Set(lineSignals(line));
    if (tones.size === 1) score += tones.has("buff") ? 1 : -1;
  }
  return score > 0 ? "buff" : score < 0 ? "nerf" : "adjust";
}

const normalize = (name: string) => name.toLowerCase().replace(/[^a-z0-9]/g, "");
const ALIASES: Record<string, string> = { monkeyking: "wukong" };

/** The champion list id for a patch-notes entry name, when it is a champion. */
export function championIdFor(name: string, champions: { id: number; name: string }[]): number | null {
  const wanted = ALIASES[normalize(name)] ?? normalize(name);
  return champions.find(c => normalize(c.name) === wanted)?.id ?? null;
}

/** Full champion names in prose, with possessives and punctuation normalized. */
export function championIdInText(text: string, champions: { id: number; name: string }[]): number | null {
  const words = (value: string) => value.toLowerCase().replace(/[’']s\b/g, "").replace(/[’']/g, "").replace(/[^a-z0-9]+/g, " ").trim();
  const prose = ` ${words(text)} `;
  const name = [...champions.map(champion => champion.name), ...Object.keys(ALIASES)].find(name => prose.includes(` ${words(name)} `));
  return name ? championIdFor(name, champions) : null;
}

export function bugfixLines(section: PatchSection): string[] {
  return [...section.paragraphs, ...section.entries.flatMap(entry => {
    const lines = [entry.context, ...entry.groups.flatMap(group => group.lines.map(line =>
      line.text ?? [line.label, [line.before, line.after].filter(value => value != null).join(" → ")].filter(Boolean).join(": ")
    )), ...entry.notes].filter((text): text is string => Boolean(text?.trim()));
    return lines.map(text => entry.name ? `${entry.name}: ${text}` : text);
  })].filter(text => text.trim());
}

export type PatchCounts = { buffs: number; nerfs: number; adjusts: number; champions: number; items: number; lines: number };

/** Summary counts from the reader's filtered sections. */
export function patchCounts(notes: PatchNotes): PatchCounts {
  const counts: PatchCounts = { buffs: 0, nerfs: 0, adjusts: 0, champions: 0, items: 0, lines: 0 };
  for (const section of notes.sections) {
    const kind = section.title.trim().toLowerCase();
    for (const entry of section.entries) {
      counts.lines += entry.groups.reduce((sum, g) => sum + g.lines.length, 0);
      if (!entry.name) continue;
      if (kind === "champions") {
        counts.champions++;
        const bucket: Record<ChangeKind, "buffs" | "nerfs" | "adjusts"> = { buff: "buffs", nerf: "nerfs", adjust: "adjusts" };
        counts[bucket[classifyChange(entry)]]++;
      } else if (kind === "items") counts.items++;
    }
  }
  return counts;
}

/** "2026-10-20T18:00:00.000Z" -> "20 October 2026"; the raw date otherwise. */
export function formatPatchDate(iso: string | null): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso.slice(0, 10);
  return date.toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" });
}

/** Section anchor id, shared by the rail and the sections. */
export const sectionId = (title: string) => `pn-${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
