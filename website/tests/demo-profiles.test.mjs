import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import {
  CHAMP_SPECS,
  DEMO_VERSION,
  ITEM_NAMES,
  REALISTIC_ACCOUNTS,
  VALID_PROFILE_ICONS,
  buildRealisticMatchDetail,
  createRealisticDemoRuntime,
} from "../src/demo/realisticData.ts";
import {
  DD_ABILITIES_16_23,
  DD_CHAMPIONS,
  LOLA_FEATURED_MATCHUPS_16_23,
  REAL_DATA_VERSION,
  RIOT_PATCH_NOTES_26_22,
  RIOT_PATCH_NOTES_26_23,
  UGG_COUNTERS_16_23,
  UGG_DUOS_16_23,
  UGG_TIERLIST_16_23,
} from "../src/demo/realPatch16_23Data.ts";

const dd = JSON.parse(fs.readFileSync(path.resolve(import.meta.dirname, "ddragon-16.23.1-ids.json"), "utf8"));
assert.equal(dd.version, DEMO_VERSION, "Data Dragon manifest version must match DEMO_VERSION");
assert.equal(dd.version, REAL_DATA_VERSION, "Data Dragon manifest version must match REAL_DATA_VERSION");

// Verify DEMO_VERSION matches across full-app-preview-mock.ts and champ-select-mock.ts.
const fullMockSrc = fs.readFileSync(path.resolve(import.meta.dirname, "../../src/full-app/full-app-preview-mock.ts"), "utf8");
const csMockSrc = fs.readFileSync(path.resolve(import.meta.dirname, "../../src/full-app/champ-select-mock.ts"), "utf8");
assert.ok(fullMockSrc.includes(`export const DEMO_VERSION = "${dd.version}"`), "full-app-preview-mock.ts DEMO_VERSION must match manifest");
assert.ok(csMockSrc.includes(`const VERSION = "${dd.version}"`), "champ-select-mock.ts VERSION must match manifest");

const validItems = new Set(dd.srItems);
const validRunes = new Set(dd.runes);
const validTrees = new Set(dd.runeTrees);
const validShards = new Set(dd.statShards);
const validSpells = new Set(dd.classicSpells);
const validChamps = new Set(dd.champions);
const validIcons = new Set(dd.profileIcons);
const validChampNamesNorm = new Set(Object.values(dd.championNames).map(n => n.toLowerCase().replace(/[^a-z0-9]/g, "")));
const validItemNamesNorm = new Set(Object.values(dd.itemNames).map(n => n.toLowerCase().replace(/[^a-z0-9]/g, "")));
const SUPPORT_ITEMS = new Set([3865, 3869, 3870, 3871, 3876, 3877]);

// 1. Landing page, layout and website copy checks (existing guarantees + no em/en dashes).
const indexSrc = fs.readFileSync(path.resolve(import.meta.dirname, "../src/pages/index.astro"), "utf8");
const layoutSrc = fs.readFileSync(path.resolve(import.meta.dirname, "../src/layouts/BaseLayout.astro"), "utf8");
const mountFullSrc = fs.readFileSync(path.resolve(import.meta.dirname, "../src/demo/mountFullApp.tsx"), "utf8");
const mountTraySrc = fs.readFileSync(path.resolve(import.meta.dirname, "../src/demo/mountTray.tsx"), "utf8");
const realPatchSrc = fs.readFileSync(path.resolve(import.meta.dirname, "../src/demo/realPatch16_23Data.ts"), "utf8");

assert.ok(indexSrc.includes("Riot Games, Inc."), "Footer must carry Riot's Legal Jibber Jabber disclaimer");
assert.ok(!indexSrc.toLowerCase().includes("artstation"), "Never embed or link ArtStation assets");

