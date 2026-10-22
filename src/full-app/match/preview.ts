import type { Game, Snapshot } from "../types";
import type { MatchView, TimelineEvent } from "../MatchDetailModel";

// Synthetic preview only. Field names were verified against the running client.
export function matchDetailFixture(game: Game, ranks: Snapshot["rank"][], params: URLSearchParams): MatchView {
  if (params.has("timeline-long")) game = {...game, duration: 34 * 60};
  const side = (id: number) => params.has("detail-red") ? 300 - id : id;
  const players = game.players.map((p, i) => {
    const leading = (i < 5) === game.win;
    const gold = leading ? 13600 - i % 5 * 720 : 11200 - i % 5 * 520;
    const damage = p.damage ?? 0, vision = p.role === "support" ? 64 + i : p.vision;
    const jungle = p.role === "jungle" ? Math.min(p.cs ?? 0, 170) : 12;
    const stats: Record<string, number | boolean> = {
      kills: p.kills ?? 0, deaths: p.deaths ?? 0, assists: p.assists ?? 0, champLevel: 18 - i % 4,
      largestKillingSpree: [4, 2, 6, 3, 1, 2, 1, 2, 1, 0][i], killingSprees: 2 + i % 3, largestMultiKill: i === 2 ? 3 : 2,
      doubleKills: i % 3, tripleKills: i === 2 ? 1 : 0, quadraKills: 0, pentaKills: 0, unrealKills: 0,
      firstBloodKill: i === 2, firstBloodAssist: i === 1, totalTimeCrowdControlDealt: 160 + i * 24,
      timeCCingOthers: 18 + i * 4, longestTimeSpentLiving: 220 + i * 31,
      totalDamageDealtToChampions: damage, physicalDamageDealtToChampions: Math.round(damage * .72),
      magicDamageDealtToChampions: Math.round(damage * .2), trueDamageDealtToChampions: Math.round(damage * .08),
      totalDamageDealt: damage * 6, physicalDamageDealt: Math.round(damage * 4.4), magicDamageDealt: Math.round(damage * 1.2), trueDamageDealt: Math.round(damage * .4),
      largestCriticalStrike: 120 + i * 54, damageDealtToTurrets: 8400 - i * 640, damageDealtToObjectives: 12000 + i * 420,
      totalDamageTaken: 21000 + i * 1400, physicalDamageTaken: 14000 + i * 900, magicalDamageTaken: 5000 + i * 300, trueDamageTaken: 2000 + i * 200,
      damageSelfMitigated: 18000 + i * 2400, totalHeal: 7400 + i * 480, totalUnitsHealed: p.role === "support" ? 5 : 1,
      visionScore: vision ?? 0, wardsPlaced: 9 + i, wardsKilled: i + 1, visionWardsBoughtInGame: 2 + i % 4, sightWardsBoughtInGame: 0,
      goldEarned: gold, goldSpent: gold - 620, totalMinionsKilled: (p.cs ?? 0) - jungle, neutralMinionsKilled: jungle,
      neutralMinionsKilledTeamJungle: jungle - 3, neutralMinionsKilledEnemyJungle: 3,
      turretKills: i === 0 ? 4 : i % 3, inhibitorKills: i === 2 ? 1 : 0,
      firstTowerKill: i === 0, firstTowerAssist: i === 1, firstInhibitorKill: i === 2, firstInhibitorAssist: i === 3,
      combatPlayerScore: 0, objectivePlayerScore: 0, totalPlayerScore: 0, totalScoreRank: 0,
      causedEarlySurrender: false, causedGameEndFromIGNBSurrender: false, earlySurrenderAccomplice: false,
      gameEndedInEarlySurrender: false, gameEndedInIGNBSurrender: false, gameEndedInSurrender: false, teamEarlySurrendered: false,
      wasSevereTransgressor: false, win: i < 5 ? game.win : !game.win,
      perkPrimaryStyle: 8000, perkSubStyle: 8400,
    };
    for (const [index, id] of [8010, 9111, 9104, 8299, 8444, 8242].entries()) {
      stats[`perk${index}`] = id;
      for (let n = 1; n <= 3; n++) stats[`perk${index}Var${n}`] = (i + index + n) * 24;
    }
    p.items.forEach((id, n) => { stats[`item${n}`] = id; });
    if (params.has("detail-missing") && i === 2) delete stats.visionScore;
    return { ...p, riotId: p.me ? "You#•••" : `Player ${i}#•••`, participantId: i + 1, teamId: side(i < 5 ? 100 : 200),
      level: 18 - i % 4, spells: [4, p.role === "jungle" ? 11 : p.role === "support" ? 14 : 12],
      perks: [8010, 9111, 9104, 8299, 8444, 8242], keystone: 8010, secondaryStyle: 8400,
      placement: params.has("detail-missing") || params.has("detail-partial") ? null : [2, 3, 1, 4, 5, 6, 7, 8, 9, 10][i],
      score: params.has("detail-missing") || params.has("detail-partial") ? null : [86, 82, 96, 74, 67, 60, 53, 46, 39, 32][i],
      // ---- matchdetail: current queue rank and LP, including fallback/unknown states ----
      rank: params.has("detail-unranked") && i === 1 ? null : {
        ...ranks[i]!, ...(i === 1 ? {tier: "MASTER", division: "I", lp: 200} : i === 2 ? {tier: "DIAMOND", division: "II", lp: 45} : {}),
        queue: game.queue === "flex" && !(params.has("detail-rank-fallback") && i === 2) ? "Flex" as const : "Solo/Duo" as const,
        ...(params.has("detail-rank-missing-lp") && i === 3 ? {lp: null} : {}),
      }, rankLoaded: true, gold, vision: params.has("detail-missing") && i === 2 ? null : vision,
      // ---- end matchdetail current ranks ----
      damageTaken: stats.totalDamageTaken as number, wardsPlaced: 9 + i, wardsKilled: i + 1, stats };
  });
  const finalDiff = players.reduce((sum, p) => sum + (p.teamId === 100 ? p.gold : -p.gold), 0);
  const finalGold = players.reduce((sum, p) => sum + p.gold, 0);
  const minutes = params.has("timeline-long") ? 34 : 25;
  const frames = Array.from({ length: minutes + 1 }, (_, i) => {
    const goldDiff = i === 0 ? 0 : i === minutes ? finalDiff : Math.round(-1700 * Math.sin(i / 5) + finalDiff * (i / minutes) ** 2);
    const blueGold = Math.round((5000 + (finalGold - 5000) * i / minutes + goldDiff) / 2);
    return {timestamp: Math.round(game.duration * 1000 * i / minutes), goldDiff, blueGold, redGold: blueGold - goldDiff};
  });
  const partial = params.has("detail-partial") || params.has("detail-partial-own");
  const kills = params.has("timeline-objectives-only") ? [] : [
    [380000, 1, 6], [421000, 7, 1], [434000, 2, 7], [446000, 3, 8],
    [785000, 1, 6], [798000, 8, 2], [1248000, 1, 9], [1268000, 10, 3],
  ].map(([timestamp, killerId, victimId]) => ({ timestamp, killerId, victimId, teamId: players[killerId - 1].teamId, label: "Champion kill" }));
  // ---- matchdetail graphs: dense, synthetic 34-minute fixture (72 kills + 18 objectives) ----
  const longEvents: TimelineEvent[] = Array.from({length: 72}, (_, i) => {
    const fight = Math.floor(i / 3), red = fight % 3 === 0;
    const killerId = (fight % 5) + (red ? 6 : 1), victimId = (i % 5) + (red ? 1 : 6);
    const locations = [[2200, 12200], [7200, 7600], [12400, 2500], [10800, 5200], [4800, 10200], [6400, 4700]];
    const [x, y] = locations[fight % locations.length];
    return {timestamp: 90000 + fight * 80000 + i % 3 * 4000, killerId, victimId,
      teamId: players[killerId - 1].teamId, label: "Champion kill",
      ...(params.has("timeline-no-positions") ? {} : {position: {x: x + (i * 127 % 1400), y: y + (i * 233 % 1200)}})};
  });
  for (let i = 0; i < 18; i++) longEvents.push({timestamp: 250000 + i * 97000, teamId: side(i % 4 === 0 ? 200 : 100), label: ["Dragon", "Tower", "Grub", "Herald", "Tower", "Baron"][i % 6]});
  // ---- end matchdetail graphs fixture ----
  return { game, players: params.has("detail-partial-own") ? players.filter((_, i) => i !== 4) : params.has("detail-partial") ? players.slice(0, 8) : players,
    teams: [100, 200].map((teamId, i) => ({teamId: side(teamId), win: i === 0 ? game.win : !game.win,
      objectives: partial ? [] : [{label: "Dragons", kills: i ? 1 : 3}, {label: "Heralds", kills: i ? 0 : 1}, {label: "Grubs", kills: i ? 2 : 4}, {label: "Barons", kills: i ? 0 : 1}, {label: "Towers", kills: (i === 0) === game.win ? 9 : 3}, {label: "Inhibitors", kills: i ? 0 : 1}] })),
    timeline: params.has("no-timeline") || partial ? null : {frames, events: params.has("timeline-long") ? longEvents.sort((a, b) => a.timestamp - b.timestamp) : [
      {timestamp: Math.round(game.duration * 1000 * .28), teamId: 200, label: "Dragon"},
      {timestamp: Math.round(game.duration * 1000 * .4), teamId: 100, label: "Tower"},
      {timestamp: Math.round(game.duration * 1000 * .52), teamId: 100, label: "Dragon"},
      {timestamp: Math.round(game.duration * 1000 * .66), teamId: 100, label: "Tower"},
      {timestamp: Math.round(game.duration * 1000 * .83), teamId: 100, label: "Baron"},
      {timestamp: Math.round(game.duration * 1000 * .9), teamId: 100, label: "Inhibitor"},
    ].map(event => ({ ...event, teamId: side(event.teamId) })).concat(kills).sort((a, b) => a.timestamp - b.timestamp),
    } };
}
