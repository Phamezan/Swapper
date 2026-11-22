import { test } from "node:test";
import assert from "node:assert/strict";
import { answerPlayable, groupAnswers, filterRows, partitionRowAnswers, runePageSummary, tabCounts, whoPlays, anyonePlays, trendOf } from "./answersModel.ts";
import { championId } from "./compsData.ts";

const a = (id, threat, answer, games, extra = {}) => ({ id, kind: "lane", role: "mid", threat: [threat], answer: [answer], source: "pro", patch: "26.20", games, winRate: 0.6, trend: [], why: "", tips: [], proGames: [], olderMeta: false, sources: [], ...extra });
const entries = [
  a("azir-taliyah", "Azir", "Taliyah", 14), a("azir-syndra", "Azir", "Syndra", 11), a("azir-akali", "Azir", "Akali", 20),
  a("ori-galio", "Orianna", "Galio", 12),
  a("top1", "Jayce", "K'Sante", 9, { role: "top" }),
  a("d1", "Lucian", "Lulu", 15, { kind: "draft" }),
];
const player = (riotId, included, names) => ({ puuid: riotId, riotId, isSelf: riotId === "You", included, pool: names.map(n => ({ championId: championId(n), role: "top", reason: "test", games: 5, masteryLevel: 7, masteryPoints: 120000 })) });

test("grouping: one row per threat, answers ranked by games", () => {
  const rows = groupAnswers(entries, "mid");
  assert.equal(rows.length, 2);
  const azir = rows.find(r => r.threat[0] === "Azir");
  assert.deepEqual(azir.answers.map(x => [x.answer[0], x.rank]), [["Akali", 1], ["Taliyah", 2], ["Syndra", 3]]);
  assert.equal(groupAnswers(entries, "top").length, 1);
  assert.equal(groupAnswers(entries, "draft")[0].answers[0].id, "d1");
});

test("search matches threat and answer names; other tabs count matches", () => {
  const rows = groupAnswers(entries, "mid");
  assert.equal(filterRows(rows, "tali", false, []).length, 1);
  assert.equal(filterRows(rows, "azir", false, [])[0].answers.length, 3);
  const counts = tabCounts(entries, "lulu", false, []);
  assert.equal(counts.draft, 1); assert.equal(counts.mid, 0);
});

test("tailoring: active vs sub, role ignored, ours keeps rank", () => {
  const team = [player("You", true, ["Taliyah"]), player("Sub#1", false, ["Syndra"])];
  const tal = entries[0], syn = entries[1];
  assert.deepEqual(whoPlays(tal, team).active.map(w => w.player.riotId), ["You"]);
  assert.deepEqual(whoPlays(syn, team).subs.map(w => w.player.riotId), ["Sub#1"]);
  assert.equal(anyonePlays(entries[2], team), false);
  const rows = filterRows(groupAnswers(entries, "mid"), "", true, team);
  const azir = rows.find(r => r.threat[0] === "Azir");
  // Akali (rank 1) filtered out; Taliyah keeps rank 2, Syndra rank 3.
  assert.deepEqual(azir.answers.map(x => x.rank), [2, 3]);
});

test("trend direction and older meta", () => {
  assert.equal(trendOf(a("x", "A", "B", 9, { trend: [{ winRate: 0.5 }, { winRate: 0.65 }] })).dir, "up");
  assert.equal(trendOf(a("x", "A", "B", 9, { trend: [{ winRate: 0.65 }, { winRate: 0.5 }] })).dir, "down");
  assert.equal(trendOf(a("x", "A", "B", 9, { olderMeta: true })).label, "older meta");
});

test("rune pages: Detail summary handles missing, empty and present pages", () => {
  assert.equal(runePageSummary(a("x", "A", "B", 9)), null); // no runePages field at all
  assert.equal(runePageSummary(a("x", "A", "B", 9, { runePages: [] })), null);
  const page = { runes: [8010, 8009, 9103, 8017, 8316, 8321, 5005, 5008, 5011], primaryStyle: 8000, subStyle: 8300, spells: [4, 21], games: 6, wins: 4 };
  const alt = { ...page, runes: [8021, ...page.runes.slice(1)], games: 3, wins: 2 };
  const one = runePageSummary(a("x", "A", "B", 12, { runePages: [page] }));
  assert.equal(one.primary, page); assert.equal(one.runnerUp, null); assert.equal(one.totalGames, 12);
  assert.equal(runePageSummary(a("x", "A", "B", 12, { runePages: [page, alt] })).runnerUp, alt);
  const minor = { ...page, runes: [...page.runes.slice(0, 5), 8304, ...page.runes.slice(6)], games: 4, wins: 3 };
  assert.equal(runePageSummary(a("x", "A", "B", 12, { runePages: [page, minor] })).runnerUp, null); // same keystone + trees: not shown
});

test("13A counters: playable when anyone on the roster plays it, ranks kept", () => {
  const team = [player("You", true, ["Taliyah"]), player("Sub#1", false, ["Syndra"])];
  assert.equal(answerPlayable(entries[0], team), true);
  assert.equal(answerPlayable(entries[2], team), false);
  const azir = groupAnswers(entries, "mid").find(r => r.threat[0] === "Azir");
  const { playable, rest } = partitionRowAnswers(azir, team);
  assert.deepEqual(playable.map(x => x.rank), [2, 3]);
  assert.deepEqual(rest.map(x => x.rank), [1]);
  const empty = partitionRowAnswers(azir, []);
  assert.deepEqual(empty.playable, []);
  assert.equal(empty.rest.length, 3);
});
