import { Icon, type IconProps } from "./Icon";

// Swapper icon set. 24px grid, live area 3–21, 1.75 stroke, radius 1.5–3 on corners.
// Motifs: the brand's half-barb arrows (switch, refresh), the hexagon (settings, error), card slots (draft, tierlist).
// `ic-duo` marks the shape that fills with the brand green when the icon sits in an active nav item.
export type { IconProps };

/* Navigation */

export const DashboardIcon = (p: IconProps) => <Icon {...p}>
  <rect className="ic-duo" x="3.5" y="3.5" width="7.5" height="10" rx="2" />
  <rect x="3.5" y="16" width="7.5" height="4.5" rx="1.5" />
  <rect x="13" y="3.5" width="7.5" height="4.5" rx="1.5" />
  <rect x="13" y="10.5" width="7.5" height="10" rx="2" />
</Icon>;

export const TierlistIcon = (p: IconProps) => <Icon {...p}>
  <rect className="ic-duo" x="3.5" y="4" width="4.5" height="4.5" rx="1.25" />
  <rect x="3.5" y="9.75" width="4.5" height="4.5" rx="1.25" />
  <rect x="3.5" y="15.5" width="4.5" height="4.5" rx="1.25" />
  <path d="M11 6.25h9.5M11 12h6.5M11 17.75h3.5" />
</Icon>;

export const MatchupsIcon = (p: IconProps) => <Icon {...p}>
  <path d="M20 4 9 15M4 4l11 11M7 13l4 4M17 13l-4 4M9 15l-3 3M15 15l3 3" />
  <circle className="ic-duo" cx="4.75" cy="19.25" r="1.25" />
  <circle className="ic-duo" cx="19.25" cy="19.25" r="1.25" />
</Icon>;

export const DraftIcon = (p: IconProps) => <Icon {...p}>
  <rect className="ic-duo" x="9.5" y="3" width="11" height="15" rx="2" />
  <path d="M9.5 6.5H5.5a2 2 0 0 0-2 2V19a2 2 0 0 0 2 2H14a2 2 0 0 0 2-2v-1" />
  <path d="M15 7.5l2.25 3-2.25 3-2.25-3z" />
</Icon>;

export const LiveGameIcon = (p: IconProps) => <Icon {...p}>
  <rect className="ic-duo" x="3.5" y="3.5" width="17" height="17" rx="3" />
  <circle cx="12" cy="12" r="1.75" fill="currentColor" />
  <path d="M8.75 8.75a4.6 4.6 0 0 0 0 6.5M15.25 8.75a4.6 4.6 0 0 1 0 6.5" />
</Icon>;

/** Patch notes: a sheet with an up/down change pair. */
export const PatchNotesIcon = (p: IconProps) => <Icon {...p}>
  <path className="ic-duo" d="M6 3.5h9L19 7.5V20a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 20V5A1.5 1.5 0 0 1 6 3.5Z" />
  <path d="M14.5 3.5v4.5H19M8.5 12.5 10 11l1.5 1.5M8.5 17.5 10 19l1.5-1.5M14 11v8" />
</Icon>;

export const AccountsIcon = (p: IconProps) => <Icon {...p}>
  <circle className="ic-duo" cx="9.5" cy="8.5" r="3.5" />
  <path d="M3.5 20a6 6 0 0 1 12 0M15.5 5.25a3.5 3.5 0 0 1 0 6.5M17.75 14.5a6 6 0 0 1 2.75 5.5" />
</Icon>;

export const SettingsIcon = (p: IconProps) => <Icon {...p}>
  <path className="ic-duo" d="M12 3l7.8 4.5v9L12 21l-7.8-4.5v-9z" />
  <circle cx="12" cy="12" r="3" />
</Icon>;

/* Actions */

/** The brand mark's two half-barb arrows. */
export const SwitchIcon = (p: IconProps) => <Icon {...p}>
  <path d="M4.5 9h14.5l-4-4M19.5 15H5l4 4" />
</Icon>;

export const AddAccountIcon = (p: IconProps) => <Icon {...p}>
  <circle cx="10" cy="8.5" r="3.5" />
  <path d="M4 20a6 6 0 0 1 9.5-4.9M18 14v6M15 17h6" />
</Icon>;

export const AddIcon = (p: IconProps) => <Icon {...p}><path d="M12 5v14M5 12h14" /></Icon>;

export const SearchIcon = (p: IconProps) => <Icon {...p}>
  <circle cx="10.5" cy="10.5" r="6.5" />
  <path d="M15.5 15.5 20 20" />
</Icon>;

export const BackIcon = (p: IconProps) => <Icon {...p}><path d="M14.5 6 8.5 12l6 6" /></Icon>;

export const ForwardIcon = (p: IconProps) => <Icon {...p}><path d="M9.5 6l6 6-6 6" /></Icon>;

/** One arc with the brand's half barb. */
export const RefreshIcon = (p: IconProps) => <Icon {...p}>
  <path d="M19.5 12A7.5 7.5 0 1 1 17 6.4L19.5 9M19.5 4.5V9H15" />
</Icon>;

/** Rounded play triangle. */
export const PlayIcon = (p: IconProps) => <Icon {...p}>
  <path className="ic-duo" d="M8 5.6v12.8a1.5 1.5 0 0 0 2.3 1.27l10-6.4a1.5 1.5 0 0 0 0-2.54l-10-6.4A1.5 1.5 0 0 0 8 5.6Z" />
</Icon>;

export const CheckIcon = (p: IconProps) => <Icon {...p}><path d="M5 12.5 9.5 17 19 7.5" /></Icon>;

/** Open wrench head and a rounded handle, on the shared 24px grid. */
export const WrenchIcon = (p: IconProps) => <Icon {...p}>
  <path d="m14.5 4.2-2.8 2.8 1.3 4 4 1.3 2.8-2.8a5.5 5.5 0 0 1-7.2 7.2l-6 4a2.1 2.1 0 0 1-3-3l4-6a5.5 5.5 0 0 1 6.9-7.5Z" />
  <path d="m5.5 18.5.01-.01" />
</Icon>;

/* Status (banners and notices) */

export const InfoIcon = (p: IconProps) => <Icon {...p}>
  <circle cx="12" cy="12" r="8.5" />
  <path d="M12 11v5M12 7.75h.01" />
</Icon>;

export const WarningIcon = (p: IconProps) => <Icon {...p}>
  <path d="M10.3 4.5a2 2 0 0 1 3.4 0l7.1 12.5a2 2 0 0 1-1.7 3H4.9a2 2 0 0 1-1.7-3z" />
  <path d="M12 9.5v4M12 16.75h.01" />
</Icon>;

/** The settings hexagon, so errors read as Swapper's own. */
export const ErrorIcon = (p: IconProps) => <Icon {...p}>
  <path d="M12 3l7.8 4.5v9L12 21l-7.8-4.5v-9z" />
  <path d="M12 8v5M12 16h.01" />
</Icon>;

export const SuccessIcon = (p: IconProps) => <Icon {...p}>
  <circle cx="12" cy="12" r="8.5" />
  <path d="M8 12.25l2.75 2.75L16 9.75" />
</Icon>;
