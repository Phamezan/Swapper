import { test } from "node:test";
import assert from "node:assert/strict";
// Import TS via built? compsModel is TS; node test runs the .ts through --experimental-strip-types if available.
// Fallback: duplicate minimal import using tsx-less strip.
import { TEAM_QUEUES, addedChampionRole, coverageFor, selectTeamMembers, suggestTeammates, teamMode, applyOverrides, isOlderMeta, peakLabel } from "./compsModel.ts";

const player = (riotId, isSelf, pool, mainRole) => ({ puuid: riotId, riotId, isSelf, included: true, pool, mainRole });
const entry = (championId, role, extra = {}) => ({ championId, role, reason: "test", ...extra });
const slot = (role, options, draftOrder = "Flexible") => ({ role, options, draftOrder });
const variation = { id: "v", style: "Engage", name: "V", winCondition: "", tags: [], power: { early: 30, mid: 90, late: 60 }, weakAgainst: [], tips: [], proExamples: [], sources: [],
  slots: [slot("top", [{ championId: 516, name: "Ornn", core: true }]), slot("jungle", [{ championId: 64, name: "Lee Sin" }]), slot("mid", [{ championId: 103, name: "Ahri", core: true }]), slot("bot", [{ championId: 222, name: "Jinx" }]), slot("support", [{ championId: 412, name: "Thresh" }])] };

test("full coverage assigns one player per role", () => {
  const team = [player("You", true, [entry(103, "mid", { games: 9 })], "mid"), player("P1", false, [entry(64, "jungle")], "jungle"), player("P2", false, [entry(222, "bot")], "bot"), player("P3", false, [entry(412, "support")], "support"), player("P4", false, [entry(516, "top")], "top")];
  const cov = coverageFor(variation, team);
  assert.equal(cov.count, 5); assert.deepEqual(cov.missing, []); assert.equal(cov.playableByMe, true);
  assert.equal(new Set(cov.assignments.map(a => a.player?.riotId)).size, 5);
});
test("missing roles and excluded players", () => {
  const team = [player("You", true, [entry(103, "mid")], "mid"), player("P1", false, [entry(64, "jungle")], "jungle")];
  team[1].included = false;
  const cov = coverageFor(variation, team);
  assert.equal(cov.count, 1); assert.ok(cov.missing.includes("jungle"));
});
test("core preferred over non-core for same player conflict", () => {
  const v2 = { ...variation, slots: [slot("top", [{ championId: 516, name: "Ornn" }]), slot("jungle", [{ championId: 64, name: "Lee Sin" }]), slot("mid", [{ championId: 103, name: "Ahri", core: true }]), slot("bot", [{ championId: 222, name: "Jinx" }]), slot("support", [{ championId: 412, name: "Thresh" }])] };
  const team = [player("You", true, [entry(103, "mid"), entry(516, "top")], "mid")];
  const cov = coverageFor(v2, team);
  // You can only fill one role; core mid wins
  assert.equal(cov.assignments.find(a => a.role === "mid")?.player?.riotId, "You");
  assert.equal(cov.count, 1);
});
test("suggestTeammates threshold 3", () => {
  const games = Array.from({ length: 5 }, () => ({ queue: "flex", players: [{ riotId: "A#1", ally: true, me: false, puuid: "a" }, { riotId: "B#1", ally: true, me: false, puuid: "b" }, { riotId: "Me#1", ally: true, me: true, puuid: "me" }] }));
  for (let i = 0; i < 3; i++) games[i].players = games[i].players.filter(p => p.riotId !== "B#1");
  const s = suggestTeammates(games);
  assert.equal(s.length, 1); assert.equal(s[0].riotId, "A#1");
});
test("overrides hide and add", () => {
  const out = applyOverrides([entry(103, "mid"), entry(157, "mid")], [{ championId: 147, role: "mid" }], [157]);
  assert.equal(out.find(e => e.championId === 157)?.hidden, true);
  assert.equal(out.find(e => e.championId === 147)?.added, true);
});
test("older meta and peak", () => {
  assert.equal(isOlderMeta("26.18", "26.21"), true);
  assert.equal(isOlderMeta("26.20", "26.21"), false);
  assert.equal(peakLabel(variation), "Strong mid game");
});

test('teammate suggestions only count Ranked Flex and Clash games', () => {
  const ally = { riotId: 'Friend#1', puuid: 'f1', ally: true, me: false };
  const game = queue => ({ queue, players: [{ riotId: 'Me#1', puuid: 'me', ally: true, me: true }, ally] });
  assert.deepEqual(TEAM_QUEUES, ['flex', 'clash']);
  assert.deepEqual(suggestTeammates([game('normal'), game('normal'), game('normal'), game('solo')]), []);
  assert.equal(suggestTeammates([game('flex'), game('clash'), game('flex'), game('normal')])[0].count, 3);
});

