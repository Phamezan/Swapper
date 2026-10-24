import { test } from "node:test";
import { strict as assert } from "node:assert";
import { abilityHeading, bugfixLines, championIdFor, championIdInText, changeSegments, classifyChange, formatPatchDate, patchCounts, riftSections } from "./PatchNotesModel.ts";

const entry = (name, context) => ({ name, context, groups: [{ title: "Base Stats", lines: [{ label: "Mana", before: "1", after: "2", text: null }] }], notes: [] });

test("ability headers use metadata before Riot's name and retain the key", () => {
  const abilities = [{ key: "Q", name: "Piltover Peacemaker", icon: "data:icon" }];
  assert.deepEqual(abilityHeading("Q", abilities), { key: "Q", text: "Q - Piltover Peacemaker", icon: "data:icon" });
  assert.equal(abilityHeading("Q - Old name", abilities).text, "Q - Piltover Peacemaker");
  assert.deepEqual(abilityHeading("W – Riot name", []), { key: "W", text: "W - Riot name", icon: null });
  assert.deepEqual(abilityHeading("e", []), { key: "E", text: "E", icon: null });
  assert.equal(abilityHeading("R - ", [{ key: "R", name: " ", icon: null }]).text, "R");
});

test("passive headers use Passive and fall back without inventing a name", () => {
  assert.deepEqual(abilityHeading("Passive - Riot name", [{ key: "P", name: "Headshot", icon: "passive-icon" }]), { key: "P", text: "Passive - Headshot", icon: "passive-icon" });
  assert.equal(abilityHeading("P — Riot name", []).text, "Passive - Riot name");
  assert.equal(abilityHeading("Passive", []).text, "Passive");
  assert.equal(abilityHeading("P", [{ key: "Passive", name: "Headshot", icon: null }]).text, "Passive - Headshot");
  assert.equal(abilityHeading("Base Stats", []), null);
  assert.equal(abilityHeading(null, []), null);
});

test("classifies from Riot's wording, mixed changes are adjusts", () => {
  assert.equal(classifyChange(entry("Azir", "We are buffing his mana.")), "buff");
  assert.equal(classifyChange(entry("Caitlyn", "It's time to nerf her.")), "nerf");
  assert.equal(classifyChange(entry("Caitlyn", "A buff to Q, a nerf to passive.")), "adjust");
  assert.equal(classifyChange({ ...entry("X", null), groups: [] }), "adjust");
});

const line = (label, before, after) => ({ label, before, after, text: null });
const verdict = (...lines) => classifyChange({ name: "Test champion", context: null, groups: [{ title: "R - Ability", lines }], notes: [] });
const colored = value => changeSegments(value).filter(part => part.tone !== "neutral").map(part => [part.text, part.tone]);

test("bugfix champion matching finds full names with punctuation and word boundaries", () => {
  const champions = [{ id: 64, name: "Lee Sin" }, { id: 145, name: "Kai'Sa" }, { id: 22, name: "Ashe" }, { id: 62, name: "Wukong" }];
  assert.equal(championIdInText("Fixed LEE  SIN’s cooldown.", champions), 64);
  assert.equal(championIdInText("Fixed Kai’Sa's tooltip.", champions), 145);
  assert.equal(championIdInText("Fixed MonkeyKing's dash.", champions), 62);
  assert.equal(championIdInText("Fixed a crash when dashes overlap.", champions), null);
  assert.equal(championIdInText("Fixed a tooltip.", champions), null);
});

test("bugfixes flatten paragraphs, groups and notes into one compact list without dropping comparisons", () => {
  const section = { title: "Bugfixes", paragraphs: ["A general fix."], entries: [{
    name: "Lee Sin", context: "A context fix.", notes: ["A note fix."], groups: [{ title: "R", lines: [
      { label: null, before: null, after: null, text: "A spell fix." },
      { label: "Cooldown", before: "12s", after: "10s", text: null },
    ] }],
  }] };
  assert.deepEqual(bugfixLines(section), ["A general fix.", "Lee Sin: A context fix.", "Lee Sin: A spell fix.", "Lee Sin: Cooldown: 12s → 10s", "Lee Sin: A note fix."]);
});

test("Monster Mod: 150% to 165% is a Buff", () => {
  const value = line("Monster Mod", "150%", "165%");
  assert.deepEqual(colored(value), [["165", "buff"]]);
  assert.equal(verdict(value), "buff");
});

