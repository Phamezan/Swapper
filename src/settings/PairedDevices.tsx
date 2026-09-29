import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, Pencil, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

type PairedDevice = {
  id: string;
  name: string;
  pairedAt: number;
  lastSeen: number;
  connected: boolean;
};

const REFRESH_INTERVAL_MS = 5000;

function formatRelative(seconds: number) {
  const elapsed = Math.max(0, Math.floor(Date.now() / 1000) - seconds);
  if (elapsed < 60) return "just now";
  const minutes = Math.floor(elapsed / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} d ago`;
  return new Date(seconds * 1000).toLocaleDateString();
}

function revokeMessage(reason: unknown) {
  return typeof reason === "string" ? reason : "Could not update paired devices.";
}

type Props = {
  /** Unix time of the last LAN address change; devices unseen since are stale. */
  staleSince?: number | null;
};

export function PairedDevices({ staleSince = null }: Props) {
  const native = isTauri();
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [pendingRevoke, setPendingRevoke] = useState<string | null>(null);
  const [pendingRemoveStale, setPendingRemoveStale] = useState(false);

  useEffect(() => {
    if (!native) return;
    let live = true;
    const load = () => {
      invoke<PairedDevice[]>("list_paired_lan_devices")
        .then((next) => {
          if (!live) return;
          setDevices(next);
          setError(null);
        })
        .catch((reason) => {
          if (live) setError(revokeMessage(reason));
        });
    };
    load();
    const timer = window.setInterval(load, REFRESH_INTERVAL_MS);
    const unlisten = listen("lan_devices_changed", load);
    return () => {
      live = false;
      window.clearInterval(timer);
      unlisten.then((fn) => fn());
    };
  }, [native]);

  if (!native) return null;

  async function revoke(id: string) {
    setPendingRevoke(null);
    try {
      await invoke("revoke_lan_device", { id });
      setDevices((prev) => prev.filter((device) => device.id !== id));
      setError(null);
    } catch (reason) {
      setError(revokeMessage(reason));
    }
  }

  async function rename(id: string) {
    const name = renameValue.trim();
    if (!name) return;
    try {
      const stored = await invoke<string>("rename_lan_device", { id, name });
      setDevices((prev) =>
        prev.map((device) => (device.id === id ? { ...device, name: stored } : device)),
      );
      setRenamingId(null);
      setError(null);
    } catch (reason) {
      setError(revokeMessage(reason));
    }
  }

  async function removeStale() {
    setPendingRemoveStale(false);
    try {
      await invoke<number>("remove_stale_lan_devices");
      setDevices((prev) =>
        staleSince ? prev.filter((device) => device.lastSeen >= staleSince) : prev,
      );
      setError(null);
    } catch (reason) {
      setError(revokeMessage(reason));
    }
  }

  // Devices that never came back after the PC's LAN address changed.
  const stale = staleSince ? devices.filter((device) => device.lastSeen < staleSince) : [];

  return (
    <div className="paired-devices" aria-label="Paired devices">
      <div className="paired-devices-head">
        <strong>Paired devices</strong>
        <span>{devices.length ? `${devices.length} paired` : "None yet"}</span>
      </div>
      {error && <p className="remote-transport-message">{error}</p>}
      {!error && devices.length === 0 && (
        <p className="remote-transport-message">
          Show the QR code and scan it on a phone to pair a device.
        </p>
      )}
      {devices.map((device) => (
        <div className="paired-device" key={device.id}>
          <span className={`remote-status-dot ${device.connected ? "is-good" : "is-bad"}`} aria-hidden="true" />
          <div className="paired-device-copy">
            {renamingId === device.id ? (
              <Input
                autoFocus
                maxLength={32}
                value={renameValue}
                aria-label={`Rename ${device.name}`}
                onChange={(event) => setRenameValue(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void rename(device.id);
                  if (event.key === "Escape") setRenamingId(null);
                }}
              />
            ) : (
              <strong>{device.name}</strong>
            )}
            <small>
              {device.connected ? "Connected" : "Offline"} · Last seen {formatRelative(device.lastSeen)} · Paired {formatRelative(device.pairedAt)}
            </small>
          </div>
          <div className="paired-device-actions">
            {renamingId === device.id ? (
              <>
                <button type="button" className="icon-button" aria-label={`Save name for ${device.name}`} onClick={() => void rename(device.id)}><Check size={14} /></button>
                <button type="button" className="icon-button" aria-label="Cancel rename" onClick={() => setRenamingId(null)}><X size={14} /></button>
              </>
            ) : pendingRevoke === device.id ? (
              <>
                <Button variant="outline" className="paired-revoke-confirm" onClick={() => void revoke(device.id)}>Revoke</Button>
                <button type="button" className="icon-button" aria-label="Cancel revoke" onClick={() => setPendingRevoke(null)}><X size={14} /></button>
              </>
            ) : (
              <>
                <button type="button" className="icon-button" aria-label={`Rename ${device.name}`} onClick={() => { setPendingRevoke(null); setRenameValue(device.name); setRenamingId(device.id); }}><Pencil size={14} /></button>
                <button type="button" className="icon-button danger-icon" aria-label={`Revoke ${device.name}`} onClick={() => setPendingRevoke(device.id)}><Trash2 size={14} /></button>
              </>
            )}
          </div>
        </div>
      ))}
      {stale.length > 0 && (
        <div className="paired-stale">
          {pendingRemoveStale ? (
            <>
              <span>Remove {stale.length} device{stale.length === 1 ? "" : "s"} not seen since the address change?</span>
              <Button variant="outline" className="paired-revoke-confirm" onClick={() => void removeStale()}>Remove</Button>
              <button type="button" className="icon-button" aria-label="Cancel removing old devices" onClick={() => setPendingRemoveStale(false)}><X size={14} /></button>
            </>
          ) : (
            <button type="button" className="paired-stale-action" onClick={() => setPendingRemoveStale(true)}>Remove old devices</button>
          )}
        </div>
      )}
    </div>
  );
}

export function PairingNotice() {
  const native = isTauri();
  const [notice, setNotice] = useState<string | null>(null);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    if (!native) return;
    const unlisten = listen<{ name?: string }>("lan_device_paired", ({ payload }) => {
      setNotice(`New device paired: ${payload?.name || "Unknown device"}`);
      if (timer.current) window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setNotice(null), 6000);
    });
    return () => {
      unlisten.then((fn) => fn());
      if (timer.current) window.clearTimeout(timer.current);
    };
  }, [native]);

  if (!notice) return null;

  return (
    <div className="pairing-notice" role="status">
      <span>{notice}</span>
      <button type="button" aria-label="Dismiss pairing notice" onClick={() => setNotice(null)}><X size={13} /></button>
    </div>
  );
}
