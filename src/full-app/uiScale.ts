import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { uiScale } from "./uiScaleModel";

const AUTO_SCALE_KEY = "ui-auto-scale";
const AUTO_SCALE_EVENT = "ui-auto-scale-change";

export function autoScaleEnabled() {
  try { return localStorage.getItem(AUTO_SCALE_KEY) !== "off"; } catch { return true; }
}

export function setAutoScaleEnabled(enabled: boolean) {
  try { localStorage.setItem(AUTO_SCALE_KEY, enabled ? "on" : "off"); } catch { /* applies for this session only */ }
  window.dispatchEvent(new CustomEvent(AUTO_SCALE_EVENT, { detail: enabled }));
}

/** Zooms the whole webview to the window's size. Native zoom (like browser zoom) keeps rects, vh and fixed popovers consistent. */
export function useUiScale() {
  useEffect(() => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    const webview = getCurrentWebview();
    let enabled = autoScaleEnabled();
    let applied = 1;
    let live = true;
    const apply = async () => {
      // Logical window size: independent of the webview zoom and of the monitor's DPI.
      const [size, dpi] = await Promise.all([appWindow.innerSize(), appWindow.scaleFactor()]);
      const zoom = enabled ? uiScale(size.width / dpi, size.height / dpi) : 1;
      if (!live || zoom === applied) return;
      applied = zoom;
      await webview.setZoom(zoom);
    };
    const run = () => void apply().catch(() => { /* keeps the current zoom */ });
    const toggle = (event: Event) => { enabled = (event as CustomEvent<boolean>).detail; run(); };
    window.addEventListener(AUTO_SCALE_EVENT, toggle);
    const stops = Promise.all([appWindow.onResized(run), appWindow.onScaleChanged(run)]);
    run();
    return () => {
      live = false;
      window.removeEventListener(AUTO_SCALE_EVENT, toggle);
      void stops.then(list => list.forEach(stop => stop()));
    };
  }, []);
}