test("damage to monster camps is higher-better and is a Buff", () => {
  const value = line("damage to monster camps", "150%", "165%");
  assert.deepEqual(colored(value), [["165", "buff"]]);
  assert.equal(verdict(value), "buff");
});

for (const label of ["monster", "monsters", "camp", "camps", "mod", "modifier", "scaling"]) {
  test(`reviewed higher-better keyword: ${label}`, () => {
    assert.deepEqual(colored(line(label, "10", "20")), [["20", "buff"]]);
    assert.deepEqual(colored(line(label, "20", "10")), [["10", "nerf"]]);
    assert.equal(verdict(line(label, "10", "20")), "buff");
  });
}

test("Scaling Level 10-20: (+5/lvl) to (+7/lvl) is a Buff", () => {
  const value = line("Scaling Level 10-20", "(+5/lvl)", "(+7/lvl)");
  assert.deepEqual(colored(value), [["+7", "buff"]]);
  assert.equal(verdict(value), "buff");
});

test("per-level growth follows the line direction without divisor inversion", () => {
  for (const unit of ["/lvl", " per level"]) {
    const value = line("Scaling", `+5${unit}`, `+7${unit}`);
    assert.deepEqual(colored(value), [["+7", "buff"]]);
    assert.equal(verdict(value), "buff");
    assert.deepEqual(colored(line("Mana Cost", `5${unit}`, `7${unit}`)), [["7", "nerf"]]);
  }
});

test("Self Heal rank and AD growth is a Buff while unchanged ranks and AP stay neutral", () => {
  const value = line("Self Heal", "100 / 135 / 170 (+50% bonus AD) (+75% AP)", "100 / 175 / 250 (+75% bonus AD) (+75% AP)");
  assert.deepEqual(colored(value), [["175", "buff"], ["250", "buff"], ["+75", "buff"]]);
  assert.equal(changeSegments(value).find(part => part.text === "100").tone, "neutral");
  assert.equal(changeSegments(value).filter(part => part.text === "+75")[1].tone, "neutral");
  assert.equal(verdict(value), "buff");
  assert.equal(changeSegments(value).map(part => part.text).join(""), value.after);
});

test("self healing and shielding are higher-better; self damage is lower-better", () => {
  for (const label of ["Self Heal", "Self Shield", "Self Healing", "Self Shielding"]) {
    assert.deepEqual(colored(line(label, "10", "20")), [["20", "buff"]], label);
    assert.deepEqual(colored(line(label, "20", "10")), [["10", "nerf"]], label);
    assert.equal(verdict(line(label, "10", "20")), "buff", label);
  }
  assert.deepEqual(colored(line("Self Damage", "20", "10")), [["10", "buff"]]);
  assert.equal(verdict(line("Self Damage", "20", "10")), "buff");
  assert.deepEqual(colored(line("Self Damage", "10", "20")), [["20", "nerf"]]);
  assert.deepEqual(colored(line("Self Heal Cost", "20", "10")), [["10", "buff"]]);
  for (const label of ["Self Slow", "Self Duration", "Self Speed", "Self", "Self Heal and Self Damage"]) {
    assert.deepEqual(colored(line(label, "10", "20")), [], label);
    assert.equal(verdict(line(label, "10", "20")), "adjust", label);
  }
});

test("Mana Cost: 46 / 42 / 38 / 34 / 30 to 30 broadcasts and is a Buff", () => {
  const value = line("Mana Cost", "46 / 42 / 38 / 34 / 30", "30");
  assert.deepEqual(changeSegments(value), [{ text: "30", tone: "buff" }]);
  assert.equal(verdict(value), "buff");
});

test("Duration: 2.5s to 2.5 / 2.75 / 3s broadcasts and is a Buff", () => {
  const value = line("Duration", "2.5s", "2.5 / 2.75 / 3s");
  assert.deepEqual(colored(value), [["2.75", "buff"], ["3", "buff"]]);
  assert.equal(changeSegments(value).find(part => part.text === "2.5").tone, "neutral");
  assert.equal(verdict(value), "buff");
  assert.equal(changeSegments(value).map(part => part.text).join(""), value.after);
});

