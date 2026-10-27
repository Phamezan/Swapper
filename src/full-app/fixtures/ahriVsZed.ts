import type { MatchupGuide } from "../matchupGuide";
import { zedCore } from "./zedCore.ts";

// Pilot content researched 2026-10-27 from the linked guides, rewritten in our own words.
// Dev-only until the content repo exists (milestone 2); not yet reviewed.
export const ahriVsZed: MatchupGuide = {
  champion: 103, enemy: 238, lane: "mid", patch: "26.21", reviewed: false, fixture: true,
  difficulty: "Skill matchup",
  favoredWhen: "Even lane. Zed's pressure peaks at levels 3–5 whenever Living Shadow is up. Once you hold Charm and Spirit Rush, a disciplined Ahri rarely gives him a kill angle.",
  keyTips: [
    { scope: "pair", text: "Levels 1–5, track two cooldowns: Razor Shuriken ({cd:238:Q}) and Living Shadow ({cd:238:W}). Trade while his shadow is down; give ground while it is up.", sourceIds: ["peng04"], abilityKeys: [{ champion: 238, key: "Q" }, { champion: 238, key: "W" }] },
    { scope: "pair", text: "Never waste Charm. Zed mains say they avoid Death Mark into an Ahri who still has Charm, because it stops the combo. Holding E is what keeps you safe.", sourceIds: ["v222", "aikopanda"], abilityKeys: [{ champion: 103, key: "E" }, { champion: 238, key: "R" }] },
    { scope: "pair", text: "Death Mark always drops Zed behind you. Aim Charm behind yourself as he lands. If he placed a shadow first, he can swap out: wait for the swap, then Charm.", sourceIds: ["peng04", "aikopanda"], abilityKeys: [{ champion: 103, key: "E" }, { champion: 238, key: "R" }, { champion: 238, key: "W" }] },
  ],
  enemyTips: zedCore,
  sections: {
    overview: { tips: [] },
    "early-lane": { tips: [
      { scope: "pair", heading: "Levels 1–3", text: "Level 1: throw Orb of Deception through Zed and the wave together, so every Q is both poke and farm.", sourceIds: ["peng04"], abilityKeys: [{ champion: 103, key: "Q" }] },
      { scope: "pair", text: "Level 2: keep trades short. Auto, Fox-Fire, auto procs Electrocute; step out right after. Do not extend unless you are clearly ahead on health.", sourceIds: ["peng04"], abilityKeys: [{ champion: 103, key: "W" }] },
      { scope: "pair", text: "Level 3: take Charm but do not spend it on poke. A missed Charm ({cd:103:E}) is Zed's signal to walk up and all-in.", sourceIds: ["peng04"], abilityKeys: [{ champion: 103, key: "E" }] },
      { scope: "pair", heading: "Trading & cooldowns", text: "Living Shadow is both his engage and his escape. When he spends it on the wave or a trade, you have its full cooldown ({cd:238:W}) to trade back with autos and Q.", sourceIds: ["peng04"], abilityKeys: [{ champion: 238, key: "W" }, { champion: 103, key: "Q" }] },
      { scope: "pair", text: "Below about 60% health before level 6, look for a reset: that is the range his first all-in can finish.", sourceIds: ["peng04"] },
    ] },
    "mid-lane": { timeline: [
      { at: "Levels 1–2", champion: "Short Electrocute trades; Q through Zed and the wave.", enemy: "Farms with Q; mostly wants to reach level 3 healthy." },
      { at: "Levels 3–5", champion: "Hold Charm. Trade only while his shadow is down.", enemy: "W–E–Q burst online; strongest when you are low or Charm is down." },
      { at: "Level 6", champion: "Swap to Oracle Lens; use lane priority to check side lanes and objectives.", enemy: "Death Mark, but rarely commits into a held Charm." },
      { at: "Items", champion: "Zhonya's Hourglass is the standard answer to Death Mark.", enemy: "Watches for your Spirit Rush dashes to catch you with W–Q." },
    ], tips: [
      { scope: "pair", heading: "Wave management", text: "With two points in Orb of Deception, two Qs clear the caster minions. Clear fast, take priority, and use it to move first instead of sitting in his shadow range.", sourceIds: ["peng04"], abilityKeys: [{ champion: 103, key: "Q" }] },
      { scope: "pair", text: "After level 6, fight from the fog: with priority, wait in a mid-lane brush or check side lanes, then return for the next wave. Zed's angles get worse when he cannot see you.", sourceIds: ["peng04"] },
    ] },
    "win-condition": { tips: [
      { scope: "pair", heading: "Your win condition", text: "Turn the lane into a non-event: reach level 6 healthy, keep Charm and Spirit Rush up, and let your priority convert into side-lane plays and objectives.", sourceIds: ["peng04", "aikopanda"] },
      { scope: "pair", text: "Use Spirit Rush dashes deliberately. Zed looks for W–Q right after a dash, so do not burn all charges to chase; keep one to answer his engage.", sourceIds: ["v222"], abilityKeys: [{ champion: 103, key: "R" }, { champion: 238, key: "W" }, { champion: 238, key: "Q" }] },
      { scope: "pair", heading: "Common mistakes", text: "Spending Charm on poke. Without it, Death Mark becomes safe for him; with it, he usually does not try.", sourceIds: ["v222", "aikopanda"], abilityKeys: [{ champion: 103, key: "E" }] },
    ] },
  },
  itemization: [
    { scope: "pair", text: "Zhonya's Hourglass is the item answer to Zed: time the stasis for the moment Death Mark detonates. Peng lists it as a third-item option specifically into assassins like Zed.", itemIds: [3157], sourceIds: ["peng04"] },
  ],
  sources: [
    { id: "peng04", author: "Peng04", rankClaim: "Challenger · Ahri main", url: "https://www.mobafire.com/league-of-legends/build/26-20-pengs-ahri-guide-5-1mil-points-13x-challenger-648700", kind: "mobafire" },
    { id: "v222", author: "V222", rankClaim: "Challenger EUW · Zed main", url: "https://www.mobafire.com/league-of-legends/build/all-matchups-26-14-zed-guide-2000lp-euw-challenger-652354", kind: "mobafire" },
    { id: "aikopanda", author: "AikoPanda", rankClaim: "Zed main", url: "https://www.mobafire.com/league-of-legends/build/26-20-video-guides-hidden-combos-all-matchups-zed-all-in-one-guide-652800", kind: "mobafire" },
  ],
};
