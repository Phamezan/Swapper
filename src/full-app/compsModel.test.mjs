import { test } from "node:test";
import assert from "node:assert/strict";
// Import TS via built? compsModel is TS; node test runs the .ts through --experimental-strip-types if available.
// Fallback: duplicate minimal import using tsx-less strip.
import { addedChampionRole, assignRoles, compPlayable, coverageFor, isTailored, lobbyActive, lobbyCandidates, partitionPlayable, playerLabel, subHintFor, toggleActive, teamMode, applyOverrides, isOlderMeta, peakLabel } from "./compsModel.ts";

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


test("optimal assignment: A core top+mid (main mid), B only core mid -> 2/5 A top B mid", () => {
  const v = { ...variation, slots: [slot("top", [{ championId: 516, name: "Ornn", core: true }]), slot("jungle", [{ championId: 64, name: "Lee Sin" }]), slot("mid", [{ championId: 61, name: "Orianna", core: true }]), slot("bot", [{ championId: 222, name: "Jinx" }]), slot("support", [{ championId: 412, name: "Thresh" }])] };
  const a = player("A", true, [entry(516, "top"), entry(61, "mid")], "mid");
  const b = player("B", false, [entry(61, "mid")], "mid");
  const cov = coverageFor(v, [a, b]);
  assert.equal(cov.count, 2);
  assert.equal(cov.assignments.find(x => x.role === "top")?.player?.riotId, "A");
  assert.equal(cov.assignments.find(x => x.role === "mid")?.player?.riotId, "B");
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
test("active set caps at five and toggles off", () => {
  assert.deepEqual(toggleActive(["you"], "a"), ["you", "a"]);
  assert.deepEqual(toggleActive(["you", "a"], "a"), ["you"]);
  const five = ["you", "a", "b", "c", "d"];
  assert.equal(toggleActive(five, "e"), five);
});
test("lobby merge activates roster members only, candidates exclude roster and you", () => {
  assert.deepEqual(lobbyActive(["a", "b", "c"], ["b", "x", "c"], ["you"]), ["you", "b", "c"]);
  const lobby = [{ puuid: "you" }, { puuid: "a" }, { puuid: "x" }];
  assert.deepEqual(lobbyCandidates(lobby, ["a"], "you").map(m => m.puuid), ["x"]);
});
test("role auto-assign maximises preference rank with no clashes", () => {
  const players = [{ puuid: "you", roles: ["mid", "top"] }, { puuid: "a", roles: ["top", "jungle"] }, { puuid: "b", roles: ["jungle"] }, { puuid: "c", roles: ["bot"] }, { puuid: "d", roles: ["support"] }];
  const { assigned, clash, open } = assignRoles(players, {});
  assert.equal(assigned.you, "mid"); assert.equal(assigned.a, "top"); assert.equal(assigned.b, "jungle");
  assert.deepEqual(clash, []); assert.deepEqual(open, []);
});
test("manual override wins and unresolvable clash is flagged", () => {
  const players = [{ puuid: "a", roles: ["bot", "support"] }, { puuid: "b", roles: ["bot", "support"] }, { puuid: "c", roles: ["mid"] }];
  const forced = assignRoles(players, { a: "bot", b: "bot" });
  assert.equal(forced.assigned.a, "bot"); assert.equal(forced.assigned.b, "bot");
  assert.deepEqual([...forced.clash].sort(), ["a", "b"]);
  const stuck = assignRoles([{ puuid: "a", roles: ["mid"] }, { puuid: "b", roles: ["mid"] }], {});
  assert.deepEqual([...stuck.clash].sort(), ["a", "b"]);
});
test("tailoring only at exactly five active", () => {
  assert.equal(isTailored(5), true); assert.equal(isTailored(4), false); assert.equal(isTailored(0), false);
});
test("sub hint swaps one non-self player, never you", () => {
  const team = [player("You", true, [entry(103, "mid")], "mid"), player("P1", false, [entry(64, "jungle")], "jungle"), player("P2", false, [entry(222, "bot")], "bot"), player("P3", false, [entry(412, "support")], "support"), player("P4", false, [entry(1, "top")], "top"), player("Sub", false, [entry(516, "top")], "top")];
  team[5].included = false;
  const hint = subHintFor(variation, team);
  assert.equal(hint?.out.riotId, "P4"); assert.equal(hint?.inn.riotId, "Sub");
  // Without the sub there is no hint; with only 4 active there is no hint either.
  assert.equal(subHintFor(variation, team.slice(0, 5)), null);
  team[4].included = false; assert.equal(subHintFor(variation, team), null);
});
test("player label prefers nickname over riot name", () => {
  assert.equal(playerLabel({ riotId: "Name#TAG", nickname: "Nick" }), "Nick");
  assert.equal(playerLabel({ riotId: "Name#TAG" }), "Name");
  assert.equal(playerLabel({ riotId: "Name#TAG", isSelf: true }), "You");
});

test("13A playable: 5 active needs 5/5 or a one-swap hint", () => {
  const cov = (count) => ({ count, assignments: [], missing: [], playableByMe: false });
  assert.equal(compPlayable(cov(5), false, 5), true);
  assert.equal(compPlayable(cov(4), true, 5), true);
  assert.equal(compPlayable(cov(4), false, 5), false);
  assert.equal(compPlayable(cov(1), false, 1), true);
  assert.equal(compPlayable(cov(0), false, 1), false);
  assert.equal(compPlayable(cov(3), false, 3), true);
  assert.equal(compPlayable(cov(5), true, 0), false);
});
test("partitionPlayable is stable: playable first, order kept", () => {
  const items = [{ id: "a", p: false }, { id: "b", p: true }, { id: "c", p: false }, { id: "d", p: true }];
  const { playable, rest } = partitionPlayable(items, x => x.p);
  assert.deepEqual(playable.map(x => x.id), ["b", "d"]);
  assert.deepEqual(rest.map(x => x.id), ["a", "c"]);
});
