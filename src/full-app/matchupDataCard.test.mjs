import { test } from "node:test";
import assert from "node:assert/strict";
import { dataCardEligible, displayCardPatch, gameSentence, goldSentence, spellSentences, wilson, selectGuideContent } from "./matchupDataCard.ts";

export const spell = (key = "Q") => ({ key, name: key, iconPath: null, cooldowns: [20,18.5,17,15.5,14], ranges: [], costs: [], charges: 0, recharge: [] });
export const card = () => ({ kind: "data-based", championId: 222, enemyChampionId: 51, lane: "adc", tier: "emerald_plus",
  stats: { wins: 460, games: 1000, patch: "16.22", dataVersion: "1.5.0", provider: "ugg", region: "world", queue: "ranked_solo_5x5", tier: "emerald_plus", endpointUrl: "https://stats2.u.gg/lol/1.5/matchups/16_22/ranked_solo_5x5/222/1.5.0.json", schemaId: "ugg-matchups-v1:opponent-wins", fetchedAt: Date.now(), stale: false },
  enemyKit: { version: "16.22", assetUrl: "https://raw.communitydragon.org/16.22/plugins/rcp-be-lol-game-data/global/default/v1/champions/51.json", fetchedAt: Date.now(),
    sheet: { championId: 51, tooltipsReady: false, passive: { name: "Passive", iconPath: null }, summoners: [], spells: [spell(), spell("W")] } } });
const identity = { championId: 222, enemyChampionId: 51, lane: "adc", tier: "emerald_plus" };

test("facts use the exact deterministic example sentences", () => {
  assert.equal(gameSentence("Jinx", "Caitlyn", "adc", 460, 1000), "Jinx won 46.0% of 1,000 games against Caitlyn in ADC.");
  assert.equal(gameSentence("Thresh", "Leona", "support", 416, 800), "Thresh won 52.0% of 800 games against Leona in support.");
  assert.equal(goldSentence("Garen", "Darius", -180), "Garen averaged 180 gold behind Darius at 15 minutes.");
  assert.equal(goldSentence("Ahri", "Zed", 125), "Ahri averaged 125 gold ahead of Zed at 15 minutes.");
  assert.equal(spellSentences("Zed", spell("W"))[0], "Enemy Zed W has a base cooldown of 20/18.5/17/15.5/14 seconds by rank.");
  assert.equal(spellSentences("Caitlyn", { ...spell(), cooldowns: [], ranges: [1250,1250,1250,1250,1250] })[0], "Enemy Caitlyn Q has a listed ability range of 1,250 units at every rank.");
  assert.equal(spellSentences("Jhin", { ...spell("E"), charges: 2, recharge: [24,22,20,18,16] })[0], "Enemy Jhin E holds 2 charges, with a base recharge of 24/22/20/18/16 seconds by rank.");
  assert.equal(goldSentence("Ahri", "Zed", -0.4), "Ahri was approximately even on gold at 15 minutes against Zed.");
});

test("whole-card gate enforces 499/500, exact identity, pinned kit and evidence age", () => {
  const c = card(); c.stats.games = 499; c.stats.wins = 200;
  assert.equal(dataCardEligible(c, identity), false);
  c.stats.games = 500;
  assert.equal(dataCardEligible(c, identity), true);
  assert.equal(dataCardEligible(c, { ...identity, tier: "diamond_plus" }), false);
  c.enemyKit.version = "16.21"; assert.equal(dataCardEligible(c, identity), false);
  c.enemyKit.version = "16.22";
  c.enemyKit.sheet.spells.pop(); assert.equal(dataCardEligible(c, identity), false);
  delete c.enemyKit; assert.equal(dataCardEligible(c, identity), false);
  const old = card(); old.stats.fetchedAt -= 24 * 3600e3 + 1;
  assert.equal(dataCardEligible(old, identity), false);
  old.stats.fetchedAt = Date.now() + 1000;
  assert.equal(dataCardEligible(old, identity), false);
});

test("selection is exact > data > general and unknown never permits data", () => {
  const c = card(), general = { champion: 222, lane: "adc" }, exact = { ...general, enemy: 51, reviewed: false };
  assert.equal(selectGuideContent(exact, "unknown", c, identity).guide, exact);
  assert.equal(selectGuideContent(general, "absent", c, identity), c);
  assert.equal(selectGuideContent(general, "unknown", c, identity).guide, general);
  assert.equal(selectGuideContent(null, "unknown", c, identity), null);
  assert.equal(selectGuideContent(exact, "exact", c, { ...identity, enemyChampionId: 222 }), null);
});

test("Wilson bounds are sampling uncertainty and malformed numeric fields disappear", () => {
  const [low, high] = wilson(460, 1000);
  assert.ok(low < .46 && high > .46 && low > .42 && high < .50);
  assert.equal(wilson(0, 500)[0], 0);
  assert.equal(wilson(500, 500)[1], 1);
  assert.deepEqual(spellSentences("Zed", { ...spell(), cooldowns: [NaN], ranges: [25000], charges: 0 }), []);
  const c = card(); c.enemyKit.sheet.spells.forEach(s => { s.cooldowns = []; s.description = "An effect only"; });
  assert.equal(dataCardEligible(c, identity), false);
});

test("gold's metric-specific 499/500 floor is independent of the game sample and patch aliases are display only", () => {
  const c = card(); delete c.enemyKit;
  c.gold15 = { mean: -180, gamesAt15: 499, signedTotal: -180 * 499, units: "gold", perspective: "champion", schemaId: "synthetic-verified" };
  assert.equal(dataCardEligible(c, identity), false);
  c.gold15.gamesAt15 = 500; c.gold15.signedTotal = -90000;
  assert.equal(dataCardEligible(c, identity), true);
  assert.equal(displayCardPatch("16.22"), "26.22");
  assert.equal(c.stats.patch, "16.22");
  c.gold15.perspective = "opponent";
  assert.equal(dataCardEligible(c, identity), false);
});