test("optimal assignment: A core top+mid (main mid), B only core mid -> 2/5 A top B mid", () => {
  const v = { ...variation, slots: [slot("top", [{ championId: 516, name: "Ornn", core: true }]), slot("jungle", [{ championId: 64, name: "Lee Sin" }]), slot("mid", [{ championId: 61, name: "Orianna", core: true }]), slot("bot", [{ championId: 222, name: "Jinx" }]), slot("support", [{ championId: 412, name: "Thresh" }])] };
  const a = player("A", true, [entry(516, "top"), entry(61, "mid")], "mid");
  const b = player("B", false, [entry(61, "mid")], "mid");
  const cov = coverageFor(v, [a, b]);
  assert.equal(cov.count, 2);
  assert.equal(cov.assignments.find(x => x.role === "top")?.player?.riotId, "A");
  assert.equal(cov.assignments.find(x => x.role === "mid")?.player?.riotId, "B");
});
test("team-source selection: lobby > saved > backend > snapshot fallback, added, cap 5", () => {
  const me = { puuid: "me", riotId: "Me#1" };
  const snap = Array.from({ length: 3 }, () => ({ queue: "flex", players: [{ riotId: "Me#1", ally: true, me: true, puuid: "me" }, { riotId: "Snap#1", ally: true, me: false, puuid: "snap" }] }));
  assert.equal(selectTeamMembers({ lobby: [me, { puuid: "l1", riotId: "L#1" }], saved: [{ puuid: "s1", riotId: "S#1" }], me, suggested: [], snapshotGames: [], added: [] }).source, "lobby");
  assert.equal(selectTeamMembers({ lobby: [], saved: [{ puuid: "s1", riotId: "S#1" }], me, suggested: [], snapshotGames: [], added: [] }).source, "saved");
  const backend = selectTeamMembers({ lobby: [], saved: [], me, suggested: [{ puuid: "b1", riotId: "B#1", games: 5 }], snapshotGames: snap, added: [] });
  assert.deepEqual(backend.members.map(m => m.puuid), ["me", "b1"]);
  const fallback = selectTeamMembers({ lobby: [], saved: [], me, suggested: [], snapshotGames: snap, added: [{ puuid: "add", riotId: "Add#1" }] });
  assert.deepEqual(fallback.members.map(m => m.puuid), ["me", "snap", "add"]);
  assert.equal(fallback.source, "auto");
});
test("browse/solo/team mode uses included players only", () => {
  const me = player("Me", true, [], "mid"); const mate = player("Mate", false, [], "top");
  assert.equal(teamMode([me, mate]), "team");
  mate.included = false; assert.equal(teamMode([me, mate]), "solo");
  me.included = false; assert.equal(teamMode([me, mate]), "browse");
  assert.equal(teamMode([]), "browse");
});
test("playableByMe respects core-only slots", () => {
  const v = { ...variation, slots: [slot("top", [{ championId: 516, name: "Ornn", core: true }, { championId: 54, name: "Malphite" }]), slot("jungle", [{ championId: 64, name: "Lee Sin" }]), slot("mid", [{ championId: 103, name: "Ahri", core: true }]), slot("bot", [{ championId: 222, name: "Jinx" }]), slot("support", [{ championId: 412, name: "Thresh" }])] };
  assert.equal(coverageFor(v, [player("Me", true, [entry(54, "top")], "top")]).playableByMe, false);
  assert.equal(coverageFor(v, [player("Me", true, [entry(516, "top")], "top")]).playableByMe, true);
});
test("added champion role prefers champion main role", () => {
  assert.equal(addedChampionRole("jungle", "mid"), "jungle");
  assert.equal(addedChampionRole(undefined, "mid"), "mid");
  assert.equal(addedChampionRole(undefined, null), "mid");
});
test('a core slot only fills with its core pick', () => {
  const v = { slots: ['top', 'jungle', 'mid', 'bot', 'support'].map(role => ({ role, draftOrder: 'Flexible', options: role === 'mid' ? [{ championId: 61, name: 'Orianna', core: true }, { championId: 157, name: 'Yasuo' }] : [{ championId: 1, name: 'X' }] })) };
  const yasuoOnly = coverageFor(v, [player('Me#1', true, [entry(157, 'mid')], 'mid')]);
  assert.equal(yasuoOnly.assignments.find(a => a.role === 'mid').champion, null);
  const ori = coverageFor(v, [player('Me#1', true, [entry(61, 'mid')], 'mid')]);
  assert.equal(ori.assignments.find(a => a.role === 'mid').champion.name, 'Orianna');
});