test("a collapsed after value uses the verdict across all old ranks, ignoring ties", () => {
  for (const [before, after, tone, kind] of [
    ["10 / 20", "25", "nerf", "nerf"],
    ["10 / 30", "20", "neutral", "adjust"],
    ["20 / 20", "20", "neutral", "adjust"],
    ["30 / 20", "20", "buff", "buff"],
  ]) {
    const value = line("Mana Cost", before, after);
    assert.deepEqual(changeSegments(value), [{ text: after, tone }]);
    assert.equal(verdict(value), kind);
  }
});

test("broadcast after ranks retain independent colours and mixed verdicts", () => {
  const value = line("Damage", "100%", "80% / 100% / 120%");
  assert.deepEqual(colored(value), [["80", "nerf"], ["120", "buff"]]);
  assert.equal(verdict(value), "adjust");
  assert.equal(verdict(line("Damage", "100% / 100% / 120%", "130%")), "buff");
  assert.deepEqual(colored(line("Cooldown", "3s", "2s / 3s / 4s")), [["2", "buff"], ["4", "nerf"]]);
  assert.deepEqual(colored(line("Scaling", "-1.5", "-1 / −.5")), [["-1", "buff"], ["−.5", "buff"]]);
});

test("broadcasting only relaxes simple rank lists with compatible units", () => {
  for (const [before, after] of [
    ["100", "80 / 120%"],
    ["2s", "2 / 3ms"],
    ["10 - 20", "15"],
    ["100", "80 Melee / 120 Ranged"],
    ["100 (+20% AP)", "120"],
    ["10 per 100 AP", "12"],
    ["80 / 150 / 200", "70 / 220"],
  ]) {
    const value = line("Damage", before, after);
    assert.deepEqual(colored(value), [], `${before} → ${after}`);
    assert.equal(verdict(value), "adjust");
  }
  assert.deepEqual(colored(line("Unknown", "1", "2 / 3")), []);
  assert.deepEqual(colored(line("Stacks", "1", "2 / 3")), []);
});

test("Talon E attack speed scaling colours the smaller per divisor green and is a Buff", () => {
  const value = line("Attack Speed Scaling", "1% per 5% bonus Attack Speed from permanent sources", "1% per 4% bonus Attack Speed from permanent sources");
  assert.deepEqual(colored(value), [["4", "buff"]]);
  assert.equal(changeSegments(value).find(part => part.text === "1").tone, "neutral");
  assert.equal(verdict(value), "buff");
  assert.equal(changeSegments(value).map(part => part.text).join(""), value.after);
});

for (const unit of ["%", " AP", " stacks"]) {
  test(`per-number divisor direction is inverted for ${unit.trim()}`, () => {
    const decrease = line("Damage", `10 per 100${unit}`, `10 per 80${unit}`);
    const increase = line("Damage", `10 PER 80${unit}`, `10 PER 100${unit}`);
    assert.deepEqual(colored(decrease), [["80", "buff"]]);
    assert.equal(verdict(decrease), "buff");
    assert.deepEqual(colored(increase), [["100", "nerf"]]);
    assert.equal(verdict(increase), "nerf");
  });
}

test("divisors invert lower-better stats and retain independent numerator signals", () => {
  const cost = line("Mana Cost", "20 per 100 AP", "20 per 80 AP");
  assert.deepEqual(colored(cost), [["80", "nerf"]]);
  assert.equal(verdict(cost), "nerf");
  const mixed = line("Damage", "10 per 100 AP", "12 per 120 AP");
  assert.deepEqual(colored(mixed), [["12", "buff"], ["120", "nerf"]]);
  assert.equal(verdict(mixed), "adjust");
});

test("only a number directly after the word per is a divisor", () => {
  for (const before of ["10 per level 5", "10 upper 5", "10 percent 5"]) {
    const value = line("Damage", before, before.replace("5", "4"));
    assert.deepEqual(colored(value), [["4", "nerf"]]);
  }
  assert.deepEqual(colored(line("Stacks", "1 per 10 stacks", "1 per 8 stacks")), []);
  assert.equal(verdict(line("Damage", "10 per 100 AP", "10 per 80 AD")), "adjust");
});

test("Lee Sin's kick colours each reduced rank red and leaves the unchanged AD ratio neutral", () => {
  const kick = line("Kick Damage", "175 / 400 / 625 (+200% bonus AD)", "150 / 350 / 550 (+200% bonus AD)");
  assert.deepEqual(colored(kick), [["150", "nerf"], ["350", "nerf"], ["550", "nerf"]]);
  assert.equal(changeSegments(kick).find(part => part.text === "+200").tone, "neutral");
  assert.equal(changeSegments(kick).map(part => part.text).join(""), kick.after);
  assert.equal(verdict(kick), "nerf");
});

