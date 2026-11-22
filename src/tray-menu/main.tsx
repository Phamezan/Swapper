import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Pencil, Power, Trash2 } from "lucide-react";
import { AddAccountIcon, DashboardIcon, RefreshIcon, SettingsIcon } from "../full-app/ui/icons";
import "../theme.css";
import "./tray-menu.css";

type Item = { id: string; label: string; icon: ReactNode; danger?: boolean };

const ICON = 16;
const OPEN: Item = { id: "open", label: "Open Swapper", icon: <DashboardIcon size={ICON} /> };
const ITEMS: Item[] = [
  { id: "add", label: "Add Account", icon: <AddAccountIcon size={ICON} /> },
  { id: "edit", label: "Edit Account", icon: <Pencil size={ICON} strokeWidth={1.75} /> },
  { id: "remove", label: "Remove Account", icon: <Trash2 size={ICON} strokeWidth={1.75} /> },
  { id: "settings", label: "Settings", icon: <SettingsIcon size={ICON} /> },
  { id: "restart", label: "Restart client", icon: <RefreshIcon size={ICON} /> },
];
const EXIT: Item = { id: "exit", label: "Exit", icon: <Power size={ICON} strokeWidth={1.75} />, danger: true };

function TrayMenu() {
  const [fullApp, setFullApp] = useState(false);
  const [opened, setOpened] = useState(0);
  const menu = useRef<HTMLDivElement>(null);
  const buttons = useCallback(() => Array.from(menu.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? []), []);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke<boolean>("tray_menu_state").then(setFullApp);
    const shown = listen<boolean>("tray-menu-show", (event) => {
      setFullApp(event.payload);
      setOpened((n) => n + 1);
    });
    return () => void shown.then((off) => off());
  }, []);

  // Each time the menu opens, put keyboard focus on the first item.
  useEffect(() => {
    buttons()[0]?.focus();
  }, [opened, fullApp, buttons]);

  const choose = (id: string) => void invoke("tray_menu_action", { id });
  const onKeyDown = (event: KeyboardEvent) => {
    const all = buttons();
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    const move = (to: number) => { event.preventDefault(); all[(to + all.length) % all.length]?.focus(); };
    if (event.key === "ArrowDown") move(at + 1);
    else if (event.key === "ArrowUp") move(at < 0 ? -1 : at - 1);
    else if (event.key === "Home") move(0);
    else if (event.key === "End") move(-1);
    else if (event.key === "Escape") void invoke("tray_menu_hide");
  };

  const row = (item: Item) => (
    <button key={item.id} type="button" role="menuitem" className={`tm-item${item.danger ? " danger" : ""}`} onClick={() => choose(item.id)}>
      {item.icon}<span>{item.label}</span>
    </button>
  );

  return (
    <div className="tm-frame">
      <div key={opened} ref={menu} className="tm-menu" role="menu" aria-label="Swapper" onKeyDown={onKeyDown}>
        {fullApp && row(OPEN)}
        {ITEMS.map(row)}
        <div className="tm-separator" role="separator" />
        {row(EXIT)}
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<TrayMenu />);
