// Dev-only preview of the Fullsized App (Dashboard, Tier list, Matchups, Live
// Game) from fixtures with a mocked Tauri IPC: `npm run dev`, then open
// /src/full-app/full-app-preview.html (?live=1 starts a fixture game; ?no-active
// signs League Client in without a Swapper-tracked account). Art comes
// from Data Dragon; names and Riot IDs are invented. Never part of the app build.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";

const DD = "https://ddragon.leagueoflegends.com/cdn";
const VERSION = "16.21.1";
const runeIcons = new Map<number, string>();
const runeReady = fetch(`${DD}/${VERSION}/data/en_US/runesReforged.json`).then(r => r.json()).then((trees: { id: number; icon: string; slots: { runes: { id: number; icon: string }[] }[] }[]) => {
  for (const tree of trees) {
    runeIcons.set(tree.id, `${DD}/img/${tree.icon}`);
    for (const slot of tree.slots) for (const rune of slot.runes) runeIcons.set(rune.id, `${DD}/img/${rune.icon}`);
  }
}).catch(() => {});
const CHAMPIONS: Record<number, [string, string]> = {
  266: ["Aatrox", "Aatrox"], 122: ["Darius", "Darius"], 103: ["Ahri", "Ahri"], 64: ["Lee Sin", "LeeSin"], 222: ["Jinx", "Jinx"],
  157: ["Yasuo", "Yasuo"], 777: ["Yone", "Yone"], 875: ["Sett", "Sett"], 902: ["Milio", "Milio"], 67: ["Vayne", "Vayne"],
  17: ["Teemo", "Teemo"], 126: ["Jayce", "Jayce"], 54: ["Malphite", "Malphite"], 86: ["Garen", "Garen"], 24: ["Jax", "Jax"],
  58: ["Renekton", "Renekton"], 164: ["Camille", "Camille"], 82: ["Mordekaiser", "Mordekaiser"], 516: ["Ornn", "Ornn"], 98: ["Shen", "Shen"],
  412: ["Thresh", "Thresh"], 81: ["Ezreal", "Ezreal"], 238: ["Zed", "Zed"], 11: ["Master Yi", "MasterYi"], 89: ["Leona", "Leona"],
};
const key = (id: number) => CHAMPIONS[id]?.[1] ?? "Aatrox";
const names = Object.fromEntries(Object.entries(CHAMPIONS).map(([id, [name]]) => [id, name]));
const now = Date.now();
const live = new URLSearchParams(location.search).has("live");
// --- uishell preview block: shell states (?disconnected, ?stale, ?empty-accounts, ?switching) ---
const shellState = new URLSearchParams(location.search);