for (const [label, before, after, tone] of [
  ["Cooldown", "12s", "15s", "nerf"],
  ["Cooldown", "15 seconds", "12s", "buff"],
  ["Mana Cost", "80 / 90 / 100", "70 / 80 / 90", "buff"],
  ["Energy Cost", "40", "50", "nerf"],
  ["Health Cost", "10%", "8%", "buff"],
  ["Cast Time", "0.25 seconds", "0.3 seconds", "nerf"],
  ["Wind-up", "0.5s", "0.4s", "buff"],
  ["Windup", "0.4s", "0.5s", "nerf"],
  ["Delay", ".5s", ".25s", "buff"],
  ["Recharge", "15", "18", "nerf"],
  ["Lockout", "2", "1", "buff"],
  ["Time to detonate", "2", "3", "nerf"],
  ["Base Health", "600", "650", "buff"],
  ["Armor / MR", "25 / 30", "30 / 35", "buff"],
  ["Slow Duration", "2s", "1.5s", "nerf"],
  ["Ability Power Ratio", "(+40% AP)", "(+50% AP)", "buff"],
  ["Cooldown Reduction", "10%", "15%", "buff"],
  ["Damage Taken Reduction", "20%", "15%", "nerf"],
  ["Mana Cost Refund", "20", "30", "buff"],
]) {
  test(`${label}: ${before} → ${after} is ${tone}`, () => {
    const value = line(label, before, after);
    assert.ok(colored(value).length > 0);
    assert.ok(colored(value).every(([, kind]) => kind === tone));
    assert.equal(verdict(value), tone);
  });
}

for (const label of [
  "AD", "AP", "base AD", "AD growth", "AP ratio", "Attack Damage", "Lethality", "Crit", "Critical Strike", "Radius", "Width", "Area",
  "Execute Threshold", "Health Threshold for Execute", "Damage Reduction",
  "Increased Damage Taken", "Bonus Damage Taken", "Damage Amplification", "Amplified", "Vulnerability",
  "Duration", "Speed",
]) {
  test(`reviewed higher-better stat: ${label}`, () => {
    const increase = line(label, "10", "20");
    const decrease = line(label, "20", "10");
    assert.deepEqual(colored(increase), [["20", "buff"]]);
    assert.deepEqual(colored(decrease), [["10", "nerf"]]);
    assert.equal(verdict(increase), "buff");
    assert.equal(verdict(decrease), "nerf");
  });
}

test("AD and AP match whole words, never substrings", () => {
  for (const label of ["Shadow", "Adaptation", "Trap", "Map"]) {
    assert.deepEqual(colored(line(label, "10", "20")), [], label);
  }
});

test("charge counts stay neutral and yield Adjustment despite other matching words", () => {
  for (const label of ["Charges", "Max Charges", "Maximum Charges", "Charge Count", "Bonus Charge Count", "Damage Charges", "AP Charges", "Cooldown Charges", "Execute Threshold Charges", "Bonus Damage Taken Charges"]) {
    for (const [before, after] of [["1", "2"], ["2", "1"]]) {
      const value = line(label, before, after);
      assert.deepEqual(colored(value), [], label);
      assert.equal(verdict(value), "adjust", label);
    }
  }
});

test("Locke unused charge refund is a Buff, with a red lower end and green upper end", () => {
  const value = line("Unused Charge Refund", "20 - 35% mana and cooldown refund", "10 - 40% mana and cooldown refund");
  assert.deepEqual(colored(value), [["10", "nerf"], ["40", "buff"]]);
  assert.equal(verdict(value), "buff");
  assert.equal(changeSegments(value).map(part => part.text).join(""), value.after);
});

test("refund is higher-better on its own and overrides charge-count and cost wording", () => {
  for (const label of ["Refund", "Charges Refund", "Refund Charges", "Mana and Cooldown Refund", "Charges Cost Refund"]) {
    assert.deepEqual(colored(line(label, "10", "20")), [["20", "buff"]], label);
    assert.deepEqual(colored(line(label, "20", "10")), [["10", "nerf"]], label);
    assert.equal(verdict(line(label, "10", "20")), "buff", label);
  }
  assert.deepEqual(colored(line("Charges Cost", "20", "10")), [["10", "buff"]]);
  assert.deepEqual(colored(line("Charges Cooldown", "10s", "12s")), [["12", "nerf"]]);
});

