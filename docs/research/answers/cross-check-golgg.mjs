// Cross-check ~10 answers-numbers entries vs gol.gg Worlds 2026 Main Event champion pick counts.
// Reuses saved page in raw/ if present. Run: node docs/research/answers/cross-check-golgg.mjs
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const DIR = dirname(fileURLToPath(import.meta.url));
const UA = "SwapperCompsResearch/1.0 (Swapper pro-play Answers cross-check)";
const htmlPath = join(DIR, "raw", "golgg-worlds-main-event.html");
if (!existsSync(htmlPath)) {
  const res = await fetch("https://gol.gg/champion/list/season-S16/split-ALL/tournament-Worlds%202026%20Main%20Event/", { headers: { "User-Agent": UA } });
  writeFileSync(htmlPath, await res.text());
}
const html = readFileSync(htmlPath, "utf8");
const theirs = {};
for (const m of html.matchAll(/<tr[^>]*>(.*?)<\/tr>/gs)) {
  const cells = [...m[1].matchAll(/<t[dh][^>]*>(.*?)<\/t[dh]>/gs)].map((c) => c[1].replace(/<[^>]+>/g, " ").trim());
  if (cells.length > 2 && /^\d+$/.test(cells[1])) theirs[cells[0].replace("KSante", "K'Sante")] = +cells[1];
}
const sg = JSON.parse(readFileSync(join(DIR, "raw", "scoreboardgames.json"), "utf8"));
const pab = new Map(JSON.parse(readFileSync(join(DIR, "raw", "picksandbanss7.json"), "utf8")).map((r) => [r.GameId, r]));
const ours = {};
for (const r of sg) if (r.Tournament === "Worlds 2026 Main Event" && pab.has(r.GameId)) {
  const p = pab.get(r.GameId);
  for (let i = 1; i <= 5; i++) for (const t of ["Team1", "Team2"]) if (p[`${t}Pick${i}`]) ours[p[`${t}Pick${i}`]] = (ours[p[`${t}Pick${i}`]] || 0) + 1;
}
const data = JSON.parse(readFileSync(join(DIR, "answers-numbers.json"), "utf8"));
const picks = [...data.roles.top.slice(0, 2), ...data.roles.jungle.slice(0, 2), ...data.roles.mid.slice(0, 2),
  ...data.roles.bot.slice(0, 1), ...data.roles.support.slice(0, 1), ...data.roles.draft.slice(0, 2)];
const results = picks.map((e) => ({ entry: e.id, champs: [...e.threat, ...e.answer].map((c) => ({ champion: c, ours: ours[c] ?? 0, golgg: theirs[c] ?? null, match: (ours[c] ?? 0) === theirs[c] })) }));
writeFileSync(join(DIR, "raw", "golgg-crosscheck.json"), JSON.stringify({ source: "gol.gg Worlds 2026 Main Event champion list", results }, null, 1));
const champs = results.flatMap((r) => r.champs);
console.log(`entries=${results.length} champs=${champs.length} matches=${champs.filter((c) => c.match).length} mismatches=${JSON.stringify(champs.filter((c) => !c.match))}`);
