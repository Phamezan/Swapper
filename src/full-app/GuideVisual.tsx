import { useId, useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

export function GuideVisual({ title, preference, header, children }: {
  title: string; preference: "trades" | "spikes"; header?: ReactNode; children: ReactNode;
}) {
  const storageKey = `guide-visual-${preference}-expanded`;
  const [expanded, setExpanded] = useState(() => {
    try { return localStorage.getItem(storageKey) !== "false"; } catch { return true; }
  });
  const id = useId();
  const toggle = () => {
    setExpanded(!expanded);
    try { localStorage.setItem(storageKey, String(!expanded)); } catch { /* preference only */ }
  };
  return <section className={`mg-data-visual mg-${preference}${expanded ? "" : " is-collapsed"}`} aria-label={title}>
    <header className="mg-visual-header"><h3>{title}</h3>{header}
      <button type="button" className="mg-visual-toggle" aria-label={`${expanded ? "Hide" : "Show"} ${title.toLowerCase()}`} aria-expanded={expanded} aria-controls={id} onClick={toggle}><ChevronDown size={16} aria-hidden /></button>
    </header>
    <div id={id} hidden={!expanded}>{children}</div>
  </section>;
}
