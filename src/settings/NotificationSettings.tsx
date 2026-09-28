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
          <strong>Notifications</strong>
          <button className="info-button" aria-label="About Notifications" aria-expanded={openInfo === "notifications"} onClick={() => toggle("notifications")}><Info size={13} /></button>
          {openInfo === "notifications" && <p>Get a Windows notification when a ready check starts and when champion select begins. They use the default notification sound.</p>}
        </div>
        <Switch checked={notifications} disabled={busy} onCheckedChange={onNotificationsChange} aria-label="Notifications" />
      </div>

      <div className="setting-row">
        <div className="setting-copy">
          <strong>Ready-check alerts</strong>
          <button className="info-button" aria-label="About Ready-check alerts" aria-expanded={openInfo === "ready-check"} onClick={() => toggle("ready-check")}><Info size={13} /></button>
          {openInfo === "ready-check" && <p>Notify when a match is found so you can accept before the check expires. Turn off to keep only the champion select notification.</p>}
        </div>
        <Switch checked={readyCheck} disabled={busy || !notifications} onCheckedChange={onReadyCheckChange} aria-label="Ready-check alerts" />
      </div>
    </>
  );
}
