export type Choices = {
  fullApp: boolean;
  startup: boolean;
  overlay: boolean;
  gold: boolean;
  objectives: boolean;
  skills: boolean;
  deceive: boolean;
  koreanFont: boolean;
  remote: boolean;
};

export const STEP_TITLES = ["How you use Swapper", "In game", "Extras"] as const;
/** Dispatched on `window` by Settings → General to reopen the setup. */
export const RUN_EVENT = "swapper:run-onboarding";

/** The choices that differ from what is already saved; only these are written. */
export function changedKeys(initial: Choices, next: Choices): (keyof Choices)[] {
  return (Object.keys(next) as (keyof Choices)[]).filter(key => initial[key] !== next[key]);
}

export const clampStep = (step: number) => Math.min(STEP_TITLES.length - 1, Math.max(0, step));