const ROLES = ["top", "jungle", "mid", "adc", "support"];
const ALLY = [266, 64, 103, 222, 412], ENEMY = [122, 11, 238, 81, 89];
function game(i: number) {
  const win = [1, 1, 0, 1, 0, 1, 0, 0, 1, 1, 0, 1][i] === 1;
  const championId = [266, 266, 122, 266, 875, 266, 122, 875, 266, 24, 266, 122][i];
  const [kills, deaths, assists] = win ? [7 + i % 4, 2 + i % 3, 6 + i % 5] : [3 + i % 3, 7 + i % 2, 4 + i % 3];
  const cs = win ? 210 + i * 3 : 160 + i * 2;
  const players = [...ALLY, ...ENEMY].map((id, p) => ({
    championId: p === 0 ? championId : id, riotId: p === 0 ? "Swapper Demo#EUW" : `Player ${p}#DEMO`, me: p === 0, ally: p < 5, role: ROLES[p % 5],
    kills: p === 0 ? kills : 2 + p % 4, deaths: p === 0 ? deaths : 3 + p % 3, assists: p === 0 ? assists : 4 + p % 5,
    cs: p === 0 ? cs : p === 5 ? (win ? 175 : 205) : 120 + p * 9, damage: p === 0 ? (win ? 26000 : 17000) : 9000 + p * 1300, gold: 9000, vision: 18 + p,
    items: [6692, 3053, 3071, 3047, 6333, 0, 3340],
  }));
  return { id: 1000 + i, queue: i % 5 === 4 ? "flex" : "solo", championId, createdAt: now - (i + 1) * 5.4e6, duration: 1500 + i * 70, win, kills, deaths, assists,
    metrics: [null, null, cs / ((1500 + i * 70) / 60)], items: [6692, 3053, 3071, 3047, 6333, 0, 3340], spells: [4, 12], keystone: 8010, cs, level: 16,
    role: "top", placement: win ? 1 + i % 3 : 5 + i % 4, lpChange: i % 5 === 4 ? null : win ? 21 : -18, players, opponent: 5 };
}
const params = new URLSearchParams(location.search);
// --- uidash states: ?empty (no games at all), ?no-solo (only Normal games), ?stale (saved snapshot), ?slow (snapshot takes 6 s) ---
const games = params.has("empty") ? [] : Array.from({ length: 12 }, (_, i) => game(i)).map(g => params.has("no-solo") ? { ...g, queue: "normal", lpChange: null } : g);
const lpHistory = [[1, 2, 40, 70, 60], [1, 2, 62, 71, 60], [1, 2, 44, 71, 61], [1, 2, 71, 72, 61], [1, 1, 20, 73, 61], [1, 1, 41, 74, 61], [1, 1, 23, 74, 62], [1, 2, 64, 75, 62]]
  .map(([, , lp, wins, losses], i) => ({ at: now - (8 - i) * 36e5, tier: "EMERALD", division: "II", lp, wins, losses }));
const summary = { metrics: [], roles: ROLES.map((role, i) => ({ role, wins: i === 0 ? 7 : i === 2 ? 1 : 0, games: i === 0 ? 10 : i === 2 ? 2 : 0 })),
  champions: [{ id: 266, wins: 5, losses: 1, kda: 4.12 }, { id: 122, wins: 1, losses: 2, kda: 2.05 }, { id: 875, wins: 0, losses: 2, kda: 1.4 }, { id: 24, wins: 1, losses: 0, kda: 5.5 }] };
const snapshot = {
  championNames: names, summoner: { riotId: "Swapper Demo#EUW", level: 287, icon: `${DD}/${VERSION}/img/profileicon/5367.png` },
  rank: { tier: "EMERALD", division: "II", lp: 64, wins: 88, losses: 71 }, fetchedAt: now - (params.has("stale") ? 72e5 : 120000), games, lpHistory: params.has("empty") || params.has("no-baseline") ? [] : lpHistory, // --- todaylp: ?no-baseline shows "–" for today's LP ---
  summaries: { all: summary, solo: summary, flex: summary, normal: { ...summary, champions: [] } },
};
const account = (id: string, name: string, icon: number) => ({ id, name, riotId: `${name}#DEMO`, nickname: null, platform: "EUW1", region: "EUW", profileIconId: icon, iconDataUrl: `${DD}/${VERSION}/img/profileicon/${icon}.png` });
const state = {
  accounts: [account("a", "Swapper Demo", 5367), account("b", "Alt Account", 4658), account("c", "Smurf Two", 29)], activeId: new URLSearchParams(location.search).has("no-active") ? null : "a", isSwitching: false,
  useDeceive: false, riotExe: null, riotDetected: false, deceiveDetected: false, autoApplyTopPreset: false, runeTier: "emerald_plus",
  applySpellsWithRunes: true, importItemsWithRunes: true, hotkey: null, hotkeyActive: false, notificationsEnabled: true, readyCheckNotifications: true,
  koreanFont: false, ltkInstalled: false,
  remote: { enabled: false, state: "disabled", address: null, tailscaleAddress: null, lanAddress: null, lanMessage: null, lanFirewallNeeded: false, message: null,
    tailscaleInstalled: false, tailscaleRunning: false, dnsName: null, localAddress: null, leagueRunning: false, lcuConnected: false,
    lanReconnectNeeded: false, lanReconnectOldAddress: null, lanReconnectNewAddress: null, lanReconnectSince: null },
};
const cards = {
  b: { rank: { tier: "PLATINUM", division: "I", lp: 12, wins: 40, losses: 38 }, wins: 6, losses: 4, lastPlayed: now - 864e5 * 2, updatedAt: now },
  c: { rank: null, wins: 2, losses: 5, lastPlayed: now - 864e5 * 9, updatedAt: now },
};
const tiers: [number, string, number][] = [[266, "S+", 52.6], [24, "S", 51.4], [875, "S+", 52.2], [86, "S", 51.1], [58, "A", 50.6], [164, "S", 50.9], [82, "A", 51.0], [516, "A", 50.4], [54, "A", 51.3], [122, "B", 49.8], [98, "B", 49.6], [126, "C", 48.7], [17, "C", 48.9], [67, "D", 47.8]];

