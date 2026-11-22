import { useEffect, useRef, useState } from "react";

/** Calls `onDismiss` after `ms` of the banner being on screen; hover or a hidden window pauses the count. */
export function useAutoDismiss(key: string | null, onDismiss: () => void, ms = 5000) {
  const [hovered, setHovered] = useState(false);
  const [hidden, setHidden] = useState(() => document.hidden);
  const remaining = useRef(ms);
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;

  useEffect(() => { remaining.current = ms; }, [key, ms]);
  useEffect(() => {
    const sync = () => setHidden(document.hidden);
    document.addEventListener("visibilitychange", sync);
    return () => document.removeEventListener("visibilitychange", sync);
  }, []);

  const paused = hovered || hidden;
  useEffect(() => {
    if (key === null || paused) return;
    const started = Date.now();
    const timer = window.setTimeout(() => dismiss.current(), remaining.current);
    return () => { window.clearTimeout(timer); remaining.current -= Date.now() - started; };
  }, [key, paused]);

  return { paused, hoverProps: { onMouseEnter: () => setHovered(true), onMouseLeave: () => setHovered(false) } };
}