for (const [name, src] of [
  ["index.astro", indexSrc],
  ["BaseLayout.astro", layoutSrc],
  ["mountFullApp.tsx", mountFullSrc],
  ["mountTray.tsx", mountTraySrc],
  ["realPatch16_23Data.ts", realPatchSrc],
]) {
  assert.ok(!/Ctrl\+Shift\+S/i.test(src), `${name} must not claim a default hotkey`);
  assert.ok(!/[–—]/.test(src), `${name} must not contain em or en dashes`);
}
assert.ok(mountFullSrc.includes("hotkey: null"), "Full app demo must default hotkey to null");
assert.ok(mountTraySrc.includes("hotkey: null"), "Tray demo must default hotkey to null");

// 2. Three distinct invented demo accounts.
const ids = REALISTIC_ACCOUNTS.map(p => p.account.id);
assert.equal(ids.length, 3, "Expected 3 invented demo accounts");
assert.equal(new Set(ids).size, 3, "Demo account IDs must be unique");

// 3. Validate every Data Dragon ID in CHAMP_SPECS, ITEM_NAMES, DD_CHAMPIONS, tierlists, duos, matchups, and abilities.
const assertItem = (id, where) => {
  if (id === 0) return;
  assert.ok(validItems.has(id), `Invalid SR item ID ${id} (${where}) for Data Dragon ${DEMO_VERSION}`);
};
const assertRunes = (r, where) => {
  assert.ok(validTrees.has(r.primaryPageId), `Invalid primary rune tree ${r.primaryPageId} (${where})`);
  assert.ok(validTrees.has(r.secondaryPageId), `Invalid secondary rune tree ${r.secondaryPageId} (${where})`);
  assert.ok(validRunes.has(r.keystone), `Invalid keystone ${r.keystone} (${where})`);
  for (const id of [...r.primaryRunes, ...r.secondaryRunes]) {
    assert.ok(validRunes.has(id), `Invalid rune ID ${id} (${where})`);
  }
  for (const id of r.shards) {
    assert.ok(validShards.has(id), `Invalid stat shard ID ${id} (${where})`);
  }
};
const assertSpells = (spells, role, where) => {
  assert.equal(spells.length, 2, `Expected 2 summoner spells (${where})`);
  assert.notEqual(spells[0], spells[1], `Duplicate summoner spells (${where})`);
  for (const id of spells) assert.ok(validSpells.has(id), `Invalid spell ID ${id} (${where})`);
  const hasSmite = spells.includes(11);
  assert.equal(hasSmite, role === "jungle", `Smite mismatch on ${role} (${where})`);
};

for (const idStr of Object.keys(ITEM_NAMES)) assertItem(Number(idStr), `ITEM_NAMES[${idStr}]`);
for (const idStr of Object.keys(DD_CHAMPIONS)) assert.ok(validChamps.has(Number(idStr)), `Invalid DD_CHAMPIONS ID ${idStr}`);
for (const iconId of VALID_PROFILE_ICONS) assert.ok(validIcons.has(iconId), `Invalid profile icon ${iconId}`);

for (const spec of Object.values(CHAMP_SPECS)) {
  assert.ok(validChamps.has(spec.id), `Invalid champion ID ${spec.id}`);
  assertSpells(spec.spells, spec.role, `CHAMP_SPECS.${spec.name}`);
  assertRunes(spec.runes, `CHAMP_SPECS.${spec.name}`);
  if (spec.altRunes) {
    assertRunes(spec.altRunes.runes, `CHAMP_SPECS.${spec.name}.altRunes`);
    assertSpells(spec.altRunes.spells, spec.role, `CHAMP_SPECS.${spec.name}.altRunes`);
    for (const id of spec.altRunes.items) assertItem(id, `CHAMP_SPECS.${spec.name}.altRunes`);
  }
  for (const [id] of spec.starters) assertItem(id, `CHAMP_SPECS.${spec.name}.starters`);
  assertItem(spec.boots, `CHAMP_SPECS.${spec.name}.boots`);
  assertItem(spec.earlyComp, `CHAMP_SPECS.${spec.name}.earlyComp`);
  assertItem(spec.trinket, `CHAMP_SPECS.${spec.name}.trinket`);
  for (const id of spec.items) assertItem(id, `CHAMP_SPECS.${spec.name}.items`);
  for (const [id] of spec.options) assertItem(id, `CHAMP_SPECS.${spec.name}.options`);
}

