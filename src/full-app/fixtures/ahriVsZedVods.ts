import type { MatchupVod } from "../matchupGuide";

// Design preview only. These IDs do not refer to YouTube videos.
export const ahriVsZedVods: MatchupVod[] = [
  {
    videoId: "fixture-ahri-1", fixture: true,
    title: "Hold Charm. Close the kill window.", player: "Fixture Pro 1", team: "T1", rankClaim: "Challenger",
    patch: "26.21", result: "W", durationSec: 1746,
    moments: [
      { atSec: 184, label: "Trades while Living Shadow is down", section: "early-lane" },
      { atSec: 412, label: "Holds Charm through Death Mark", section: "overview" },
      { atSec: 768, label: "Crashes the wave before moving", section: "mid-lane" },
      { atSec: 1431, label: "Saves Spirit Rush for the second engage", section: "win-condition" },
    ],
  },
  {
    videoId: "fixture-ahri-2", fixture: true,
    title: "Turn lane priority into a side-lane lead.", player: "Fixture Pro 2", team: "Gen.G", rankClaim: "Grandmaster",
    patch: "26.21", result: "W", durationSec: 1538,
    moments: [
      { atSec: 155, label: "Keeps the level-two trade short", section: "early-lane" },
      { atSec: 624, label: "Finds a roam after the cannon crash", section: "mid-lane" },
      { atSec: 1202, label: "Uses fog to approach the objective", section: "win-condition" },
    ],
  },
  {
    videoId: "fixture-ahri-3", fixture: true,
    title: "A missed Charm changes the lane.", player: "Fixture Pro 3", team: "G2 Esports", rankClaim: "Master",
    patch: "26.21", result: "L", durationSec: 1924,
    moments: [
      { atSec: 213, label: "Spends Charm and gives up lane space", section: "early-lane" },
      { atSec: 537, label: "Resets instead of contesting at low health", section: "mid-lane" },
      { atSec: 1570, label: "Peels the carry instead of chasing Zed", section: "win-condition" },
    ],
  },
  {
    videoId: "fixture-ahri-4", fixture: true,
    title: "Make every shadow cast cost him space.", player: "Fixture Solo 1", rankClaim: "Challenger",
    patch: "26.21", result: "W", durationSec: 1685,
    moments: [
      { atSec: 169, label: "Steps forward as Living Shadow expires", section: "early-lane" },
      { atSec: 458, label: "Waits for the swap before aiming Charm", section: "overview" },
      { atSec: 1124, label: "Moves first to the objective after a crash", section: "win-condition" },
    ],
  },
  {
    videoId: "fixture-ahri-5", fixture: true,
    title: "Keep the all-in out of reach.", player: "Fixture Solo 2", rankClaim: "Grandmaster",
    patch: "26.21", result: "L", durationSec: 1840,
    moments: [
      { atSec: 246, label: "Punishes shadow used on the wave", section: "early-lane" },
      { atSec: 690, label: "Uses lane priority to enter the river first", section: "mid-lane" },
      { atSec: 1523, label: "Protects the back line by holding Charm", section: "win-condition" },
    ],
  },
];
