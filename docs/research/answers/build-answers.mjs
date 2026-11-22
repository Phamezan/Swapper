// Build pro-play Answers dataset (research data only). Reuses queries/scope from docs/research/comps-sources.md.
// Run: node docs/research/answers/build-answers.mjs   (raw Cargo rows cached in ./raw/, delete to re-query)
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildNameMaps, runePagesForAnswer } from "../../../src/full-app/proRunePages.ts";

// ===== THRESHOLDS (spec docs/specs/comps-rework.md §5 — edit only here) =====
const THRESHOLDS = {
  laneMinGames: 8,        // pro lane counter: same-role opponent, >= games (user: hide < 8)
  laneMinWinRate: 0.55,   // pro lane counter: answer win rate >= 55%
  draftMinGames: 8,       // pro draft answer: picked after threat, >= games
  draftMinLift: 1.5,      // pro draft answer: lift >= 1.5x base pick rate
  draftMinWinRate: 0.5,   // pro draft answer: win rate >= 50% (losing picks are habits, not answers)
  windowPatches: 4,       // window = last N patches present; rest only tags "older meta"
  ua: "SwapperCompsResearch/1.0 (Swapper League companion; pro-play Answers dataset; repo Swapper)",
  pauseMs: 10_000, rateLimitWaitMs: 60_000,
};
// ===== END THRESHOLDS =====

const DIR = dirname(fileURLToPath(import.meta.url));
const RAW = join(DIR, "raw"); mkdirSync(RAW, { recursive: true });
const SCOPE = [
  "2026 Season World Championship%", "LCK/2026 Season/Rounds 3-4%", "LCK/2026 Season/Season Play-In%",
  "LCK/2026 Season/Season Playoffs%", "LPL/2026 Season/Split 3%", "LPL/2026 Season/Grand Finals%",
  "LEC/2026 Season/Summer Season%", "LEC/2026 Season/Summer Playoffs%", "LCS/2026 Season/Summer Season%",
  "LCS/2026 Season/Summer Playoffs%",
];
const WHERE = "(" + SCOPE.map((s) => `GameId LIKE "${s}"`).join(" OR ") + ")";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function cargo(table, fields, file) {
  const cache = join(RAW, file);
  if (existsSync(cache)) { const rows = JSON.parse(readFileSync(cache, "utf8")); console.log(`${file}: cache ${rows.length} rows`); return rows; }
  const rows = [];
  for (let offset = 0; ; offset += 500) {
    const url = "https://lol.fandom.com/api.php?" + new URLSearchParams({
      action: "cargoquery", format: "json", tables: table, fields, where: WHERE, limit: "500", offset: String(offset),
    });
    let json;
    for (let attempt = 0; attempt < 2; attempt++) {
      if (rows.length || offset || attempt) await sleep(attempt ? THRESHOLDS.rateLimitWaitMs : THRESHOLDS.pauseMs);
      const res = await fetch(url, { headers: { "User-Agent": THRESHOLDS.ua } });
      json = await res.json();
      if (!json.error || !String(json.error.info || "").includes("ratelimited")) break;
      console.log(`ratelimited on ${table} offset ${offset}, attempt ${attempt + 1}`);
      if (attempt === 1) throw new Error(`STOP: ratelimited twice on ${table} offset ${offset}: ${json.error.info}`);
    }
    if (json.error) throw new Error(`Cargo error ${table}: ${JSON.stringify(json.error)}`);
    const page = (json.cargoquery || []).map((r) => r.title);
    rows.push(...page);
    console.log(`${file}: offset ${offset} +${page.length} (total ${rows.length})`);
    if (page.length < 500) break;
  }
  writeFileSync(cache, JSON.stringify(rows, null, 1));
  return rows;
}

