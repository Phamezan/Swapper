// Dev-only preview of the Fullsized App (Dashboard, Tier list, Matchups, Live
// Game) from fixtures with a mocked Tauri IPC: `npm run dev`, then open
// /src/full-app/full-app-preview.html (?live=1 starts a fixture game; ?no-active
// signs League Client in without a Swapper-tracked account). Art comes
// from Data Dragon; names and Riot IDs are invented. Never part of the app build.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
// ---- matchdetail: rich match fixture (preview only) ----
import { matchDetailFixture } from "./match/preview";
import type { PlayerHit } from './playerModel';
// ---- end matchdetail import ----

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
  268: ["Azir", "Azir"], 51: ["Caitlyn", "Caitlyn"], 7: ["LeBlanc", "Leblanc"],
  58: ["Renekton", "Renekton"], 164: ["Camille", "Camille"], 82: ["Mordekaiser", "Mordekaiser"], 516: ["Ornn", "Ornn"], 98: ["Shen", "Shen"],
  412: ["Thresh", "Thresh"], 81: ["Ezreal", "Ezreal"], 238: ["Zed", "Zed"], 11: ["Master Yi", "MasterYi"], 89: ["Leona", "Leona"],
};
const DUO_CHAMPIONS: Record<number, string> = { 115: "Ziggs", 30: "Karthus", 50: "Swain", 45: "Veigar", 910: "Hwei", 147: "Seraphine", 63: "Brand", 901: "Smolder", 117: "Lulu" };
const key = (id: number) => CHAMPIONS[id]?.[1] ?? DUO_CHAMPIONS[id] ?? "Aatrox";
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
    // ---- profile round9: varied, invented champion history for the searched teammate ----
    championId: p === 0 ? championId : p === 1 ? [64, 64, 24, 64, 11, 64, 24, 11, 64, 24, 64, 11][i] : id, riotId: p === 0 ? "Swapper Demo#EUW" : `Player ${p}#DEMO`, me: p === 0, ally: p < 5, role: ROLES[p % 5],
    // ---- profile2: observed participant icons (invented identities) ----
    profileIcon: [5367, 29, 4658, 7, 23, 29, 7, 23, 4658, 5367][p],
    // ---- end profile2 icons ----
    // ---- end profile champion history ----
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
// ---- locbar: deliberately oversized invented ID to exercise middle-segment truncation ----
if (params.has('locbar-long')) for (const game of games) for (const player of game.players) {
  if (player.riotId === 'Player 1#DEMO') player.riotId = 'A very long invented player name for checking the location bar#DEMO';
}
// ---- end locbar long trail fixture ----
const lpHistory = [[1, 2, 40, 70, 60], [1, 2, 62, 71, 60], [1, 2, 44, 71, 61], [1, 2, 71, 72, 61], [1, 1, 20, 73, 61], [1, 1, 41, 74, 61], [1, 1, 23, 74, 62], [1, 2, 64, 75, 62]]
  .map(([, , lp, wins, losses], i) => ({ at: now - (8 - i) * 36e5, tier: "EMERALD", division: "II", lp, wins, losses }));
// ?lp=month: recorded climbs and dips through Platinum, Emerald and Diamond.
if (params.get("lp") === "month") lpHistory.splice(0, lpHistory.length, ...Array.from({ length: 60 }, (_, i) => {
  const score = Math.round(1870 + i * 10 + 38 * Math.sin(i / 2.2)), tier = score >= 2400 ? "DIAMOND" : score >= 2000 ? "EMERALD" : "PLATINUM";
  return { at: now - (59 - i) * 12 * 36e5, tier, division: ["IV", "III", "II", "I"][Math.floor(score % 400 / 100)], lp: score % 100, wins: 40 + Math.floor(i * .7), losses: 30 + i - Math.floor(i * .7) };
}));
const summary = { metrics: [], roles: ROLES.map((role, i) => ({ role, wins: i === 0 ? 7 : i === 2 ? 1 : 0, games: i === 0 ? 10 : i === 2 ? 2 : 0 })),
  champions: [{ id: 266, wins: 5, losses: 1, kda: 4.12 }, { id: 122, wins: 1, losses: 2, kda: 2.05 }, { id: 875, wins: 0, losses: 2, kda: 1.4 }, { id: 24, wins: 1, losses: 0, kda: 5.5 }] };
