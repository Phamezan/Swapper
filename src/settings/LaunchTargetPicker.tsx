import type { ReactNode } from "react";
import type { LaunchTarget } from "../app/types";
import { LAUNCH_TARGET_NAMES, launchOverride, resolveLaunchTarget } from "../app/launchTarget";

/** Small inline marks in each game's own colours (no Riot assets ship with the app). */
const MARKS: Record<LaunchTarget, ReactNode> = {
  riot_client: <>
    <path fill="#d13639" fillRule="evenodd" d="M4 6.2 13.6 3l6.9 2.6-1.2 9.6H9.4L8.3 11l-2.6 1.1zM12 7.2h.9v7.2H12zm3.6-.6h.9v7.8h-.9z" />
    <path fill="#d13639" d="M9.6 17h4.2l-.1 3.2H10zm5.2 0h4.3l-.4 3.2h-4z" />
  </>,
  league: <>
    <path fill="#c8aa6e" d="M5.5 3h5.2v14.2h7.6L16.2 21H5.5l1.6-2.2V5.2z" />
    <path fill="none" stroke="#0ac8b9" strokeWidth="1.6" d="M13.4 5.2a7.6 7.6 0 0 1 5.2 11.2" />
  </>,
  valorant: <path fill="#ff4655" d="M2 4.5v5.2l8.6 10.8h6.3zm20 0-8.5 10.7h6.3l2.2-2.8z" />,
};

const TARGETS = ["riot_client", "league", "valorant"] as const;

type Props = {
  label: string;
  value: LaunchTarget | null;
  onChange: (target: LaunchTarget | null) => void;
  disabled?: boolean;
  /** Per-account picker: `value` null shows `global` as selected, and picking `global` stores null again. */
  perAccount?: boolean;
  global?: LaunchTarget;
};

/** What a switch opens afterwards: the house segmented control, with each game's mark and accent. */
export function LaunchTargetPicker({ label, value, onChange, disabled, global, perAccount }: Props) {
  const shown = resolveLaunchTarget(value, global);
  return (
    <div className="segmented launch-picker" role="group" aria-label={label}>
      {TARGETS.map(id => {
        const on = shown === id;
        return (
          <button key={id} type="button" data-game={id} disabled={disabled} className={on ? "is-on" : ""} aria-pressed={on}
            title={perAccount && on && value === null ? "Following Settings" : undefined}
            onClick={() => onChange(perAccount ? launchOverride(id, global) : id)}>
            <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden focusable="false">{MARKS[id]}</svg>
            <span>{LAUNCH_TARGET_NAMES[id]}</span>
          </button>
        );
      })}
    </div>
  );
}