const expand = (base) => Array.from({ length: 5 }, (_, i) => `${base}${i + 1}`).join(",");
const [sg, pab, msg] = await Promise.all([]).then(async () => [
  await cargo("ScoreboardGames", "GameId,Tournament,Team1,Team2,Winner,DateTime_UTC,Patch,VOD,N_GameInMatch,Team1Picks,Team2Picks", "scoreboardgames.json"),
  await cargo("PicksAndBansS7", `GameId,MatchId,Team1,Team2,Winner,${expand("Team1Ban")},${expand("Team2Ban")},${expand("Team1Pick")},${expand("Team2Pick")},${expand("Team1Role")},${expand("Team2Role")},Team1PicksByRoleOrder,Team2PicksByRoleOrder`, "picksandbanss7.json"),
  await cargo("MatchScheduleGame", "GameId,MatchId,Blue,Red,Winner,Vod,N_GameInMatch", "matchschedulegame.json"),
]);

const byId = (rows) => new Map(rows.map((r) => [r.GameId, r]));
const SG = byId(sg), PAB = byId(pab), MSG = byId(msg);
const ROLES = ["top", "jungle", "mid", "bot", "support"];
const ORDER = [["B", 0], ["R", 0], ["R", 1], ["B", 1], ["B", 2], ["R", 2], ["R", 3], ["B", 3], ["B", 4], ["R", 4]]; // B1 R1R2 B2B3 R3R4 B4B5 R5

function side(pabRow, teamKey, blueName) {
  const picks = [1, 2, 3, 4, 5].map((i) => pabRow[`${teamKey}Pick${i}`]).filter(Boolean);
  const byRole = {};
  const roleList = String(pabRow[`${teamKey}PicksByRoleOrder`] || "").split(",").map((s) => s.trim()).filter(Boolean);
  if (roleList.length === 5) ROLES.forEach((r, i) => (byRole[r] = roleList[i]));
  else picks.forEach((c, i) => { const r = String(pabRow[`${teamKey}Role${i + 1}`] || "").toLowerCase(); if (ROLES.includes(r)) byRole[r] = c; });
  return { team: pabRow[teamKey], isBlue: pabRow[teamKey] === blueName, picks, byRole,
           roleOf: Object.fromEntries(Object.entries(byRole).map(([r, c]) => [c, r])) };
}

const games = [];
for (const [id, p] of PAB) {
  const s = SG.get(id), m = MSG.get(id);
  if (!s || !m) continue;
  const blueName = m.Blue || s.Team1;
  const t1 = side(p, "Team1", blueName), t2 = side(p, "Team2", blueName);
  const blue = t1.isBlue ? t1 : t2, red = t1.isBlue ? t2 : t1;
  const pos = new Map();
  ORDER.forEach(([sd, idx], i) => { const c = (sd === "B" ? blue : red).picks[idx]; if (c) pos.set(c, i + 1); });
  // Cargo Winner is "1"/"2" = first/second listed team (SG Team1 is blue), not a team name.
  const winnerTeam = s.Winner === "1" ? s.Team1 : s.Winner === "2" ? s.Team2 : (s.Winner || (m.Winner === "1" ? m.Blue : m.Red));
  games.push({ gameId: id, tournament: s.Tournament, patch: s.Patch || "", winner: winnerTeam,
    vod: s.VOD || null, blue, red, pos });
}
console.log(`joined games: ${games.length} (SG ${sg.length}, PAB ${pab.length}, MSG ${msg.length})`);
const patchCmp = (a, b) => { const x = a.split(".").map(Number), y = b.split(".").map(Number); return x[0] - y[0] || x[1] - y[1]; };
const allPatches = [...new Set(games.map((g) => g.patch).filter(Boolean))].sort(patchCmp);
const patches = allPatches.slice(-THRESHOLDS.windowPatches);
const inWindow = (g) => patches.includes(g.patch);
console.log(`patches present: ${allPatches.join(", ")} | window: ${patches.join(", ")}`);