const snapshot = {
  championNames: names, summoner: { riotId: "Swapper Demo#EUW", level: 287, icon: `${DD}/${VERSION}/img/profileicon/5367.png` },
  rank: { tier: "EMERALD", division: "II", lp: 64, wins: 88, losses: 71 }, fetchedAt: now - (params.has("stale") ? 72e5 : 120000), mmr: params.has("empty") ? null : { tier: "DIAMOND", division: "IV", lp: 42, games: 10, trend: 31 }, games, lpHistory: params.has("empty") || params.has("no-baseline") ? [] : lpHistory, // --- todaylp: ?no-baseline shows "–" for today's LP ---
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
const searchCalls: { query: string; online: boolean; done: boolean }[] = [];
const liveChecks: boolean[] = [];
Object.assign(window, { searchCalls, liveChecks });
mockIPC(async (cmd, args) => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const a = (args ?? {}) as Record<string, any>;
  // ---- glass: exercise contextual draft art inside the real shell (?glass-draft) ----
  if (shellState.has('glass-draft') && (cmd === 'get_runes' || cmd === 'full_app_draft')) {
    const { fixtureView, fixtureDraft } = await import('./champ-select-fixtures');
    return cmd === 'get_runes' ? fixtureView('locked') : fixtureDraft('locked');
  }
  // ---- end glass draft fixture ----
  switch (cmd) {
    // ---- locbar: keep the existing version label visible in browser evidence ----
    case 'plugin:app|version': return '1.0.7';
    // ---- end locbar version fixture ----
    // ---- search: player fixtures (?stale / ?disconnected / ?search-partial / ?search-slow / ?search-error) ----
    case 'full_app_search_players': {
      const connected = !params.has('stale') && !params.has('disconnected');
      const call = { query: String(a.query), online: Boolean(a.online), done: false };
      searchCalls.push(call);
      if (a.online && params.has('search-slow')) await new Promise(resolve => setTimeout(resolve, 1000));
      const q = String(a.query).trim().toLowerCase();
      if (a.online && params.has('search-reorder') && q === 'exam') await new Promise(resolve => setTimeout(resolve, 800));
      const saved = snapshot.games.flatMap(g => g.players.filter(p => !p.me && p.riotId.toLowerCase().startsWith(q)).map(p => ({ riotId: p.riotId,
        icon: `${DD}/${VERSION}/img/profileicon/29.png`, level: null, ally: p.ally, lastPlayed: g.createdAt })));
      const players: (PlayerHit & { live: boolean })[] = [...new Map(saved.reverse().map(p => [p.riotId, p])).values()].slice(0, 8).map(p => ({ ...p, live: false }));
      if (connected && a.online && q === 'demo player#test') players.unshift({ riotId: 'Demo Player#TEST', icon: `${DD}/${VERSION}/img/profileicon/29.png`, level: null, ally: false, lastPlayed: 0, live: false });
      if (connected && a.online && !params.has('search-online-fail') && 'example'.startsWith(q) && q.length >= 2) players.unshift({ riotId: 'Example#TAG', icon: null, level: null, ally: null, lastPlayed: null, live: false });
      if (params.has('profile-live')) for (const player of players) player.live = player.riotId === 'Player 1#DEMO';
      call.done = true;
      return { connected, players, message: connected ? q.includes('#') && !players.length ? 'No player found for that Riot ID.' : null : 'Open League Client to load a profile. Showing saved players only.' };
    }
    case 'full_app_player_live': {
      liveChecks.push(Boolean(a.details));
      const active = params.has('profile-live') && !params.has('stale') && !params.has('disconnected');
      const view = active && a.details && !params.has('profile-live-fail') ? { ...liveView, gameTime: null,
        players: liveView.players.map(p => ({ ...p, isSelf: false, live: null })) } : null;
      return { live: active, view, message: a.details && !view ? 'Live players are unavailable. Try again shortly.' : null };
    }
    case 'full_app_player_profile': {
      const { profileGame } = await import('./playerModel');
      if (params.has('search-slow')) await new Promise(resolve => setTimeout(resolve, 1000));
      if (params.has('search-error')) throw new Error('Preview profile unavailable');
      const live = !params.has('stale') && !params.has('disconnected');
      const riotId = String(a.riotId);
      return { region: live ? 'EUW' : null, player: { riotId, icon: `${DD}/${VERSION}/img/profileicon/29.png`, level: live ? 184 : null, ally: true, lastPlayed: games[0]?.createdAt ?? null },
        // ---- profile2: honest rank records and cached participant portraits ----
        solo: live ? { ...rank('EMERALD', 'II'), recordAvailable: !params.has('profile-no-record'), ...(params.has('profile-ugg-record') && { recordSource: 'ugg' }) } : null, flex: null,
        profileIcons: Object.fromEntries(games.flatMap(g => g.players.map(p => [p.riotId.toLowerCase(), `${DD}/${VERSION}/img/profileicon/${p.profileIcon}.png`]))),
        // ---- end profile2 rank and portraits ----
        games: (riotId === 'Demo Player#TEST' ? games : games.map(g => profileGame(g, riotId)).filter((g): g is NonNullable<typeof g> => g !== null))
          // ---- profile round9: metrics from the fixture's observed participant totals ----
          .map(g => {
            const me = g.players.find(p => p.me), team = g.players.filter(p => p.ally), minutes = g.duration / 60;
            const kills = team.reduce((sum, p) => sum + p.kills, 0), damage = team.reduce((sum, p) => sum + p.damage, 0);
            return live && me ? { ...g, spells: [4, g.role === 'jungle' ? 11 : 12], keystone: 8010, level: 16, placement: 3,
              metrics: [(g.kills + g.assists) / Math.max(1, g.deaths), kills ? Math.min(100, 100 * (g.kills + g.assists) / kills) : null,
                g.cs / minutes, me.damage / minutes, damage ? 100 * me.damage / damage : null, me.vision / minutes],
              opponent: g.players.findIndex(p => !p.ally && p.role === g.role) } : g;
          })
          // ---- profile2: missing estimated roles retain the shared empty state ----
          .map(g => params.has('profile-no-roles') ? { ...g, role: '' } : g), live, fetchedAt: snapshot.fetchedAt,
          // ---- end profile2 role fixture ----
          // ---- end profile metrics ----
        message: params.has('search-partial') ? 'Some profile data could not be loaded. Try again shortly.' : null };
    }
    // ---- end search fixtures ----
    case "get_state": return shellState.has("empty-accounts") ? { ...state, accounts: [], activeId: null } : state;
    // uishell preview states: ?disconnected (no client), ?stale (saved stats), ?empty-accounts handled in get_state above.
    case "dashboard_snapshot":
      if (params.has("slow")) await new Promise(resolve => setTimeout(resolve, 6000));
      if (shellState.has("loading")) await new Promise(resolve => setTimeout(resolve, 600000));
      if (shellState.has("disconnected")) return { snapshot: null, cards, stale: false, message: null };
      if (params.has("matchup-roles")) return { snapshot: { ...snapshot, games: [{ ...games[0], championId: 7, role: "top" }] }, cards, stale: false, message: null };
      return { snapshot, cards, stale: (shellState.has("stale") || params.has("stale")), message: (shellState.has("stale") || params.has("stale")) ? "League Client is not running." : null };
    // ---- matchdetail: stats and timeline states ----
    case "full_app_match_detail": {
      if (new URLSearchParams(location.search).has("detail-error")) throw new Error("Preview: match details unavailable");
      const original = games.find(g => g.id === a.gameId) ?? games[0];
      const { profileGame } = await import('./playerModel');
      let match = a.riotId ? profileGame(original, String(a.riotId)) ?? original : original;
      if (params.has("detail-flex")) match = {...match, queue: "flex"};
      const detail = matchDetailFixture(match, RANKS.map(([tier, division]) => rank(tier, division)), params);
      if (new URLSearchParams(location.search).has("slow-ranks")) {
        await emit("match_detail", { ...detail, players: detail.players.map(player => ({ ...player,
          riotId: player.me ? match.players.find(p => p.me)?.riotId ?? player.riotId : player.riotId,
          rank: null, rankLoaded: false })) });
        await new Promise(resolve => setTimeout(resolve, 1500));
      }
      return detail;
    }
    case "objective_icon": return a.name === "grub"
      ? "https://raw.communitydragon.org/latest/game/assets/ux/minimap/icons/grub.png"
      : `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-match-history/global/default/${a.name}.png`;
    // ---- end matchdetail cases ----
    // ---- glass: normalize the preview bracket just like the backend (no *_plus asset exists) ----
    case "rank_icon": return `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-mini-crests/${String(a.tier).replace(/_plus$/, '').replace(/^all$/, 'unranked')}.svg`;
    // ---- end glass rank fixture ----
    case "rune_icon": await runeReady; if (runeIcons.has(a.id)) return runeIcons.get(a.id); throw new Error("Preview rune icon unavailable");
    case "dashboard_benchmarks": return Object.fromEntries((a.picks as [number, string][]).map(([id, role]) => [`${id}|${role}`, { kills: 5, deaths: 5, assists: 6, damage: 21000, gold: 11000, cs: 185, games: 9000 }]));
    case "get_current_patch": return "16.21";
    // Patch key art: the browser preview shows champion splashes in its place.
    case "newest_patch_notes": return "26.21";
    case "patch_banner": return `${DD}/img/champion/splash/${a.version === "26.20" ? "Sett" : "Ekko"}_0.jpg`;
    case "get_latest_patch_notes": {
      if (a.version != null && a.version !== "26.21" && a.version !== "26.20") throw new Error("Only current and previous patches are available");
      const notes = {
      patch: "26.21", title: "League of Legends Patch 26.21 Notes",
      availablePatches: [{ version: "26.21", bannerUrl: null }, { version: "26.20", bannerUrl: null }],
      description: "True Damage has arrived with style! Plus tons to look forward to across SR, Mayhem, Classic, and Arena.",
      date: "2026-10-20T18:00:00.000Z", bannerUrl: `${DD}/img/champion/splash/Aatrox_0.jpg`,
      articleUrl: "https://www.leagueoflegends.com/en-us/news/game-updates/league-of-legends-patch-26-21-notes/",
      intro: ["Worlds has begun, but time still passes for the rest of us non-professional mortals, so here we've got 26.21. This is a pretty chunky patch, with changes to some chunks like Rammus, Sion, Nunu & Willump, as well as to a bunch of other champions across roles who've been on top of the game."],
      stale: false, fetchedAt: now,
      sections: [
        { title: "Champions", paragraphs: [], entries: [
          { name: "Azir", context: "We've kept the Emperor of the Sands' mana pool pretty dry, but this is overly restrictive now. We're buffing his Mana Growth so he can more freely use his abilities to contest lane.", notes: [], groups: [{ title: "Base Stats", lines: [{ label: "Mana Growth", before: "40/Level", after: "50/Level", text: null }] }] },
          { name: "Caitlyn", context: "Caitlyn's been at a very high pick and ban rate for months, so it's time to nerf her. We're moving some power from her late-game headshots into her early game Q damage.", notes: [], groups: [
            { title: "Passive", lines: [{ label: "Damage", before: "60 - 100% AD", after: "60 - 80% AD", text: null }] },
            { title: "Q", lines: [{ label: "Base Damage", before: "50 / 90 / 130 / 170 / 210", after: "50 / 95 / 140 / 185 / 230", text: null }] }] },
          { name: "Jinx", context: "Jinx is a touch weak after the item changes, so we're buffing her base attack speed to help her early trades.", notes: [], groups: [{ title: "Base Stats", lines: [{ label: "Attack Speed", before: "0.625", after: "0.645", text: null }] }] },
          { name: "Lee Sin", context: "Dragon’s Rage deals less damage at every rank.", notes: [], groups: [
            { title: "R", lines: [{ label: "Kick Damage", before: "175 / 400 / 625 (+200% bonus AD)", after: "150 / 350 / 550 (+200% bonus AD)", text: null }] },
            { title: "E - Tempest", lines: [{ label: "Cooldown", before: "12s", after: "15s", text: null }] },
          ] },
        ] },
        { title: "Items", paragraphs: [], entries: [
          { name: "Dawncore", itemId: 6621, context: "Dawncore is the enchanter capstone that isn't. We're buffing its AP value to make it less purely bound to Healing and Shielding.", notes: [], groups: [{ title: null, lines: [{ label: "Ability Power per 100% Base Mana Regeneration", before: "10", after: "15", text: null }] }] },
          { name: "Hubris", itemId: 6697, context: "Hubris has been overperforming for ranged users, so we're hitting it with a nerf for ranged while helping melee.", notes: [], groups: [{ title: null, lines: [{ label: "Base Bonus Attack Damage", before: "12", after: "15 Melee / 10 Ranged", text: null }] }] },
          { name: "New Preview Relic", itemId: null, context: null, notes: [], groups: [{ title: null, lines: [{ label: "Health", before: "100", after: "120", text: null }] }] },
        ] },
        { title: "Runes", paragraphs: [], entries: [
          { name: "Lethal Tempo", context: "Adjusting the early attack speed and late-game scaling.", notes: [], groups: [{ title: null, lines: [{ label: "Attack speed per stack", before: "5–9%", after: "6–8%", text: null }] }] },
        ] },
        { title: "Summoner’s Rift Changes", paragraphs: [], entries: [
          { name: "Jungle", context: "A small adjustment to jungle monster respawns.", notes: [], groups: [{ title: "Monster camps", lines: [{ label: "Respawn timer", before: "2:15", after: "2:10", text: null }, { label: "New", before: null, after: "The minimap timer appears 15 seconds before a camp respawns.", text: null }] }] },
        ] },
        // Deliberately mixed source data: the reader must filter these out.
        { title: "ARAM Mayhem", paragraphs: ["Mayhem-exclusive balance."], entries: [{ name: "Azir", context: "An ARAM buff.", notes: [], groups: [{ title: null, lines: [{ label: "Damage", before: "100%", after: "110%", text: null }] }] }] },
        { title: "Arena", paragraphs: ["Arena-exclusive changes."], entries: [] },
        { title: "Upcoming Skins", paragraphs: ["New cosmetic releases."], entries: [] },
        { title: "Bugfixes & QoL Changes", paragraphs: [], entries: [
          { name: "", context: null, notes: [], groups: [{ title: null, lines: [
            { label: null, before: null, after: null, text: "Fixed a bug where a tooltip displayed the wrong cooldown." },
            { label: null, before: null, after: null, text: "Fixed a bug where Lee Sin’s ability could be cast twice in one frame." }] }] },
        ] },
      ],
      };
      if (a.version === "26.20") return {
        ...notes, patch: "26.20", title: "League of Legends Patch 26.20 Notes", date: "2026-10-06T18:00:00.000Z",
        articleUrl: "https://www.leagueoflegends.com/en-us/news/game-updates/league-of-legends-patch-26-20-notes/",
        sections: [
          { title: "Champions", paragraphs: [], entries: [
            { name: "Caitlyn", context: "Piltover Peacemaker gains damage at every rank.", notes: [], groups: [{ title: "Q", lines: [{ label: "Base Damage", before: "45 / 85 / 125 / 165 / 205", after: "50 / 90 / 130 / 170 / 210", text: null }] }] },
            { name: "Lee Sin", context: "Tempest is available more often.", notes: [], groups: [{ title: "E - Tempest", lines: [{ label: "Cooldown", before: "12s", after: "10s", text: null }] }] },
          ] },
          { title: "Items", paragraphs: [], entries: [{ name: "Dawncore", itemId: 6621, context: null, notes: [], groups: [{ title: null, lines: [{ label: "Ability Power", before: "50", after: "60", text: null }] }] }] },
          { title: "Runes", paragraphs: [], entries: [{ name: "Lethal Tempo", context: null, notes: [], groups: [{ title: null, lines: [{ label: "Attack Speed", before: "4%", after: "5%", text: null }] }] }] },
          notes.sections[notes.sections.length - 1],
        ],
      };
      return notes;
    }
    case "get_champion_list": return Object.entries(CHAMPIONS).map(([id, [name]]) => ({ id: Number(id), name }));
    case "champion_icon": return `${DD}/${VERSION}/img/champion/${key(a.id)}.png`;
    // The app's tiles are the client's 380 px splash tiles; Data Dragon's are the same crops.
    // ---- glass: neutral client artwork (preview only; same allow-list as Rust) ----
    case 'ambient_background':
      if (a.name !== 'client') throw new Error('Unknown ambient background.');
      if (shellState.has('art-offline')) throw new Error('Preview artwork offline.');
      return 'https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/day1-background.jpg';
    // ---- end glass ----
    case "champion_art": return `${DD}/img/champion/${a.kind === "splash" ? "splash" : a.kind === "loading" ? "loading" : "tiles"}/${key(a.id)}_0.jpg`;
    case "item_icon": return `${DD}/${VERSION}/img/item/${a.id}.png`;
    case "spell_icon": return `${DD}/${VERSION}/img/spell/${({ 4: "SummonerFlash", 12: "SummonerTeleport", 11: "SummonerSmite", 14: "SummonerDot" } as Record<number, string>)[a.id] ?? "SummonerFlash"}.png`;
    case "get_duo_list": {
      const duoState = params.get("duos");
      if (duoState === "loading") await new Promise(resolve => setTimeout(resolve, 1200));
      if (duoState === "error") throw new Error("Preview duo data unavailable.");
      const pairs: [number, number, string, number, number][] = [
        [222, 902, "S+", 54.8, 18420], [67, 412, "S", 53.9, 12680], [51, 89, "A", 53.2, 11450],
        [115, 902, "A", 54.1, 1420], [30, 412, "A", 55.0, 780], [81, 902, "A", 52.6, 9680],
        [115, 89, "B", 53.7, 1230], [50, 89, "B", 53.4, 940], [45, 412, "B", 53.1, 650], [910, 89, "B", 53.0, 750],
        [222, 412, "B", 51.9, 15230], [67, 89, "B", 51.4, 7840], [147, 902, "C", 52.4, 580],
        [63, 412, "C", 52.3, 560], [901, 902, "C", 51.8, 1310], [51, 412, "C", 50.7, 6420], [81, 89, "D", 49.8, 3910],
      ];
      if (duoState === "paged") {
        pairs.length = 0;
        for (const bot of [222, 67, 51, 81, 115, 30, 50, 45, 910, 147, 63, 901]) {
          for (const support of [902, 412, 89, 117]) pairs.push([bot, support, "B", 51.7, 2400]);
        }
      }
      return {
        rows: ["empty", "unavailable"].includes(duoState ?? "") ? [] : pairs.map(([adcId, supportId, tier, winPct, games]) => ({
          adcId, adcName: CHAMPIONS[adcId]?.[0] ?? DUO_CHAMPIONS[adcId], supportId, supportName: CHAMPIONS[supportId]?.[0] ?? DUO_CHAMPIONS[supportId], tier,
          winPct: a.tier === "diamond_plus" ? winPct - 0.9 : winPct, games: a.tier === "diamond_plus" ? Math.round(games / 2) : games,
        })),
        unavailable: duoState === "unavailable", stale: duoState === "stale", updatedAt: now - 3600000,
      };
    }
    case "get_tier_list": {
      const pool: Record<string, number[]> = { top: tiers.map(([id]) => id), jungle: [64, 11], mid: [103, 7, 157, 777, 268, 238], adc: [222, 67, 51, 81], support: [902, 412, 89] };
      const rows = (pool[a.position] ?? []).map((id, i) => {
        const ranked = tiers.find(([championId]) => championId === id);
        return { championId: id, name: CHAMPIONS[id][0], tier: ranked?.[1] ?? "B", winPct: ranked?.[2] ?? 50.5, pickPct: 9 - i * .5, banPct: 4 + i % 3, games: 24000 - i * 1200 };
      });
      if (a.position === "top") rows.push({ championId: 7, name: "LeBlanc", tier: "D", winPct: 49, pickPct: .2, banPct: 4, games: 775 });
      return { unavailable: false, stale: false, updatedAt: null, rows };
    }
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
    case "champion_abilities":
      if (params.has("patch-abilities-offline")) throw new Error("Preview abilities unavailable");
      return { championId: a.championId, tooltipsReady: true, summoners: [], passive: a.championId === 51
        ? { name: "Headshot", iconPath: "/lol-game-data/assets/caitlyn-passive.png", description: "Caitlyn periodically fires an empowered attack.", tooltip: null }
        : { name: "Deathbringer Stance", iconPath: null, description: "Periodically, Aatrox's next basic attack deals bonus physical damage and heals him.", tooltip: "Periodically, Aatrox's next attack deals (+4–8% target max Health) physical damage and heals him for the damage dealt (based on level)." },
      spells: ["Q", "W", "E", "R"].map((key, i) => ({ key, name: ["The Darkin Blade", "Infernal Chains", "Umbral Dash", "World Ender"][i], iconPath: null,
        description: ["Aatrox slams his greatsword down three times; each strike is stronger at the edge.", "Aatrox smashes the ground, damaging and slowing the first enemy hit, then pulls them back.", "Passive: Aatrox heals from champion damage. Active: Aatrox dashes, empowering his next attacks.", "Aatrox unleashes his demonic form, fearing nearby minions and gaining Attack Damage and healing."][i],
        tooltip: i === 0 ? "Aatrox slams his greatsword down, dealing 10/25/40/55/70 (+60/70/80/90/100% AD) physical damage.\n\nEnemies hit at the edge take increased damage and are knocked up." : null,
        cooldowns: key === "R" ? [120, 100, 80] : [14, 12, 10, 8, 6], costs: [], ranges: key === "R" ? [] : [625, 625, 625, 625, 625], charges: key === "E" ? 2 : 0, recharge: key === "E" ? [9, 8, 7, 6, 5] : [] })) };
    case "ability_icon": return `${DD}/${VERSION}/img/passive/Caitlyn_Headshot.png`;
    case "full_app_abilities":
      if (params.has("patch-abilities-offline")) throw new Error("Preview abilities unavailable");
      if (a.championId === 51) return [{ key: "Q", name: "Piltover Peacemaker", icon: `${DD}/${VERSION}/img/spell/CaitlynQ.png` }];
      // Empty E metadata exercises the fallback to Riot's name in the patch text.
      if (a.championId === 64) return [{ key: "R", name: "Dragon's Rage", icon: `${DD}/${VERSION}/img/spell/LeeSinR.png` }, { key: "E", name: "", icon: null }];
      return ["Q", "W", "E", "R"].map(k => ({ key: k, name: `Ability ${k}`, icon: `${DD}/${VERSION}/img/spell/Aatrox${k}.png` }));
    case "get_runes": return { phase: "None", championId: 0, championName: "", position: "top", presets: [], trees: [], shards: [], applied: null, autoApply: false, importItems: true, canApply: false, locked: false, spells: { spell1Id: 4, spell2Id: 12, available: [], applyWithRunes: true }, enemies: [], enemySlots: 5 };
    case "get_pro_builds": return { championId: 266, position: "top", role: "top", page: 1, hasMore: false, unavailable: false, message: null, stale: false, updatedAt: null, matches: [1, 2, 3].map(i => ({ matchId: i, proName: `Pro player ${String.fromCharCode(64 + i)}`, team: `Team ${i}`, league: "LCK", region: "KR", role: "top", roleLabel: "Top", win: i % 2 === 1, playedAgo: `${i * 3}h ago`, patch: "16.21", ...runePage(8400, 8437, [8446, 8429, 8451], 8000, [9111, 9104]), kills: 4 + i, deaths: 2, assists: 6, spells: [4, 12], finalItems: [6610, 3053, 3071, 3047, 6333, 3026].map(named), completedItems: [6610, 3053, 3071, 6333].map(named), skillOrder: SKILLS.split(""), itemPath: [], otp: false })) };
    case "role_icon": return `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/svg/position-${a.role}.svg`;
    // --- clientctl preview block: restart client / Deceive restart (?restart=ask|defer|save|restart) ---
    case "save_settings": return state;
    case "client_health": return new URLSearchParams(location.search).get("client") ?? "ok"; // ?client=hung|dead|closed
    // ?emblem loads the real art from CommunityDragon; e2e runs offline, so it stays off by default.
    case "full_app_player_mmr": return { tier: "EMERALD", division: "I", lp: 18, games: 10, trend: -12 };
    // ?update: an update is waiting, so the title bar shows its pill.
    case "update_status": return { state: params.has("update") ? "available" : "upToDate", currentVersion: "1.0.3", availableVersion: params.has("update") ? "1.0.4" : null, notes: null, message: null, lastCheck: null };
    case "level_border": { const level = Number(a.level); const theme = level < 30 ? 1 : level < 50 ? 2 : 3 + Math.floor((Math.min(level, 500) - 50) / 25); return `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/uikit/themed-borders/theme-${theme}-border.png`; }
    case "rank_emblem": return params.has("emblem") ? `https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-emblem/emblem-${a.tier}.png` : null;
    case "riot_client_icon": return null; // PLAY falls back to its own icon; Swapper ships no Riot artwork.
    case "restart_plan": return new URLSearchParams(location.search).get("restart") ?? "restart";
    case "switch_account": {
      const account = state.accounts.find(x => x.id === a.id);
      setTimeout(() => void emit("switch_started", { id: a.id, name: account?.name ?? "" }), 0);
      setTimeout(() => void emit("switch_done", { id: a.id, view: state }), 500);
      return null;
    }
    // ---- end clientctl cases ----
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
  // ?ended: a game just ended and is not in match history yet.
  if (params.has("ended")) setTimeout(() => void emit("game_ended", { ...games[0], id: 999, win: true, kills: 7, deaths: 2, assists: 9, createdAt: Date.now() - 1830e3, lpChange: null }), 800);
});
