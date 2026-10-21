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
    let active = false;
    let stateReceived = false;
    let generation = 0;
    let timer: number | undefined;
    const visible = () => document.visibilityState === "visible";
    const readLive = () => {
      const started = generation;
      void invoke<LiveGameView | null>("live_game_live")
        .then(next => { if (live && active && visible() && started === generation) setView(next); })
        .catch(() => {});
    };
    const sync = () => {
      window.clearInterval(timer); timer = undefined;
      const started = ++generation;
      if (!active) { setView(null); setError(""); setLoaded(true); return; }
      if (!visible()) return;
      void invoke<LiveGameView | null>("live_game_view")
        .then(next => {
          if (!live || !active || !visible() || started !== generation) return;
          setView(next); setError("");
        })
        .catch(reason => { if (live && started === generation) setError(String(reason)); })
        .finally(() => {
          if (!live || started !== generation) return;
          setLoaded(true);
          if (refreshLive && active && visible()) { readLive(); timer = window.setInterval(readLive, LIVE_REFRESH_MS); }
        });
    };
    const stop = listen<LiveGameView>("live_game", ({ payload }) => {
      if (live && active && visible()) setView(current => (!current || current.gameId === payload.gameId ? payload : current));
    });
    const state = listen<boolean>("live_game_state", ({ payload }) => {
      stateReceived = true;
      if (live) { active = payload; sync(); }
    });
    void state.then(() => invoke<boolean>("live_game_active")).then(value => {
      if (live && !stateReceived) { active = value; sync(); }
    }).catch(() => {});
    document.addEventListener("visibilitychange", sync);
    return () => {
      live = false; generation++; window.clearInterval(timer);
      document.removeEventListener("visibilitychange", sync);
      void stop.then(unlisten => unlisten()); void state.then(unlisten => unlisten());
    };
  }, [refreshLive]);
  return { view, error, loaded };
}