const slug = (n) => n.toLowerCase().replace(/['.]/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
const ref = (g, winnerSide) => ({ gameId: g.gameId, tournament: g.tournament, teams: `${g.blue.team} vs ${g.red.team}`,
  winner: g.winner, patch: g.patch, vod: g.vod, _win: g.winner === winnerSide.team });
const finish = (gamesList) => {
  const wins = gamesList.filter((r) => r._win).length;
  const trend = patches.concat(allPatches.filter((p) => !patches.includes(p))).sort(patchCmp)
    .map((p) => { const gs = gamesList.filter((r) => r.patch === p); return gs.length ? { patch: p, games: gs.length, wins: gs.filter((r) => r._win).length, winRate: +(gs.filter((r) => r._win).length / gs.length).toFixed(3) } : null; }).filter(Boolean);
  return { games: gamesList.length, wins, winRate: +(wins / gamesList.length).toFixed(3), trend,
    proGames: gamesList.map(({ _win, ...r }) => ({ ...r, result: _win ? "Win" : "Loss" })) };
};

// lane: same-role opponents, both directions
function laneEntries(set) {
  const map = new Map();
  for (const g of set) for (const role of ROLES) {
    const a = g.blue.byRole[role], b = g.red.byRole[role];
    if (!a || !b) continue;
    for (const [threat, answer, ansSide] of [[a, b, g.red], [b, a, g.blue]]) {
      const k = `${role}|${threat}|${answer}`;
      if (!map.has(k)) map.set(k, { role, threat: [threat], answer: [answer], list: [] });
      map.get(k).list.push(ref(g, ansSide));
    }
  }
  return [...map.values()].filter((e) => e.list.length >= THRESHOLDS.laneMinGames)
    .map((e) => ({ e, f: finish(e.list) })).filter(({ f }) => f.winRate >= THRESHOLDS.laneMinWinRate);
}
// draft: opposing champ picked later, cross-role; single + bot/support pair threats
function draftEntries(set) {
  const N = set.length, threatGames = new Map(), pickCount = new Map(), after = new Map();
  const bump = (m, k) => m.set(k, (m.get(k) || 0) + 1);
  for (const g of set) {
    for (const sd of [g.blue, g.red]) for (const c of sd.picks) { bump(pickCount, c); }
    for (const [mine, opp] of [[g.blue, g.red], [g.red, g.blue]]) {
      for (const c of mine.picks) bump(threatGames, `${c}`);
      const pair = [mine.byRole.bot, mine.byRole.support].filter(Boolean);
      if (pair.length === 2) bump(threatGames, pair.join("+"));
      for (const tc of mine.picks) {
        const tPos = g.pos.get(tc), tRole = mine.roleOf[tc];
        for (const ac of opp.picks) {
          if (g.pos.get(ac) > tPos && opp.roleOf[ac] !== tRole) {
            const k = `${tc}|${ac}`;
            if (!after.has(k)) after.set(k, { threat: [tc], answer: [ac], role: tRole, list: [] });
            after.get(k).list.push(ref(g, opp));
          }
        }
      }
      if (pair.length === 2) {
        const pairPos = Math.max(g.pos.get(pair[0]), g.pos.get(pair[1]));
        const oppPair = [opp.byRole.bot, opp.byRole.support].filter(Boolean);
        const cands = opp.picks.filter((c) => !["bot", "support"].includes(opp.roleOf[c])).map((c) => [c]);
        if (oppPair.length === 2 && Math.max(g.pos.get(oppPair[0]), g.pos.get(oppPair[1])) > pairPos) cands.push(oppPair);
        for (const ans of cands) {
          if (ans.length === 1 && g.pos.get(ans[0]) <= pairPos) continue;
          const k = `${pair.join("+")}|${ans.join("+")}`;
          if (!after.has(k)) after.set(k, { threat: pair, answer: ans, role: "bot", list: [] });
          after.get(k).list.push(ref(g, opp));
        }
      }
    }
  }
  const liftOf = (e) => {
    const tg = threatGames.get(e.threat.join("+")) || 0;
    const base = e.answer.length === 2
      ? [...pickCount.keys()].length && set.filter((g) => [g.blue, g.red].some((sd) => sd.byRole.bot === e.answer[0] && sd.byRole.support === e.answer[1])).length / (2 * N)
      : (pickCount.get(e.answer[0]) || 0) / (2 * N);
    return base ? (e.list.length / tg) / base : 0;
  };
  return [...after.values()].filter((e) => e.list.length >= THRESHOLDS.draftMinGames)
    .map((e) => ({ e, f: finish(e.list), lift: liftOf(e) })).filter(({ f, lift }) => lift >= THRESHOLDS.draftMinLift && f.winRate >= THRESHOLDS.draftMinWinRate);
}

const winGames = games.filter(inWindow);
const build = (lane, draft, olderMeta) => {
  const roles = { top: [], jungle: [], mid: [], bot: [], support: [], draft: [] };
  for (const { e, f } of lane) roles[e.role].push({ id: `${slug(e.threat[0])}-${slug(e.answer[0])}`, kind: "lane", role: e.role,
    threat: e.threat, answer: e.answer, source: "pro", patch: f.proGames.map((r) => r.patch).sort(patchCmp).at(-1),
    games: f.games, winRate: f.winRate, trend: f.trend, why: "", tips: [], proGames: f.proGames, olderMeta,
    sources: ["Leaguepedia Cargo: PicksAndBansS7+ScoreboardGames+MatchScheduleGame, scope in docs/research/comps-sources.md"] });
  for (const { e, f, lift } of draft) roles.draft.push({ id: `${e.threat.map(slug).join("-")}-${e.answer.map(slug).join("-")}-draft`, kind: "draft", role: e.role,
    threat: e.threat, answer: e.answer, source: "pro", patch: f.proGames.map((r) => r.patch).sort(patchCmp).at(-1),
    games: f.games, winRate: f.winRate, lift: +lift.toFixed(2), trend: f.trend, why: "", tips: [], proGames: f.proGames, olderMeta,
    sources: ["Leaguepedia Cargo: PicksAndBansS7+ScoreboardGames+MatchScheduleGame, scope in docs/research/comps-sources.md"] });
  for (const k of Object.keys(roles)) roles[k].sort((a, b) => b.games - a.games || b.winRate - a.winRate);
  return roles;
};

const roles = build(laneEntries(winGames), draftEntries(winGames), false);
const winKeys = new Set(Object.values(roles).flat().map((e) => e.id));
const older = build(laneEntries(games), draftEntries(games), true);
for (const k of Object.keys(roles)) for (const e of older[k]) if (!winKeys.has(e.id)) roles[k].push(e);
for (const k of Object.keys(roles)) roles[k].sort((a, b) => b.games - a.games || b.winRate - a.winRate);

// self-check: every entry meets thresholds on its own proGames; role pairing sane
for (const [k, list] of Object.entries(roles)) for (const e of list) {
  if (!e.olderMeta && e.kind === "lane" && !(e.games >= THRESHOLDS.laneMinGames && e.winRate >= THRESHOLDS.laneMinWinRate)) throw new Error(`lane threshold violated ${e.id}`);
  if (!e.olderMeta && e.kind === "draft" && !(e.games >= THRESHOLDS.draftMinGames && e.lift >= THRESHOLDS.draftMinLift && e.winRate >= THRESHOLDS.draftMinWinRate)) throw new Error(`draft threshold violated ${e.id}`);
  if (e.proGames.length !== e.games) throw new Error(`proGames count mismatch ${e.id}`);
}
// ===== Pro rune pages: ScoreboardPlayers runes for the answer champion in each pro game =====
async function fetchJsonCached(url, file) {
  const cache = join(RAW, file);
  if (existsSync(cache)) { const data = JSON.parse(readFileSync(cache, "utf8")); console.log(`${file}: cache`); return data; }
  const res = await fetch(url, { headers: { "User-Agent": THRESHOLDS.ua } });
  if (!res.ok) throw new Error(`fetch ${file}: HTTP ${res.status}`);
  const data = await res.json();
  writeFileSync(cache, JSON.stringify(data));
  console.log(`${file}: fetched`);
  return data;
}

async function cargoScoreboardPlayers(gameIds) {
  const cache = join(RAW, "scoreboardplayers.json");
  if (existsSync(cache)) { const rows = JSON.parse(readFileSync(cache, "utf8")); console.log(`scoreboardplayers.json: cache ${rows.length} rows`); return rows; }
  const rows = [];
  const CHUNK = 40; // 40 games x 10 players stays under the 500-row Cargo page
  for (let i = 0; i < gameIds.length; i += CHUNK) {
    const chunk = gameIds.slice(i, i + CHUNK);
    const where = `GameId IN (${chunk.map((id) => `"${id}"`).join(",")})`;
    for (let offset = 0; ; offset += 500) {
      const url = "https://lol.fandom.com/api.php?" + new URLSearchParams({
        action: "cargoquery", format: "json", tables: "ScoreboardPlayers",
        fields: "Champion,Role,Runes,SummonerSpells,GameId", where, limit: "500", offset: String(offset),
      });
      let json;
      for (let attempt = 0; attempt < 2; attempt++) {
        if (rows.length || offset || attempt || i) await sleep(attempt ? THRESHOLDS.rateLimitWaitMs : THRESHOLDS.pauseMs);
        const res = await fetch(url, { headers: { "User-Agent": THRESHOLDS.ua } });
        json = await res.json();
        if (!json.error || !String(json.error.info || "").includes("ratelimited")) break;
        console.log(`ratelimited on ScoreboardPlayers chunk ${i / CHUNK} offset ${offset}, attempt ${attempt + 1}`);
        if (attempt === 1) throw new Error(`STOP: ratelimited twice on ScoreboardPlayers chunk ${i / CHUNK} offset ${offset}: ${json.error.info}`);
      }
      if (json.error) throw new Error(`Cargo error ScoreboardPlayers: ${JSON.stringify(json.error)}`);
      const page = (json.cargoquery || []).map((r) => r.title);
      rows.push(...page);
      console.log(`scoreboardplayers: chunk ${i / CHUNK} offset ${offset} +${page.length} (total ${rows.length})`);
      if (page.length < 500) break;
    }
  }
  writeFileSync(cache, JSON.stringify(rows, null, 1));
  return rows;
}

const CDRAGON = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1";
const answerEntries = Object.values(roles).flat();
const runeGameIds = [...new Set(answerEntries.flatMap((e) => e.proGames.map((g) => g.gameId)))];
const [spRows, cdPerks, cdStyles, cdSpells] = [
  await cargoScoreboardPlayers(runeGameIds),
  await fetchJsonCached(`${CDRAGON}/perks.json`, "cdragon-perks.json"),
  await fetchJsonCached(`${CDRAGON}/perkstyles.json`, "cdragon-perkstyles.json"),
  await fetchJsonCached(`${CDRAGON}/summoner-spells.json`, "cdragon-summoner-spells.json"),
];
const rowsByGame = new Map();
for (const r of spRows) { if (!rowsByGame.has(r.GameId)) rowsByGame.set(r.GameId, []); rowsByGame.get(r.GameId).push(r); }
const nameMaps = buildNameMaps(cdPerks, cdStyles, cdSpells);
const coverage = { total: 0, withRunes: 0, missing: 0, dropped: 0 };
const unmappedNames = new Set();
let answersWithPage = 0;
for (const e of answerEntries) {
  const { pages, coverage: cov } = runePagesForAnswer(e, rowsByGame, nameMaps);
  coverage.total += cov.total; coverage.withRunes += cov.withRunes; coverage.missing += cov.missing; coverage.dropped += cov.dropped;
  cov.unmapped.forEach((n) => unmappedNames.add(n));
  if (pages.length) {
    e.runePages = pages;
    answersWithPage++;
    e.sources = [...e.sources, "Leaguepedia Cargo: ScoreboardPlayers (pro rune pages); CommunityDragon perks/perkstyles/summoner-spells (name->id)"];
  }
}
console.log(`rune coverage: ${coverage.withRunes} with runes, ${coverage.missing} missing, ${coverage.dropped} dropped (unmapped) of ${coverage.total} answer pro games; answers with a page: ${answersWithPage}/${answerEntries.length}`);
if (unmappedNames.size) console.log(`unmapped rune/spell names: ${[...unmappedNames].join(" | ")}`);

const out = { generatedAt: new Date().toISOString(), patches, thresholds: THRESHOLDS, totalGames: games.length, windowGames: winGames.length, roles };
writeFileSync(join(DIR, "answers-numbers.json"), JSON.stringify(out, null, 1));
console.log(Object.fromEntries(Object.entries(roles).map(([k, v]) => [k, v.length])), `windowGames=${winGames.length}`);