for (const separator of [" - ", "-", "–", " – ", "—", "−"]) {
  test(`range verdict prioritizes the upper end (${JSON.stringify(separator)})`, () => {
    const range = (low, high) => `${low}${separator}${high}%`;
    assert.equal(verdict(line("Refund", range(20, 35), range(10, 40))), "buff");
    assert.equal(verdict(line("Damage", range(10, 40), range(20, 35))), "nerf");
    assert.equal(verdict(line("Damage", range(40, 200), range(20, 200))), "nerf");
    assert.equal(verdict(line("Damage", range(20, 200), range(40, 200))), "buff");
    assert.equal(verdict(line("Damage", range(20, 200), range(20, 200))), "adjust");
  });
}

test("range verdicts retain stat direction and independent line/ratio signals", () => {
  assert.equal(verdict(line("Cooldown", "12 - 20s", "10 - 22s")), "nerf");
  assert.equal(verdict(line("Damage", "20 - 35 (+10% AP)", "10 - 40 (+5% AP)")), "adjust");
  assert.equal(verdict(line("Refund", "20 - 35%", "10 - 40%"), line("Damage", "100", "90")), "adjust");
  assert.equal(verdict(line("Damage", "80 / 150 / 200", "70 / 150 / 220"), line("Health", "100", "110")), "adjust", "mixed rank arrays still contribute both signals");
});

test("Locke grey health store compares through a removed level note and is a Nerf", () => {
  const value = line("Grey Health Store Based on Current Health", "40 - 200 (both level based)", "20 - 200");
  assert.deepEqual(colored(value), [["20", "nerf"]]);
  assert.equal(changeSegments(value).find(part => part.text === "200").tone, "neutral");
  assert.equal(verdict(value), "nerf");
});

for (const qualifier of ["both level based", "scales with level", "based on level"]) {
  test(`added/removed scaling note contributes a signal: ${qualifier}`, () => {
    assert.equal(verdict(line("Health", `40 - 200 (${qualifier})`, "40 - 200")), "nerf");
    const gained = line("Health", "40 - 200", `40 - 200 (${qualifier})`);
    assert.equal(verdict(gained), "buff");
    assert.deepEqual(colored(gained), [], "unchanged numbers stay neutral");
    assert.equal(changeSegments(gained).map(part => part.text).join(""), gained.after);
    assert.equal(verdict(line("Health", `40 - 200 (${qualifier})`, "50 - 250")), "adjust", "qualifier loss remains a nerf signal against a numeric buff");
  });
}

test("only added/removed text notes relax shape matching; formulas and changed notes stay guarded", () => {
  assert.deepEqual(colored(line("Damage", "100 (per hit)", "110")), [["110", "buff"]]);
  assert.deepEqual(colored(line("Damage", "100 (per hit)", "110 (per hit) (new)")), [["110", "buff"]]);
  for (const [before, after] of [
    ["100 (melee)", "110 (ranged)"],
    ["100 (+20% AP)", "110"],
    ["100 (+20% AP)", "110 (+20% AD)"],
    ["40 - 200 (level 18)", "20 - 200"],
    ["40 - 200", "20% - 200%"],
  ]) assert.deepEqual(colored(line("Damage", before, after)), [], `${before} → ${after}`);
});

test("thresholds without execute remain neutral", () => {
  for (const label of ["Threshold", "Health Threshold", "Damage Threshold", "Execution Threshold"]) {
    assert.deepEqual(colored(line(label, "10", "20")), [], label);
    assert.equal(verdict(line(label, "10", "20")), "adjust", label);
  }
});

test("unqualified damage taken is neutral because its recipient is unknown", () => {
  for (const label of ["Damage Taken", "Base Damage Taken"]) {
    for (const [before, after] of [["100%", "95%"], ["95%", "100%"]]) {
      assert.deepEqual(colored(line(label, before, after)), [], label);
      assert.equal(verdict(line(label, before, after)), "adjust", label);
    }
  }
});

test("cost stays lower-better, including AD and AP cost labels", () => {
  for (const label of ["Cost", "AD Cost", "AP Cost"]) {
    assert.deepEqual(colored(line(label, "20", "10")), [["10", "buff"]]);
    assert.deepEqual(colored(line(label, "10", "20")), [["20", "nerf"]]);
  }
});