const form = (games: number, wins: number) => ({ games, wins, kills: games * 6, deaths: games * 4, assists: games * 7 });
const rank = (tier: string, division: string) => ({ tier, division, lp: 40, wins: 60, losses: 52 });
const RANKS = [["EMERALD", "II"], ["DIAMOND", "IV"], ["EMERALD", "I"], ["PLATINUM", "I"], ["EMERALD", "III"], ["EMERALD", "IV"], ["DIAMOND", "III"], ["EMERALD", "II"], ["PLATINUM", "II"], ["EMERALD", "I"]];
const BADGES = [[], [{ kind: "streak-win", label: "4 win streak", detail: "Won the last 4 ranked games" }], [{ kind: "one-trick", label: "One-trick", detail: "80% of games on this champion" }], [],
  [{ kind: "off-role", label: "Off-role", detail: "Usually plays Mid" }], [{ kind: "streak-loss", label: "3 loss streak", detail: "Lost the last 3 ranked games" }], [], [{ kind: "new-champ", label: "New champ", detail: "First game on this champion" }], [], []];
const liveView = {
  gameId: 1, queueId: 420, queue: "Ranked Solo/Duo", mapId: 11, gameTime: 754,
  blue: { averageTier: "EMERALD", averageDivision: "II", ranked: 5, physical: 60 }, red: { averageTier: "EMERALD", averageDivision: "I", ranked: 5, physical: 40 },
  players: [...ALLY, ...ENEMY].map((championId, p) => ({
    key: `p${p}`, side: p < 5 ? "blue" : "red", championId, championName: CHAMPIONS[championId][0], riotId: p === 0 ? "Swapper Demo#EUW" : `Player ${p}#DEMO`,
    isSelf: p === 0, isBot: false, position: ROLES[p % 5], spells: [4, p % 5 === 1 ? 11 : 12], keystone: 8010, primaryStyle: 8000, secondaryStyle: 8400,
    summonerLevel: 120 + p * 17, profileIcon: 29, solo: rank(RANKS[p][0], RANKS[p][1]), flex: p % 3 ? null : rank("GOLD", "I"),
    history: { form: form(20, 9 + p % 5), onChampion: form(4 + p, 2 + p % 4), mainRole: [ROLES[p % 5], 70], streak: 0, recent: [] }, badges: BADGES[p],
    damage: "", status: "ready", live: { level: 9 + p % 3, kills: p % 4, deaths: p % 3, assists: p % 5, cs: 80 + p * 6, wardScore: 8, items: [1055, 3047, 6692, 0, 0, 0, 3340], dead: p === 7 },
  })),
};

