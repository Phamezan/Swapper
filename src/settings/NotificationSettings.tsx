import { useState } from "react";
import { Info } from "lucide-react";
import { Switch } from "@/components/ui/switch";

type Props = {
  notifications: boolean;
  readyCheck: boolean;
  busy: boolean;
  onNotificationsChange: (enabled: boolean) => void;
  onReadyCheckChange: (enabled: boolean) => void;
};

export function NotificationSettings({
  notifications,
  readyCheck,
  busy,
  onNotificationsChange,
  onReadyCheckChange,
}: Props) {
  const [openInfo, setOpenInfo] = useState<string | null>(null);
  const toggle = (key: string) => setOpenInfo(openInfo === key ? null : key);

  return (
    <>
      <div className="setting-row">
        <div className="setting-copy">
          <label htmlFor="notifications-switch">Notifications</label>
          <button className="info-button" aria-label="About Notifications" aria-expanded={openInfo === "notifications"} onClick={() => toggle("notifications")}><Info size={13} /></button>
          {openInfo === "notifications" && <p>Windows notification when a ready check starts and when draft begins.</p>}
        </div>
        <Switch id="notifications-switch" checked={notifications} disabled={busy} onCheckedChange={onNotificationsChange} aria-label="Notifications" />
      </div>

      <div className={`setting-row setting-dependent${notifications ? "" : " is-unavailable"}`}>
        <div className="setting-copy">
          <label htmlFor="ready-check-switch">Ready-check alerts</label>
          <button className="info-button" aria-label="About Ready-check alerts" aria-expanded={openInfo === "ready-check"} onClick={() => toggle("ready-check")}><Info size={13} /></button>
          {openInfo === "ready-check" && <p>Needs Notifications on. Alerts when a game is found so you can accept in time.</p>}
        </div>
        <Switch id="ready-check-switch" checked={readyCheck} disabled={busy || !notifications} onCheckedChange={onReadyCheckChange} aria-label="Ready-check alerts" />
      </div>
    </>
  );
}
