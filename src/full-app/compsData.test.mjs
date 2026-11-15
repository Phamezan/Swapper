import { test } from "node:test";
import assert from "node:assert/strict";
import { ANSWERS, CHAMPION_IDS, PRO_META, STYLES, VARIATIONS } from "./compsData.ts";

test("all names resolve to a champion id (no silent 266 fallback)", () => {
  const names = new Set();
  for (const v of VARIATIONS) for (const s of v.slots) for (const o of s.options) names.add(o.name);
  for (const a of ANSWERS) { a.threat.forEach(n => names.add(n)); a.answer.forEach(n => names.add(n)); }
  for (const p of PRO_META) { names.add(p.champion); p.answers.forEach(n => names.add(n)); }
  for (const s of STYLES) names.add(s.art);
  for (const n of names) assert.ok(n in CHAMPION_IDS, `unknown champion name: ${n}`);
});
test("every champion named in a variation title is a core option", () => {
  for (const v of VARIATIONS) {
    const core = new Set(v.slots.flatMap(s => s.options.filter(o => o.core).map(o => o.name)));
    for (const part of v.name.split(" + ")) assert.ok(core.has(part), `${v.id}: ${part} is not core`);
  }
});
test("no duplicate example keys within a variation", () => {
  for (const v of VARIATIONS) {
    const keys = v.proExamples.map(g => `${g.title}|${g.detail}`);
    assert.equal(new Set(keys).size, keys.length, v.id);
  }
});
test("every vod has a vodStart", () => {
  for (const v of VARIATIONS) for (const g of v.proExamples) if (g.vod) assert.equal(typeof g.vodStart, "number", `${v.id} ${g.title}`);
});
test("weakAgainst values are style names", () => {
  const styles = new Set(STYLES.map(s => s.name));
  for (const v of VARIATIONS) for (const w of v.weakAgainst) assert.ok(styles.has(w), `${v.id}: ${w}`);
});
test("no Leaguepedia disambiguation left in titles", () => {
  for (const v of VARIATIONS) for (const g of v.proExamples) assert.ok(!g.title.includes("(2024"), g.title);
});
