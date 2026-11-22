import { DropdownMenu } from "radix-ui";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, ChevronDown, ChevronUp, MoreHorizontal, ShieldCheck, Wrench } from "lucide-react";
import { AddAccountIcon, GripIcon, RefreshIcon, ReorderIcon, SwitchIcon } from "./ui/icons";
import type { Account } from "../App";
import { Notice } from "./ui/States";
import { Tooltip } from "./ui/Tooltip";

export type AccountCardsProps = {
  accounts: Account[]; activeId: string | null; switchingId: string | null; repairableId: string | null; busy: boolean;
  closed?: boolean;
  onSwitch: (id: string) => void; onRestart?: () => void; onAdd: () => void; onRename: (account: Account) => void;
  onRemove: (id: string) => void; onRepair: (id: string) => void; onError: (reason: unknown) => void;
  /** Saves a new account order (optimistic; the caller rolls back on failure). */
  onReorder?: (ids: string[]) => void;
};

/** `ids` with `id` moved to index `to`. */
export const movedTo = (ids: string[], id: string, to: number) => {
  const rest = ids.filter(x => x !== id);
  return [...rest.slice(0, to), id, ...rest.slice(to)];
};
const DROP_MS = 160;
const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;
type Drag = { id: string; from: number; to: number; x: number; y: number; dx: number; dy: number; settling: boolean };