// ---- uibuilds: champion page, matchups and build workspace mocks (preview only) ----
const SHARD_ROWS = [[5008, 5005, 5007], [5008, 5010, 5001], [5011, 5013, 5001]];
const shardIcon: Record<number, string> = { 5008: "AdaptiveForce", 5005: "AttackSpeed", 5007: "CDRScaling", 5010: "MovementSpeed", 5001: "HealthPlus", 5011: "HealthScaling", 5013: "Tenacity" };
for (const [id, name] of Object.entries(shardIcon)) runeIcons.set(Number(id), `${DD}/img/perk-images/StatMods/StatMods${name}Icon.png`);
let runeCatalog: unknown = null;
const catalogReady = fetch(`${DD}/${VERSION}/data/en_US/runesReforged.json`).then(r => r.json()).then((trees: { id: number; name: string; slots: { runes: { id: number; name: string }[] }[] }[]) => {
  const rune = (r: { id: number; name: string }) => ({ id: r.id, name: r.name, winPct: null, pickPct: null, play: 0 });
  runeCatalog = {
    trees: trees.map(tree => ({ id: tree.id, name: tree.name, keystones: tree.slots[0].runes.map(rune), rows: tree.slots.slice(1).map((slot, i) => ({ kind: "row", label: `Row ${i + 1}`, runes: slot.runes.map(rune) })) })),
    shards: SHARD_ROWS.map((row, i) => ({ kind: "shard", label: `Shard ${i + 1}`, runes: row.map(id => ({ id, name: `Shard ${id}`, winPct: null, pickPct: null, play: 0 })) })),
  };
}).catch(() => {});
const ITEM_NAMES: Record<number, string> = { 1055: "Doran's Blade", 2003: "Health Potion", 3047: "Plated Steelcaps", 3111: "Mercury's Treads", 6692: "Eclipse", 3053: "Sterak's Gage", 3071: "Black Cleaver", 6333: "Death's Dance", 3156: "Maw of Malmortius", 3065: "Spirit Visage", 3026: "Guardian Angel", 6610: "Sundered Sky" };
const named = (id: number) => ({ id, name: ITEM_NAMES[id] ?? "Item" });
const SKILLS = "QEWQQRQEQERWEEWRWW";
const buildOf = (ids: number[], winPct: number, games: number, order: string) => ({
  starters: [{ ...named(1055), count: 1 }, { ...named(2003), count: 1 }], items: ids.map(named),
  options: [[6333, 4, 53.1, 4200], [3156, 4, 52.4, 1900], [3065, 5, 54, 2100], [3026, 6, 55.2, 1500]].map(([id, slot, wr, g]) => ({ ...named(id), slot, winPct: wr, games: g })),
  games, coreGames: Math.round(games / 3), coreLen: 3, coreWinPct: winPct, stale: false, updatedAt: null, skillPriority: null, skillOrder: order,
});
const runePage = (primaryPageId: number, keystone: number, primaryRunes: number[], secondaryPageId: number, secondaryRunes: number[]) => ({ primaryPageId, keystone, primaryRunes, secondaryPageId, secondaryRunes, shards: [5005, 5008, 5001] });
// ---- end uibuilds mocks ----
mockIPC(async (cmd, args) => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const a = (args ?? {}) as Record<string, any>;
  switch (cmd) {
    // ---- search: player fixtures (?stale / ?disconnected / ?search-partial / ?search-slow / ?search-error) ----
    case 'full_app_search_players': {
      const connected = !params.has('stale') && !params.has('disconnected');
      if (params.has('search-slow')) await new Promise(resolve => setTimeout(resolve, 1000));
      const q = String(a.query).trim().toLowerCase();
      const saved = snapshot.games.flatMap(g => g.players.filter(p => !p.me && p.riotId.toLowerCase().startsWith(q)).map(p => ({ riotId: p.riotId,
        icon: `${DD}/${VERSION}/img/profileicon/29.png`, level: null, ally: p.ally, lastPlayed: g.createdAt })));
      const players = [...new Map(saved.reverse().map(p => [p.riotId, p])).values()].slice(0, 8).map(p => ({ ...p, live: false }));
      if (connected && q === 'demo player#test') players.unshift({ riotId: 'Demo Player#TEST', icon: `${DD}/${VERSION}/img/profileicon/29.png`, level: null, ally: false, lastPlayed: 0, live: true });
      return { connected, players, message: connected ? q.includes('#') && !players.length ? 'No player found for that Riot ID.' : null : 'Open League Client to load a profile. Showing saved players only.' };
    }
    case 'full_app_player_profile': {
      const { profileGame } = await import('./playerModel');
      if (params.has('search-slow')) await new Promise(resolve => setTimeout(resolve, 1000));
      if (params.has('search-error')) throw new Error('Preview profile unavailable');
      const live = !params.has('stale') && !params.has('disconnected');
      const riotId = String(a.riotId);
      return { player: { riotId, icon: `${DD}/${VERSION}/img/profileicon/29.png`, level: live ? 184 : null, ally: true, lastPlayed: games[0]?.createdAt ?? null },
        solo: live ? rank('EMERALD', 'II') : null, flex: live ? rank('PLATINUM', 'IV') : null,
        games: (riotId === 'Demo Player#TEST' ? games : games.map(g => profileGame(g, riotId)).filter((g): g is NonNullable<typeof g> => g !== null))
          .map(g => live ? { ...g, spells: [4, g.role === 'jungle' ? 11 : 12], keystone: 8010, level: 16, placement: 3,
            opponent: g.players.findIndex(p => !p.ally && p.role === g.role) } : g), live, fetchedAt: snapshot.fetchedAt,
        message: params.has('search-partial') ? 'Some profile data could not be loaded. Try again shortly.' : null };
    }
    // ---- end search fixtures ----
    case "get_state": return shellState.has("empty-accounts") ? { ...state, accounts: [], activeId: null } : state;
    // uishell preview states: ?disconnected (no client), ?stale (saved stats), ?empty-accounts handled in get_state above.
    case "dashboard_snapshot":
      if (params.has("slow")) await new Promise(resolve => setTimeout(resolve, 6000));
      if (shellState.has("loading")) await new Promise(resolve => setTimeout(resolve, 600000));
      if (shellState.has("disconnected")) return { snapshot: null, cards, stale: false, message: null };
      return { snapshot, cards, stale: (shellState.has("stale") || params.has("stale")), message: (shellState.has("stale") || params.has("stale")) ? "League Client is not running." : null };
    case "full_app_match_detail": {
      if (new URLSearchParams(location.search).has("detail-error")) throw new Error("Preview: match details unavailable");
      const original = games.find(g => g.id === a.gameId) ?? games[0];
      const { profileGame } = await import('./playerModel');
      const match = a.riotId ? profileGame(original, String(a.riotId)) ?? original : original;
      const detail = { game: match, teams: [100, 200].map((teamId, i) => ({ teamId, win: i === 0 ? original.win : !original.win,
        objectives: [{ label: "Dragons", kills: i ? 1 : 3 }, { label: "Heralds", kills: i ? 0 : 1 }, { label: "Grubs", kills: i ? 2 : 4 }, { label: "Barons", kills: i ? 0 : 1 }, { label: "Towers", kills: i ? 3 : 9 }] })),
        players: match.players.map((p, i) => ({ ...p, riotId: p.me ? "You#•••" : `Player ${i}#•••`, participantId: i + 1, teamId: i < 5 ? 100 : 200,
          level: 18 - i % 4, spells: [4, p.role === "jungle" ? 11 : p.role === "support" ? 14 : 12],
          perks: [8010, 9111, 9104, 8299, 8444, 8242], keystone: 8010, secondaryStyle: 8400,
          placement: [2, 3, 1, 4, 5, 6, 7, 8, 9, 10][i], score: [86, 82, 96, 74, 67, 60, 53, 46, 39, 32][i],
          rank: i === 9 ? null : rank(RANKS[i][0], RANKS[i][1]), rankLoaded: i !== 9,
          damageTaken: 21000 + i * 1400, wardsPlaced: 9 + i, wardsKilled: i + 1 })) };
      if (new URLSearchParams(location.search).has("slow-ranks")) {
        await emit("match_detail", { ...detail, players: detail.players.map(player => ({ ...player, rank: null, rankLoaded: false })) });
        await new Promise(resolve => setTimeout(resolve, 1500));
      }
      return detail;
    }
    case "rank_icon": return `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-mini-crests/${a.tier}.svg`;
    case "rune_icon": await runeReady; if (runeIcons.has(a.id)) return runeIcons.get(a.id); throw new Error("Preview rune icon unavailable");
    case "dashboard_benchmarks": return Object.fromEntries((a.picks as [number, string][]).map(([id, role]) => [`${id}|${role}`, { kills: 5, deaths: 5, assists: 6, damage: 21000, gold: 11000, cs: 185, games: 9000 }]));
    case "get_champion_list": return Object.entries(CHAMPIONS).map(([id, [name]]) => ({ id: Number(id), name }));
    case "champion_icon": return `${DD}/${VERSION}/img/champion/${key(a.id)}.png`;
    // The app's tiles are the client's 380 px splash tiles; Data Dragon's are the same crops.
    case "champion_art": return `${DD}/img/champion/${a.kind === "splash" ? "splash" : a.kind === "loading" ? "loading" : "tiles"}/${key(a.id)}_0.jpg`;
    case "item_icon": return `${DD}/${VERSION}/img/item/${a.id}.png`;
    case "spell_icon": return `${DD}/${VERSION}/img/spell/${({ 4: "SummonerFlash", 12: "SummonerTeleport", 11: "SummonerSmite", 14: "SummonerDot" } as Record<number, string>)[a.id] ?? "SummonerFlash"}.png`;
    case "get_tier_list": return { unavailable: false, stale: false, updatedAt: null, rows: tiers.map(([id, tier, winPct], i) => ({ championId: id, name: CHAMPIONS[id][0], tier, winPct, pickPct: 9 - i * .5, banPct: 4 + i % 3, games: 24000 - i * 1200 })) };
    case "full_app_matchups": return { championId: a.championId, championName: CHAMPIONS[a.championId]?.[0] ?? "Aatrox", position: "top", bestInto: [], unavailable: false, stale: false, updatedAt: null,
      beats: tiers.filter(([id]) => id !== a.championId).map(([id], i) => ({ championId: id, name: CHAMPIONS[id][0], winPct: 54 - i * .8, delta: 3.6 - i * .8, games: 3000 - i * 150, lowSample: i > 11 })) };
    // --- uishell preview block: Settings pages ---
    case "window_settings": return { fullApp: true, lowMemory: false };
    case "live_game_auto_open": return true;
    case "plugin:autostart|is_enabled": return true;
    case "get_overlay_settings": return { enabled: true, gold: true, objectives: true, skills: true, goldHotkey: 9 };
    case "set_overlay_settings": return a.settings;
    case "list_paired_lan_devices": return [];
    case "probe_remote": return state.remote;
    case "begin_repair": return null;
    case "doctor_report": return null;
    // ---- uibuilds cases ----
    case "get_champion_overview": return { championId: a.championId, position: a.position, unavailable: false, damage: null,
      stats: { tier: "S+", rank: 1, rankTotal: 58, winPct: 52.6, pickPct: 9, banPct: 4, games: 24000, patch: "16.21" },
      build: { runes: runePage(8000, 8010, [9111, 9104, 8299], 8300, [8304, 8347]), spells: [4, 12], items: buildOf([6692, 3053, 3071, 6333, 3065, 3026], 52.8, 18200, SKILLS), skillOrder: SKILLS } };
    case "get_keystone_build": return { ...buildOf([6692, 3053, 3071, 6333, 3065, 3026], 52.8, 18200, SKILLS), highestWin: { preset: { index: 1, title: "Grasp", ...runePage(8400, 8437, [8446, 8429, 8451], 8000, [9111, 9104]), winPct: 54.1, play: 6100, spells: [4, 14] }, build: buildOf([6610, 3053, 3071, 3065, 3026, 6333], 54.3, 6100, "QWEQQRQEQERWEEWRWW") }, highestWinMatchesPick: false };
    case "get_matchup": return { enemyChampionId: a.enemyChampionId, enemyName: CHAMPIONS[a.enemyChampionId]?.[0] ?? "Darius", fallback: false, stats: { winPct: 47.6, avgWinPct: 50.4, delta: -2.8, games: 3150, patch: "16.21" }, preset: { index: 0, title: "Conqueror", ...runePage(8000, 8010, [9111, 9105, 8299], 8400, [8444, 8242]), winPct: 47.6, play: 3150, spells: [4, 12] }, build: buildOf([6692, 3111, 3053, 3071, 3065, 3026], 48.9, 3150, "QEQWQRQEQERWEEWRWW") };
    case "full_app_rune_catalog": await catalogReady; if (runeCatalog) return runeCatalog; throw new Error("Rune catalog unavailable");
    case "champion_abilities": return { championId: a.championId, summoners: [], passive: { name: "Deathbringer Stance", iconPath: null, description: "Periodically, Aatrox's next basic attack deals bonus physical damage and heals him." },
      spells: ["Q", "W", "E", "R"].map((key, i) => ({ key, name: ["The Darkin Blade", "Infernal Chains", "Umbral Dash", "World Ender"][i], iconPath: null,
        description: ["Aatrox slams his greatsword down three times; each strike is stronger at the edge.", "Aatrox smashes the ground, damaging and slowing the first enemy hit, then pulls them back.", "Passive: Aatrox heals from champion damage. Active: Aatrox dashes, empowering his next attacks.", "Aatrox unleashes his demonic form, fearing nearby minions and gaining Attack Damage and healing."][i],
        cooldowns: key === "R" ? [120, 100, 80] : [14, 12, 10, 8, 6], costs: [], ranges: key === "R" ? [] : [625, 625, 625, 625, 625], charges: key === "E" ? 2 : 0, recharge: key === "E" ? [9, 8, 7, 6, 5] : [] })) };
    case "full_app_abilities": return ["Q", "W", "E", "R"].map(k => ({ key: k, name: `Ability ${k}`, icon: `${DD}/${VERSION}/img/spell/Aatrox${k}.png` }));
    case "get_runes": return { phase: "None", championId: 0, championName: "", position: "top", presets: [], trees: [], shards: [], applied: null, autoApply: false, importItems: true, canApply: false, locked: false, spells: { spell1Id: 4, spell2Id: 12, available: [], applyWithRunes: true }, enemies: [], enemySlots: 5 };
    case "get_pro_builds": return { championId: 266, position: "top", role: "top", page: 1, hasMore: false, unavailable: false, message: null, stale: false, updatedAt: null, matches: [1, 2, 3].map(i => ({ matchId: i, proName: `Pro player ${String.fromCharCode(64 + i)}`, team: `Team ${i}`, league: "LCK", region: "KR", role: "top", roleLabel: "Top", win: i % 2 === 1, playedAgo: `${i * 3}h ago`, patch: "16.21", ...runePage(8400, 8437, [8446, 8429, 8451], 8000, [9111, 9104]), kills: 4 + i, deaths: 2, assists: 6, spells: [4, 12], finalItems: [6610, 3053, 3071, 3047, 6333, 3026].map(named), completedItems: [6610, 3053, 3071, 6333].map(named), skillOrder: SKILLS.split(""), itemPath: [], otp: false })) };
    case "role_icon": return `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/svg/position-${a.role}.svg`;
    // ---- end uibuilds cases ----
    case "live_game_active": return live;
    case "live_game_view": case "live_game_live": return live ? liveView : null;
    default: throw new Error(`Not mocked: ${cmd}`);
  }
}, { shouldMockEvents: true });
mockWindows("full-app");
(globalThis as { isTauri?: boolean }).isTauri = true;

void import("./main").then(() => {
  // ?switching: the switch flow only reports started/done/failed, so that is all the preview fakes.
  if (shellState.has("switching")) setTimeout(() => void emit("switch_started", { id: "b", name: "Alt Account" }), 1200);
});