for (const [role, rows] of Object.entries(UGG_TIERLIST_16_23)) {
  assert.ok(rows.length >= 20, `Expected real u.gg tier rows for ${role}`);
  for (const [cid] of rows) assert.ok(validChamps.has(cid), `Invalid tierlist champ ${cid} in ${role}`);
}
for (const [adcId, supId] of UGG_DUOS_16_23) {
  assert.ok(validChamps.has(adcId) && validChamps.has(supId), `Invalid duo pair ${adcId}+${supId}`);
}
for (const [cidStr, table] of Object.entries(UGG_COUNTERS_16_23)) {
  assert.ok(validChamps.has(Number(cidStr)), `Invalid counter table champ ${cidStr}`);
  for (const [oppId] of [...table.bestInto, ...table.beats]) assert.ok(validChamps.has(oppId), `Invalid counter opp ${oppId}`);
}
for (const [pairKey, fm] of Object.entries(LOLA_FEATURED_MATCHUPS_16_23)) {
  assert.ok(validChamps.has(fm.championId) && validChamps.has(fm.enemyChampionId), `Invalid featured matchup ${pairKey}`);
  assertRunes(fm.runes, `featured ${pairKey}`);
  for (const id of fm.items) assertItem(id, `featured ${pairKey}`);
}
for (const cidStr of Object.keys(DD_ABILITIES_16_23)) {
  assert.ok(validChamps.has(Number(cidStr)), `Invalid DD_ABILITIES_16_23 champ ${cidStr}`);
}

// 4. Verify every champion, item, and rune named in Patch 26.23 and 26.22 notes exists in Data Dragon 16.23.1.
const norm = s => s.toLowerCase().replace(/[^a-z0-9]/g, "");
for (const notes of [RIOT_PATCH_NOTES_26_23, RIOT_PATCH_NOTES_26_22]) {
  const jsonText = JSON.stringify(notes);
  assert.ok(!/[–—]/.test(jsonText), `Patch ${notes.patch} must not contain em or en dashes`);
  assert.ok(!/New Preview Relic/i.test(jsonText), `Patch ${notes.patch} must not contain placeholder item`);
  for (const sec of notes.sections) {
    if (sec.title === "Champions") {
      assert.ok(sec.entries.length >= 10, `Patch ${notes.patch} Champions section must be complete`);
      for (const entry of sec.entries) {
        assert.ok(validChampNamesNorm.has(norm(entry.name)), `Patch ${notes.patch} champion "${entry.name}" not in Data Dragon ${DEMO_VERSION}`);
      }
    } else if (sec.title === "Items") {
      for (const entry of sec.entries) {
        assert.ok(entry.itemId && validItems.has(entry.itemId), `Patch ${notes.patch} item "${entry.name}" missing valid SR itemId in ${DEMO_VERSION}`);
        assert.ok(validItemNamesNorm.has(norm(entry.name)), `Patch ${notes.patch} item "${entry.name}" name not in Data Dragon ${DEMO_VERSION}`);
        assert.equal(norm(dd.itemNames[String(entry.itemId)]), norm(entry.name), `Patch ${notes.patch} itemId ${entry.itemId} must match "${entry.name}"`);
      }
    } else if (sec.title === "Runes") {
      for (const entry of sec.entries) {
        assert.ok(entry.name.length > 0, `Patch ${notes.patch} rune entry missing name`);
      }
    }
  }
}

// 5. Runtime fixture integrity: Riot IDs, item/rune/spell/icon validity, role sanity and LP math.
const FORBIDDEN_NAME = /#DEMO\b|^Player \d|Swapper Demo|Fixture Pro|Fixture Solo|Pro player [A-Z]|You#/;
const assertCleanText = (val, where) => {
  if (typeof val !== "string") return;
  assert.ok(!FORBIDDEN_NAME.test(val), `Placeholder name "${val}" found in ${where}`);
  assert.ok(!/[–—]/.test(val), `Em or en dash found in "${val}" (${where})`);
};

