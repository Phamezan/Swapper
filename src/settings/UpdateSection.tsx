import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CircleAlert, Download, Info, LoaderCircle, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import "./update.css";

type UpdateStateKind =
  | "notConfigured"
  | "checking"
  | "available"
  | "upToDate"
  | "installing"
  | "failed";

export type UpdateSnapshot = {
  state: UpdateStateKind;
  currentVersion: string;
  availableVersion: string | null;
  notes: string | null;
  message: string | null;
  lastCheck: number | null;
};

/** The updater state as the backend publishes it: one snapshot pulled on
 *  mount, then refreshed by every update_state event. */
function useUpdateSnapshot(): UpdateSnapshot | null {
  const [snapshot, setSnapshot] = useState<UpdateSnapshot | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    invoke<UpdateSnapshot>("update_status")
      .then((snapshot) => {
        if (live) setSnapshot(snapshot);
      })
      .catch(() => {});
    const unlisten = listen<UpdateSnapshot>("update_state", ({ payload }) => {
      setSnapshot(payload);
    });
    return () => {
      live = false;
      void unlisten.then((off) => off());
    };
  }, []);
  return snapshot;
}

function checkForUpdates() {
  return invoke("update_check_now");
}

function installUpdate() {
  return invoke("update_install");
}

/** The Settings updates section: current version, release notes for an
 *  announced update, and the Update & Restart action. Failures keep the
 *  section usable — the message explains and Check for updates retries. */
export function UpdateSection() {
  const native = isTauri();
  const snapshot = useUpdateSnapshot();
  const [busy, setBusy] = useState(false);
  const [openInfo, setOpenInfo] = useState(false);

  if (!native || !snapshot) return null;

  const waiting = snapshot.state === "checking" || snapshot.state === "installing";

  async function run(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    try {
      // The backend publishes the new state as an event, including failures.
      await action();
    } catch {
      // The failed state carries the short message.
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="update-section" aria-label="Updates">
      <div className="setting-row">
        <div className="setting-copy">
          <strong>Updates</strong>
          <button
            className="info-button"
            aria-label="About Updates"
            aria-expanded={openInfo}
            onClick={() => setOpenInfo((open) => !open)}
          >
            <Info size={13} />
          </button>
          {openInfo && (
            <p>
              Swapper checks GitHub for new releases after startup and about every
              six hours. Updating restarts Swapper; saved accounts stay.
            </p>
          )}
        </div>
        <div className="update-actions">
          {snapshot.state === "available" && (
            <Button variant="outline" disabled={busy} onClick={() => void run(installUpdate)}>
              {busy ? <LoaderCircle className="spin" size={14} /> : <Download size={14} />}
              Update &amp; Restart
            </Button>
          )}
          {!waiting && snapshot.state !== "notConfigured" && (
            <Button variant="outline" disabled={busy} onClick={() => void run(checkForUpdates)}>
              {busy ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={13} />}
              {busy ? "Checking…" : "Check for updates"}
            </Button>
          )}
        </div>
      </div>

      {snapshot.state === "notConfigured" && (
        <p className="support-text">
          Swapper {snapshot.currentVersion} · update checking is not configured in this build.
        </p>
      )}
      {snapshot.state === "checking" && (
        <div className="setting-status"><LoaderCircle className="spin" size={13} /> Checking for updates…</div>
      )}
      {snapshot.state === "upToDate" && (
        <div className="setting-status"><span className="update-dot is-good" />Swapper {snapshot.currentVersion} · up to date</div>
      )}
      {snapshot.state === "available" && (
        <div className="update-offer">
          <div className="setting-status">
            <span className="update-dot is-good" />Update available · v{snapshot.availableVersion}
          </div>
          {snapshot.notes && <p className="update-notes">{snapshot.notes}</p>}
          <p className="update-notes">Installed: Swapper {snapshot.currentVersion}</p>
        </div>
      )}
      {snapshot.state === "installing" && (
        <div className="setting-status"><LoaderCircle className="spin" size={13} /> Downloading and installing the update. Swapper restarts when it finishes…</div>
      )}
      {snapshot.state === "failed" && (
        <div className="setting-status update-failed">
          <CircleAlert size={13} />
          <span>{snapshot.message ?? "The last update check failed."}</span>
        </div>
      )}
    </section>
  );
}

/** The slim banner on the accounts view, shown only while an update is
 *  waiting. The text opens Settings for the details; the button installs. */
export function UpdateBanner({ onOpenSettings }: { onOpenSettings: () => void }) {
  const native = isTauri();
  const snapshot = useUpdateSnapshot();
  const [busy, setBusy] = useState(false);

  if (!native || !snapshot || snapshot.state !== "available" || !snapshot.availableVersion) {
    return null;
  }

  async function install() {
    if (busy) return;
    setBusy(true);
    try {
      await installUpdate();
    } catch {
      // The settings section shows what went wrong.
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="update-banner" role="status">
      <button
        className="update-banner-text"
        aria-label={`Update available, version ${snapshot.availableVersion}. Open Settings`}
        onClick={onOpenSettings}
      >
        <Download size={14} />
        <span>Update available · v{snapshot.availableVersion}</span>
      </button>
      <button className="update-banner-action" disabled={busy} onClick={() => void install()}>
        {busy ? <LoaderCircle className="spin" size={13} /> : "Update & Restart"}
      </button>
    </div>
  );
}