test("generic stack counts remain neutral and recognized per-stack yields stay higher-better", () => {
  for (const label of ["Stacks", "Maximum Stacks"]) {
    assert.deepEqual(colored(line(label, "1", "2")), [], label);
  }
  for (const label of ["Damage per stack", "Attack Speed per stack"]) {
    assert.deepEqual(colored(line(label, "1", "2")), [["2", "buff"]], label);
  }
});

for (const label of ["Stacks Required", "Stacks Needed", "Stacks to Unlock", "Stacks to Empower", "Stacks to Transform", "Stacks to Evolve", "Stack Threshold", "Stack Checkpoints"]) {
  test(`${label} is lower-better`, () => {
    const increased = line(label, "25", "30");
    const decreased = line(label, "30", "25");
    assert.deepEqual(colored(increased), [["30", "nerf"]]);
    assert.equal(verdict(increased), "nerf");
    assert.deepEqual(colored(decreased), [["25", "buff"]]);
    assert.equal(verdict(decreased), "buff");
  });
}

test("Smolder bonuses at 25 / 125 / 225 stacks to 30 / 140 / 250 stacks are a Nerf", () => {
  const value = line("Smolder bonuses at", "25 / 125 / 225 stacks", "30 / 140 / 250 stacks");
  assert.deepEqual(colored(value), [["30", "nerf"], ["140", "nerf"], ["250", "nerf"]]);
  assert.equal(verdict(value), "nerf");
  assert.equal(changeSegments(value).map(part => part.text).join(""), value.after);
});

test("stack requirements and checkpoints can be described in values, including unlabeled lines", () => {
  for (const wording of ["stacks required", "stacks needed", "stacks to unlock", "stacks to empower", "stacks to transform", "stacks to evolve"]) {
    const value = line(null, `25 ${wording}`, `30 ${wording}`);
    assert.deepEqual(colored(value), [["30", "nerf"]], wording);
    assert.equal(verdict(value), "nerf", wording);
  }
  const checkpoint = line("Bonuses", "at 25 / 125 / 225 stacks", "at 30 / 140 / 250 stacks");
  assert.deepEqual(colored(checkpoint), [["30", "nerf"], ["140", "nerf"], ["250", "nerf"]]);
  assert.equal(verdict(checkpoint), "nerf");
});

test("Nasus Stacks per cannon kill: 6 to 12 is a Buff", () => {
  const value = line("Stacks per cannon kill", "6", "12");
  assert.deepEqual(colored(value), [["12", "buff"]]);
  assert.equal(verdict(value), "buff");
});

for (const label of ["Stacks per Kill", "Stacks per Cannon", "Stacks per Minion", "Stacks per Hit", "Stacks per Takedown", "Stacks Gained", "Stacks Granted"]) {
  test(`${label} is higher-better`, () => {
    assert.deepEqual(colored(line(label, "6", "12")), [["12", "buff"]]);
    assert.equal(verdict(line(label, "6", "12")), "buff");
    assert.deepEqual(colored(line(label, "12", "6")), [["6", "nerf"]]);
    assert.equal(verdict(line(label, "12", "6")), "nerf");
  });
}

test("stack earnings and yields described in values are higher-better", () => {
  for (const wording of ["stacks gained", "stacks granted", "stacks per kill", "stacks per cannon", "stacks per minion", "stacks per hit", "stacks per takedown", "per stack", "per soul", "per mark", "per chime"]) {
    const value = line(null, `1 ${wording}`, `2 ${wording}`);
    assert.deepEqual(colored(value), [["2", "buff"]], wording);
    assert.equal(verdict(value), "buff", wording);
  }
});

test("Veigar AP per stack: 1 to 2 is a Buff and its yield is not a divisor", () => {
  const value = line("AP per stack", "1", "2");
  assert.deepEqual(colored(value), [["2", "buff"]]);
  assert.equal(verdict(value), "buff");
  assert.deepEqual(colored(line("AP", "1 per stack", "2 per stack")), [["2", "buff"]]);
});

for (const unit of ["stack", "soul", "mark", "chime"]) {
  test(`per ${unit} yields are higher-better without numeric-divisor inversion`, () => {
    assert.deepEqual(colored(line(`Value per ${unit}`, "1", "2")), [["2", "buff"]]);
    assert.equal(verdict(line(`Value per ${unit}`, "1", "2")), "buff");
    assert.deepEqual(colored(line(`Value per ${unit}`, "2", "1")), [["1", "nerf"]]);
    assert.deepEqual(colored(line("Damage", `1 per 10 ${unit}s`, `1 per 8 ${unit}s`)), [["8", "buff"]]);
  });
}