const runtime = createRealisticDemoRuntime(Date.now());
for (const cfg of REALISTIC_ACCOUNTS) {
  const b = runtime.bundles.get(cfg.account.id);
  assert.ok(b, `Missing bundle for ${cfg.account.id}`);
  assertCleanText(cfg.account.riotId, `account.${cfg.account.id}`);
  assert.ok(validIcons.has(cfg.account.profileIconId), `Invalid account icon ${cfg.account.profileIconId}`);

  // LP history endpoint must agree with current rank.
  const lpHist = b.snapshot.lpHistory ?? [];
  assert.ok(lpHist.length >= 10, `Expected multi-day LP history for ${cfg.account.id}`);
  const lastLp = lpHist[lpHist.length - 1];
  assert.equal(lastLp.tier, cfg.rank.tier, `${cfg.account.id} final LP tier matches rank`);
  assert.equal(lastLp.division, cfg.rank.division, `${cfg.account.id} final LP division matches rank`);
  assert.equal(lastLp.lp, cfg.rank.lp, `${cfg.account.id} final LP matches rank`);
  assert.equal(lastLp.wins, cfg.rank.wins, `${cfg.account.id} final wins match rank`);
  assert.equal(lastLp.losses, cfg.rank.losses, `${cfg.account.id} final losses match rank`);

  // Check every match and its MatchDetail.
  for (const m of b.matches) {
    assert.equal(m.players.length, 10, `Expected 10 players in match ${m.id}`);
    assert.ok(m.duration >= 1200 && m.duration <= 2300, `Duration out of realistic bounds: ${m.duration}s`);
    const allyKills = m.players.slice(0, 5).reduce((s, p) => s + p.kills, 0);
    const allyDeaths = m.players.slice(0, 5).reduce((s, p) => s + p.deaths, 0);
    const enemyKills = m.players.slice(5).reduce((s, p) => s + p.kills, 0);
    const enemyDeaths = m.players.slice(5).reduce((s, p) => s + p.deaths, 0);
    assert.equal(allyKills, enemyDeaths, `Ally kills must equal enemy deaths in match ${m.id}`);
    assert.equal(enemyKills, allyDeaths, `Enemy kills must equal ally deaths in match ${m.id}`);

    const seenRiotIds = new Set();
    for (const p of m.players) {
      assertCleanText(p.riotId, `match ${m.id} player`);
      assert.ok(!seenRiotIds.has(p.riotId), `Duplicate Riot ID ${p.riotId} in match ${m.id}`);
      seenRiotIds.add(p.riotId);
      assert.ok(validIcons.has(p.profileIcon), `Invalid profile icon ${p.profileIcon} in match ${m.id}`);
      assertSpells(p.spells, p.role, `match ${m.id} ${p.riotId}`);
      assertRunes(p.runes, `match ${m.id} ${p.riotId}`);
      for (const itemId of p.items) {
        assertItem(itemId, `match ${m.id} ${p.riotId}`);
        if (SUPPORT_ITEMS.has(itemId)) assert.equal(p.role, "support", `Non-support ${p.role} has support item ${itemId}`);
      }
      if (p.role === "support") {
        assert.ok(p.items.some(id => SUPPORT_ITEMS.has(id)), `Support missing support item in match ${m.id}`);
        assert.ok(p.cs >= 15 && p.cs <= 55, `Unrealistic support CS ${p.cs} in match ${m.id}`);
      }
    }

    const detail = buildRealisticMatchDetail(m);
    for (const dp of detail.players) {
      assertCleanText(dp.riotId, `matchDetail ${m.id}`);
      for (const perkId of dp.perks) assert.ok(validRunes.has(perkId), `Invalid matchDetail perk ${perkId}`);
    }
  }

  // Check Live Game.
  const lv = b.liveView;
  assert.equal(lv.players.length, 10, `Live game must have 10 players for ${cfg.account.id}`);
  const lbKills = lv.players.slice(0, 5).reduce((s, p) => s + p.live.kills, 0);
  const lbDeaths = lv.players.slice(0, 5).reduce((s, p) => s + p.live.deaths, 0);
  const lrKills = lv.players.slice(5).reduce((s, p) => s + p.live.kills, 0);
  const lrDeaths = lv.players.slice(5).reduce((s, p) => s + p.live.deaths, 0);
  assert.equal(lbKills, lrDeaths, `Live game blue kills === red deaths (${cfg.account.id})`);
  assert.equal(lrKills, lbDeaths, `Live game red kills === blue deaths (${cfg.account.id})`);
  for (const p of lv.players) {
    assertCleanText(p.riotId, `liveGame ${cfg.account.id}`);
    assert.ok(validIcons.has(p.profileIcon), `Invalid live profile icon ${p.profileIcon}`);
    assertSpells(p.spells, p.position, `liveGame ${p.riotId}`);
    assert.ok(validRunes.has(p.keystone), `Invalid live keystone ${p.keystone}`);
    assert.ok(p.history.mainRole[1] <= p.history.form.games, `mainRole count ${p.history.mainRole[1]} exceeds games ${p.history.form.games}`);
    for (const itemId of p.live.items) assertItem(itemId, `liveGame ${p.riotId}`);
  }

  // Check Handlers: Patch, Tier list, Pro builds, VODs, Comps, Abilities, Patch notes, Guide.
  const handlers = runtime.createSharedHandlers(() => cfg.account.id);
  assert.equal(handlers.get_current_patch({}), "16.23");
  assert.equal(handlers.newest_patch_notes({}), "26.23");
  for (const pos of ["top", "jungle", "mid", "adc", "support"]) {
    const tl = handlers.get_tier_list({ position: pos, tier: "emerald_plus" });
    assert.ok(tl.rows.length >= 20, `Expected >= 20 real u.gg tier rows for ${pos}`);
    for (const row of tl.rows) {
      assert.ok(row.winPct >= 40 && row.winPct <= 60, `Out-of-range winPct ${row.winPct} for ${row.name}`);
    }
  }
  const pros = handlers.get_pro_builds({ championId: cfg.liveMatch.myChamp, page: 1 });
  for (const pm of pros.matches) {
    assert.equal(pm.patch, "16.23");
    assertCleanText(pm.proName, "pro build name");
    assertCleanText(pm.team, "pro build team");
    for (const it of pm.finalItems) assertItem(it.id, `pro build ${pm.proName}`);
  }
  const vods = handlers.get_all_vods({});
  assert.ok(vods.length >= 8, "Expected realistic invented VOD list");
  for (const v of vods) {
    assertCleanText(v.title, `vod ${v.videoId}`);
    assertCleanText(v.player, `vod ${v.videoId}`);
    assertCleanText(v.team, `vod ${v.videoId}`);
  }
  const comps = handlers.comps_team({});
  for (const m of comps.roster) assertCleanText(m.riotId, "comps roster");
  const patch = handlers.get_latest_patch_notes({});
  assert.equal(patch.patch, "26.23");
  const prevPatch = handlers.get_latest_patch_notes({ version: "26.22" });
  assert.equal(prevPatch.patch, "26.22");
  const guide = await handlers.get_matchup_guide({ championId: 103, enemyChampionId: 238, position: "mid" });
  assert.ok(guide.guide && !/[–—]/.test(JSON.stringify(guide.guide)), "Ahri vs Zed demo guide must have no em/en dashes");
  assert.ok(!guide.guide.fixture && guide.guide.keyTips.length === 3, "Ahri vs Zed demo guide must be the shipped guide, not the dev fixture");
  const ahriAb = handlers.champion_abilities({ championId: 103 });
  assert.deepEqual(ahriAb.spells[3].cooldowns, [140, 120, 100], "Ahri R cooldown must match Data Dragon 16.23.1");
}

console.log(`website self-check passed (${DEMO_VERSION} / Patch 26.23):`, ids.join(", "));
