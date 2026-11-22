// Merge docs/research/answers numbers + prose into the bundled Answers dataset.
// Run: node scripts/build-answers-data.mjs
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "docs/research/answers");
const numbers = JSON.parse(readFileSync(join(dir, "answers-numbers.json"), "utf8"));
const prose = new Map();
for (const role of ["top", "jungle", "mid", "bot", "support", "draft"])
  for (const p of JSON.parse(readFileSync(join(dir, `answers-${role}.json`), "utf8"))) prose.set(p.id, p);

const out = [];
for (const role of ["top", "jungle", "mid", "bot", "support", "draft"])
  for (const e of numbers.roles[role] ?? []) {
    const p = prose.get(e.id); // prose ids not in numbers are ignored
    out.push({
      id: e.id, kind: e.kind, role: e.role, threat: e.threat, answer: e.answer, source: e.source,
      patch: e.patch, games: e.games, winRate: e.winRate, ...(e.lift ? { lift: e.lift } : {}),
      trend: e.trend ?? [], why: p?.why ?? e.why ?? "", tips: p?.tips ?? e.tips ?? [],
      proGames: e.proGames ?? [], ...(e.runePages?.length ? { runePages: e.runePages } : {}), olderMeta: Boolean(e.olderMeta),
      sources: [...(e.sources ?? []), ...((p?.sources ?? []).filter(s => !(e.sources ?? []).includes(s)))],
      ...(p?.lowConfidence ? { lowConfidence: true } : {}),
    });
  }
writeFileSync(join(root, "src/full-app/answersData.json"), JSON.stringify(out, null, 1) + "\n");
console.log(`wrote ${out.length} answers (${out.filter(e => e.why).length} with prose)`);