test("maximum stacks and unclassified stack wording stay neutral despite stat keywords", () => {
  for (const label of ["Maximum Stacks", "Max Stacks", "AP Max Stacks", "Maximum Stacks per Kill", "Stacks", "Stack Count", "Damage Stacks", "Stack Damage", "Stacks per Second", "Stacks Lost", "Stacks Decay", "Stacks Required and Stacks Gained"]) {
    const value = line(label, "10", "20");
    assert.deepEqual(colored(value), [], label);
    assert.equal(verdict(value), "adjust", label);
  }
  for (const wording of ["maximum stacks", "max stacks", "stacks", "stacks lost"]) {
    const value = line("Damage", `10 ${wording}`, `20 ${wording}`);
    assert.deepEqual(colored(value), [], wording);
    assert.equal(verdict(value), "adjust", wording);
  }
});

test("stack words use word boundaries and stack requirements support rank broadcasting", () => {
  assert.equal(verdict(line("Stacks Required", "25 / 30", "20")), "buff");
  assert.deepEqual(colored(line("Stacks Required", "25 / 30", "20")), [["20", "buff"]]);
  assert.deepEqual(colored(line("Damage Haystack", "10", "20")), [["20", "buff"]]);
});

test("a mixed rank line is red / neutral / green and aggregates to Adjustment", () => {
  const value = line("Damage", "80 / 150 / 200", "70 / 150 / 220");
  assert.deepEqual(changeSegments(value).filter(part => /^\d+$/.test(part.text)), [
    { text: "70", tone: "nerf" }, { text: "150", tone: "neutral" }, { text: "220", tone: "buff" },
  ]);
  assert.equal(verdict(value), "adjust");
  assert.equal(verdict(line("Damage", "100", "110"), line("Cooldown", "12s", "15s")), "adjust");
});

test("compares ratios separately and preserves negative signs, decimals and source formatting", () => {
  const value = line("Damage", "80 / -1.5 / −2.5 (+200% bonus AD)", "90 / −2.5 / -1.5 (+175% bonus AD)");
  assert.deepEqual(colored(value), [["90", "buff"], ["−2.5", "nerf"], ["-1.5", "buff"], ["+175", "nerf"]]);
  assert.equal(changeSegments(value).map(part => part.text).join(""), value.after);
  assert.deepEqual(colored(line("Health", "1,000", "1,100")), [["1,100", "buff"]]);
  assert.deepEqual(colored(line("Damage", "60-100", "60-80")), [["80", "nerf"]], "a range dash is not a negative sign");
  assert.deepEqual(colored(line("Damage", "− 5", "- 3")), [["- 3", "buff"]]);
});

for (const [before, after] of [
  ["80 / 150 / 200", "70 / 220"],
  ["100", "100%"],
  ["12s", "120ms"],
  ["100 (+200% bonus AD)", "110 (+200% total AD)"],
  ["100 (+20% AP)", "110 (+20% AD)"],
  ["100 / 200", "110–220"],
  [null, "100"],
  ["100", null],
  ["None", "Now deals 100 damage"],
]) {
  test(`incomparable shapes remain neutral: ${before} → ${after}`, () => {
    const value = line("Damage", before, after);
    assert.deepEqual(colored(value), []);
    assert.equal(changeSegments(value).map(part => part.text).join(""), after ?? "");
    assert.equal(verdict(value), "adjust");
  });
}

test("unknown or ambiguous labels, unlabeled lines and unchanged values remain neutral", () => {
  for (const label of [null, "Mystery Stat", "Health Threshold", "Self Slow", "Cooldown and Damage", "Damage Penalty"]) {
    assert.deepEqual(colored(line(label, "10", "20")), [], label);
    assert.equal(verdict(line(label, "10", "20")), "adjust", label);
  }
  assert.deepEqual(colored(line("Damage", "100", "100")), []);
});

