import { useEffect, useState } from "react";

/** Dispatched on `window` when settings change outside the panel showing them (e.g. the first-run setup). */
export const SETTINGS_CHANGED = "swapper:settings-changed";

export const announceSettingsChanged = () => window.dispatchEvent(new Event(SETTINGS_CHANGED));

/** A number that changes whenever settings were changed elsewhere; add it to an effect's deps to reload. */
export function useSettingsVersion(): number {
  const [version, setVersion] = useState(0);
  useEffect(() => {
    const bump = () => setVersion(current => current + 1);
    window.addEventListener(SETTINGS_CHANGED, bump);
    return () => window.removeEventListener(SETTINGS_CHANGED, bump);
  }, []);
  return version;
}
