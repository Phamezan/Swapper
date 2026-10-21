import { Fragment, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import "./ui.css";

export type Crumb = { label: string; onClick?: () => void };

/** One-row page header: optional breadcrumb trail, the page title, right-side actions. */
export function PageHeader({ title, crumbs, actions }: { title: ReactNode; crumbs?: Crumb[]; actions?: ReactNode }) {
  return <header className="ui-page-header">
    <div className="ui-page-title">
      {crumbs?.length ? <nav className="ui-crumbs" aria-label="Breadcrumb">{crumbs.map(crumb => <Fragment key={crumb.label}>
        {crumb.onClick ? <button type="button" onClick={crumb.onClick}>{crumb.label}</button> : <span>{crumb.label}</span>}
        <ChevronRight size={15} aria-hidden />
      </Fragment>)}</nav> : null}
      <h1>{title}</h1>
    </div>
    {actions && <div className="ui-page-actions">{actions}</div>}
  </header>;
}