test("Riot's explicit verdict overrides numeric aggregation; summary counts use the fallback", () => {
  const reduced = { name: "Lee Sin", context: null, notes: [], groups: [{ title: "R", lines: [line("Kick Damage", "175 / 400 / 625 (+200% bonus AD)", "150 / 350 / 550 (+200% bonus AD)")] }] };
  assert.equal(classifyChange({ ...reduced, context: "We are buffing his other tools." }), "buff");
  assert.equal(classifyChange({ ...reduced, context: "A buff here and a nerf there." }), "adjust");
  assert.deepEqual(patchCounts({ sections: [{ title: "Champions", entries: [reduced] }] }), { buffs: 0, nerfs: 1, adjusts: 0, champions: 1, items: 0, lines: 1 });
});

test("matches champion names across punctuation and aliases", () => {
  const champions = [{ id: 62, name: "Wukong" }, { id: 20, name: "Nunu & Willump" }, { id: 145, name: "Kai'Sa" }];
  assert.equal(championIdFor("Nunu & Willump", champions), 20);
  assert.equal(championIdFor("Kai'Sa", champions), 145);
  assert.equal(championIdFor("MonkeyKing", champions), 62);
  assert.equal(championIdFor("Dawncore", champions), null);
});

test("counts champions by kind and every change line", () => {
  const notes = { sections: [
    { title: "Champions", entries: [entry("Azir", "buffing"), entry("Caitlyn", "nerf her")], paragraphs: [] },
    { title: "Items", entries: [entry("Dawncore", "buffing")], paragraphs: [] },
    { title: "Bugfixes & QoL Changes", entries: [{ name: "", context: null, groups: [{ title: null, lines: [{ label: null, before: null, after: null, text: "Fixed." }] }], notes: [] }], paragraphs: [] },
  ] };
  assert.deepEqual(patchCounts(notes), { buffs: 1, nerfs: 1, adjusts: 0, champions: 2, items: 1, lines: 4 });
});

test("formats the publish date and tolerates junk", () => {
  assert.equal(formatPatchDate("2026-10-20T18:00:00.000Z"), "20 October 2026");
  assert.equal(formatPatchDate(null), null);
});

test("keeps only Rift gameplay, orders champions first and combines core systems", () => {
  const section = title => ({ title, paragraphs: [], entries: [entry(title, "buffing")] });
  const sections = ["Items", "ARAM", "Arena", "ARAM Mayhem", "URF", "Nexus Blitz", "Swarm", "Doom Bots", "TFT", "Skins", "Upcoming skins", "Ranked", "Mythic Shop", "Champions", "Runes", "Systems", "Summoner’s Rift Changes", "Jungle Changes", "Summoner Spells", "ARAM Champion Changes", "Arena Items"].map(section);
  const original = structuredClone(sections);
  const filtered = riftSections(sections);
  assert.deepEqual(filtered.map(s => s.title), ["Champions", "Items", "Runes", "Systems"]);
  assert.equal(filtered[3].entries.length, 4);
  assert.deepEqual(patchCounts({ sections: filtered }), { buffs: 1, nerfs: 0, adjusts: 0, champions: 1, items: 1, lines: 7 });
  assert.deepEqual(sections, original, "leaves the fetched/cache data untouched");
});

test("filters mode-specific subsections and mixed bugfix lists, dropping empty sections", () => {
  const sections = [
    { title: "Champions", paragraphs: [], entries: [entry("Azir", "buffing"), entry("ARAM balance", "buffing")] },
    { title: "Bugfixes & QoL Changes", paragraphs: ["Fixed jungle pathing.", "Fixed Arena tooltips."], entries: [
      { name: "", context: null, notes: ["Fixed an ARAM Mayhem crash."], groups: [
        { title: "Arena", lines: [{ text: "Fixed champion damage." }] },
        { title: null, lines: [{ text: "Fixed an ARAM reroll bug." }, { text: "Fixed a ward vision bug." }] },
      ] },
    ] },
    { title: "Runes", paragraphs: [], entries: [{ name: "Arena runes", groups: [], notes: [], context: null }] },
  ];
  const filtered = riftSections(sections);
  assert.deepEqual(filtered.map(s => s.title), ["Champions", "Bugfixes"]);
  assert.deepEqual(filtered[0].entries.map(e => e.name), ["Azir"]);
  assert.deepEqual(filtered[1].paragraphs, ["Fixed jungle pathing."]);
  assert.deepEqual(filtered[1].entries[0].notes, []);
  assert.deepEqual(filtered[1].entries[0].groups, [{ title: null, lines: [{ text: "Fixed a ward vision bug." }] }]);
});
