import type { CSSProperties, ReactNode } from "react";
import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import "./ui.css";

/** Static placeholder blocks (no shimmer: nothing loops) with a screen-reader label. */
export function Skeleton({ label, rows = 3, height = 44 }: { label: string; rows?: number; height?: number }) {
  return <div className="ui-skeleton" role="status" aria-label={label} style={{ "--row": `${height}px` } as CSSProperties}>
    {Array.from({ length: rows }, (_, i) => <span key={i} />)}
  </div>;
}

/** Centered message for a page or card with nothing to show yet. */
export function EmptyState({ icon, title, children, action }: { icon?: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return <section className="ui-empty">
    {icon && <span className="ui-empty-icon" aria-hidden>{icon}</span>}
    <h2>{title}</h2>
    {children && <p>{children}</p>}
    {action}
  </section>;
}

const icons = { info: Info, warn: TriangleAlert, error: CircleAlert };

/** Inline notice row: icon, text, optional action. Errors are announced (role alert). */
export function Notice({ tone = "info", children, action }: { tone?: keyof typeof icons; children: ReactNode; action?: ReactNode }) {
  const Icon = icons[tone];
  return <div className={`ui-notice is-${tone}`} role={tone === "error" ? "alert" : "status"}>
    <Icon size={16} aria-hidden /><span>{children}</span>{action}
  </div>;
}