export function AccountCards(props: AccountCardsProps) {
  const [notice, setNotice] = useState("");
  const [announce, setAnnounce] = useState("");
  const [drag, setDrag] = useState<Drag | null>(null);
  // Reorder mode: cards become drag targets and their normal actions step aside.
  const [editing, setEditing] = useState(false);
  const grid = useRef<HTMLDivElement>(null);
  // Card slots measured when a drag starts; other cards slide between them.
  const slots = useRef<DOMRect[]>([]);
  const refocus = useRef<HTMLElement | null>(null);
  const ids = props.accounts.map(a => a.id);
  const canReorder = Boolean(props.onReorder) && !props.busy && ids.length > 1;
  // A moved card's DOM node is re-inserted, which drops focus; put it back (on the handle if that button is now disabled).
  useLayoutEffect(() => {
    const el = refocus.current;
    refocus.current = null;
    if (!el) return;
    const target = el.matches(":disabled") ? el.closest(".fa-account-card")?.querySelector<HTMLElement>(".fa-account-grip") : el;
    if (target && document.activeElement !== target) target.focus();
  }, [props.accounts]);
  // Leave reorder mode when a switch starts or an account is added or removed.
  useEffect(() => setEditing(false), [props.switchingId, ids.length]);
  useEffect(() => {
    if (!editing) return;
    const onEscape = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") setEditing(false); };
    window.addEventListener("keydown", onEscape);
    return () => window.removeEventListener("keydown", onEscape);
  }, [editing]);

  const commit = (id: string, to: number) => {
    if (to === ids.indexOf(id)) return;
    const name = props.accounts.find(a => a.id === id)?.name ?? "Account";
    setAnnounce(`Moved ${name} to position ${to + 1} of ${ids.length}`);
    props.onReorder?.(movedTo(ids, id, to));
  };
  const onKey = (event: KeyboardEvent, id: string, onGrip: boolean) => {
    if (!canReorder || (event.key !== "ArrowUp" && event.key !== "ArrowDown") || (!event.altKey && !onGrip)) return;
    // React bubbles through portals: keys in the card's actions menu are not the card's.
    if (!event.currentTarget.contains(event.target as Node)) return;
    event.preventDefault();
    const to = ids.indexOf(id) + (event.key === "ArrowUp" ? -1 : 1);
    if (to < 0 || to >= ids.length) return;
    refocus.current = document.activeElement as HTMLElement | null;
    commit(id, to);
  };
  const step = (event: { currentTarget: HTMLElement }, id: string, by: -1 | 1) => {
    refocus.current = event.currentTarget;
    commit(id, ids.indexOf(id) + by);
  };
  const startDrag = (event: PointerEvent<HTMLElement>, id: string) => {
    if (!editing || !canReorder || drag || event.button !== 0 || !grid.current) return;
    // Move up/down are buttons of their own, not drag starts.
    if ((event.target as Element).closest(".fa-account-step")) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    slots.current = [...grid.current.querySelectorAll<HTMLElement>(".fa-account-card")].map(card => card.getBoundingClientRect());
    const from = ids.indexOf(id);
    setDrag({ id, from, to: from, x: event.clientX, y: event.clientY, dx: 0, dy: 0, settling: false });
  };
  const moveDrag = (event: PointerEvent) => {
    if (!drag || drag.settling) return;
    const dx = event.clientX - drag.x, dy = event.clientY - drag.y;
    const own = slots.current[drag.from];
    const cx = own.left + own.width / 2 + dx, cy = own.top + own.height / 2 + dy;
    const distance = (r: DOMRect) => Math.hypot(r.left + r.width / 2 - cx, r.top + r.height / 2 - cy);
    const to = slots.current.reduce((best, r, i) => distance(r) < distance(slots.current[best]) ? i : best, 0);
    setDrag({ ...drag, dx, dy, to });
  };
  const endDrag = (cancelled: boolean) => {
    if (!drag || drag.settling) return;
    const settled = { ...drag, to: cancelled || props.busy ? drag.from : drag.to, settling: true };
    setDrag(settled);
    // Order and transforms change in one render with transitions off, so nothing jumps.
    window.setTimeout(() => { setDrag(null); commit(settled.id, settled.to); }, reducedMotion() ? 0 : DROP_MS);
  };
  const offset = (id: string, index: number): CSSProperties | undefined => {
    if (!drag) return undefined;
    const slot = (i: number) => slots.current[i];
    if (id === drag.id && !drag.settling) return { transform: `translate(${drag.dx}px, ${drag.dy}px)` };
    const target = id === drag.id ? drag.to : movedTo(ids, drag.id, drag.to).indexOf(id);
    if (!slot(index) || !slot(target)) return undefined;
    return { transform: `translate(${slot(target).left - slot(index).left}px, ${slot(target).top - slot(index).top}px)` };
  };

  const shortcut = async (id: string) => {
    try { await invoke("create_account_shortcut", { id }); setNotice("Desktop shortcut created."); }
    catch (reason) { props.onError(reason); }
  };
  const showReorder = Boolean(props.onReorder) && ids.length > 1;
  return <div className="fa-accounts dash-scope">
    {notice && <Notice>{notice}</Notice>}
    <p className="sr-only" aria-live="polite">{announce}</p>
    <div className="fa-launcher-intro"><p><ShieldCheck size={16} aria-hidden /> Pick an account to sign in.</p>
      {showReorder && (editing
        ? <button type="button" className="fa-primary fa-reorder-toggle" onClick={() => setEditing(false)}><Check size={16} aria-hidden /> Done</button>
        : <button type="button" className="fa-secondary fa-reorder-toggle" disabled={props.busy} onClick={() => setEditing(true)}><ReorderIcon size={16} /> Reorder</button>)}</div>
    {editing && <p className="fa-reorder-hint">Drag cards or use the arrows to set the order they appear in the account switcher.</p>}
    <div ref={grid} className={`fa-account-grid${drag ? " is-sorting" : ""}${editing ? " is-editing" : ""}`}>{props.accounts.map((account, index) => {
      const active = props.activeId === account.id;
      const switching = props.switchingId === account.id;
      const repair = props.repairableId === account.id;
      const dragging = drag?.id === account.id;
      return <article className={`fa-card fa-account-card${active ? " is-active" : ""}${dragging ? " is-dragging" : ""}${dragging && drag.settling ? " is-settling" : ""}`}
        key={account.id} style={offset(account.id, index)} onKeyDown={event => onKey(event, account.id, false)}
        onPointerDown={event => startDrag(event, account.id)} onPointerMove={moveDrag}
        onPointerUp={() => endDrag(false)} onPointerCancel={() => endDrag(true)}>
        <div className="fa-account-top"><span className={`fa-account-state${active ? " is-active" : ""}`}>{active ? <><Check size={12} /> Active session</> : repair ? <><Wrench size={12} /> Needs repair</> : "Ready"}</span>
          {editing ? <span className="fa-account-steps">
            <Tooltip label="Move up"><button type="button" className="fa-icon-button fa-account-step" disabled={!canReorder || index === 0} aria-label={`Move ${account.name} up`} onClick={event => step(event, account.id, -1)}><ChevronUp size={20} /></button></Tooltip>
            <Tooltip label="Move down"><button type="button" className="fa-icon-button fa-account-step" disabled={!canReorder || index === ids.length - 1} aria-label={`Move ${account.name} down`} onClick={event => step(event, account.id, 1)}><ChevronDown size={20} /></button></Tooltip>
          </span> : <DropdownMenu.Root><DropdownMenu.Trigger className="fa-icon-button" disabled={props.busy} aria-label={`Actions for ${account.name}`}><MoreHorizontal size={20} /></DropdownMenu.Trigger>
            <DropdownMenu.Portal><DropdownMenu.Content className="fa-account-menu" align="end" sideOffset={6}>
              <DropdownMenu.Item onSelect={() => props.onRename(account)}>Rename account</DropdownMenu.Item>
              <DropdownMenu.Item onSelect={() => props.onRepair(account.id)}>Repair saved session</DropdownMenu.Item>
              <DropdownMenu.Item onSelect={() => props.onAdd()}>Add / update account</DropdownMenu.Item>
              <DropdownMenu.Item onSelect={() => void shortcut(account.id)}>Create desktop shortcut</DropdownMenu.Item>
              <DropdownMenu.Separator /><DropdownMenu.Item className="is-danger" onSelect={() => props.onRemove(account.id)}>Remove account…</DropdownMenu.Item>
            </DropdownMenu.Content></DropdownMenu.Portal>
          </DropdownMenu.Root>}
        </div>
        <div className="fa-account-identity">{account.iconDataUrl ? <img src={account.iconDataUrl} alt="" loading="lazy" /> : <span className="fa-account-initial">{account.name[0]?.toUpperCase()}</span>}<span><h2 title={account.name}>{account.name}</h2><p title={account.riotId ?? undefined}>{account.riotId ?? "Riot ID unavailable"}</p></span></div>
        <div className="fa-account-meta"><span>Region<strong>{account.region ?? account.platform ?? "Unknown"}</strong></span><span>Session<strong>{switching ? "Switching…" : active ? "In use" : "Saved securely"}</strong></span></div>
        {/* The "Active session" badge already names the signed-in account, so its one action is the restart:
            switching to the signed-in account restarts the client with it (same as the tray flyout). */}
        {editing
          ? <button type="button" className="fa-account-grip" disabled={!canReorder}
            aria-label={`Reorder ${account.name}, position ${index + 1} of ${ids.length}`} aria-keyshortcuts="ArrowUp ArrowDown Alt+ArrowUp Alt+ArrowDown"
            onKeyDown={event => { event.stopPropagation(); onKey(event, account.id, true); }}><GripIcon size={20} /> Drag to reorder</button>
          : active
          ? <button className="fa-secondary fa-switch fa-restart" disabled={props.busy} onClick={() => (props.onRestart ?? (() => props.onSwitch(account.id)))()}>{props.closed ? "Open League" : "Restart client"}<RefreshIcon size={16} /></button>
          : <button className="fa-secondary fa-switch" disabled={props.busy} onClick={() => props.onSwitch(account.id)}>{switching ? "Signing in…" : "Switch account"}<SwitchIcon size={16} /></button>}
      </article>;
    })}<button className="fa-add-account" disabled={props.busy || editing} onClick={props.onAdd}><AddAccountIcon size={22} /><span className="fa-add-copy"><strong>{props.accounts.length ? "Add another account" : "Save your first account"}</strong><span>Sign in once through Riot Client.</span></span></button></div>
  </div>;
}
