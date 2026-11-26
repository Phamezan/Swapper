// Dev-only preview of the Fullsized App (Dashboard, Tier list, Matchups, Live
// Game) from fixtures with a mocked Tauri IPC: `npm run dev`, then open
// /src/full-app/full-app-preview.html (?live=1 starts a fixture game; ?no-active
// signs League Client in without a Swapper-tracked account). Art comes
// from Data Dragon; names and Riot IDs are invented. Never part of the app build.
import { emit } from "@tauri-apps/api/event";
import { installFullAppMock } from "./full-app-preview-mock";

const params = new URLSearchParams(location.search);
const { games } = installFullAppMock();

void import("./main").then(() => {
  // ?switching: the switch flow only reports started/done/failed, so that is all the preview fakes.
  if (params.has("switching")) setTimeout(() => void emit("switch_started", { id: "b", name: "Alt Account" }), 1200);
  // ?ended: a game just ended and is not in match history yet.
  if (params.has("ended")) setTimeout(() => void emit("game_ended", { ...games[0], id: 999, win: true, kills: 7, deaths: 2, assists: 9, createdAt: Date.now() - 1830e3, lpChange: null }), 800);
});
