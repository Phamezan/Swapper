import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { LiveGameView } from "./LiveGameModel";

/** Live Client numbers are re-read this often, only while the page is visible.
 *  The backend also refuses reads closer than 2.5 s. */
export const LIVE_REFRESH_MS = 3000;

/** Whether a game is running, from the backend's gameflow watcher. */
export function useInGame(): boolean {
  const [inGame, setInGame] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    const stop = listen<boolean>("live_game_state", ({ payload }) => { if (live) setInGame(payload); });
    void invoke<boolean>("live_game_active").then(active => { if (live) setInGame(active); }).catch(() => {});
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, []);
  return inGame;
}

/** The current game: built once, then each player as their data lands
 *  (`live_game` events). With `refreshLive`, Live Client numbers are merged in
 *  every few seconds while the document is visible, and never while hidden. */
export function useLiveGame(refreshLive: boolean) {
  const [view, setView] = useState<LiveGameView | null>(null);
  const [error, setError] = useState("");
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    if (!isTauri()) { setLoaded(true); return; }
    let live = true;
    const stop = listen<LiveGameView>("live_game", ({ payload }) => {
      if (live) setView(current => (!current || current.gameId === payload.gameId ? payload : current));
    });
    void invoke<LiveGameView | null>("live_game_view")
      .then(next => { if (live) { setView(next); setError(""); } })
      .catch(reason => { if (live) setError(String(reason)); })
      .finally(() => { if (live) setLoaded(true); });
    return () => { live = false; void stop.then(unlisten => unlisten()); };
  }, []);
  useEffect(() => {
    if (!refreshLive || !isTauri()) return;
    let live = true;
    let timer: number | undefined;
    const read = () => void invoke<LiveGameView | null>("live_game_live")
      .then(next => { if (live && next) setView(next); })
      .catch(() => {}); // Loading screen or game closing: keep what is shown.
    const sync = () => {
      window.clearInterval(timer);
      timer = undefined;
      if (document.visibilityState !== "visible") return;
      read();
      timer = window.setInterval(read, LIVE_REFRESH_MS);
    };
    sync();
    document.addEventListener("visibilitychange", sync);
    return () => { live = false; window.clearInterval(timer); document.removeEventListener("visibilitychange", sync); };
  }, [refreshLive]);
  return { view, error, loaded };
}
