import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Info, LoaderCircle, Play, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import "./doctor.css";

type DoctorStatus = "ok" | "warn" | "error";
type DoctorCheck = {
  id: string;
  label: string;
  status: DoctorStatus;
  message: string;
  detail: string | null;
};

type DoctorState =
  | { phase: "idle" }
  | { phase: "running" }
  | { phase: "done"; checks: DoctorCheck[] }
  | { phase: "failed" };

/** The Settings health checks. Nothing runs until the user asks for it: the
 *  Run checks button probes everything once, per-check Retry refreshes one row,
 *  and Copy diagnostics hands the current results to the backend for the
 *  sanitized plain-text report. Results live as long as the component does. */
export function SwapperDoctor({ remoteTransport }: { remoteTransport: "lan" | "tailscale" }) {
  const native = isTauri();
  const [state, setState] = useState<DoctorState>({ phase: "idle" });
  const [retrying, setRetrying] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [openInfo, setOpenInfo] = useState(false);
  const copiedTimer = useRef<number | null>(null);

  const running = state.phase === "running";

  useEffect(() => cleanup, []);

  function cleanup() {
    if (copiedTimer.current !== null) {
      window.clearTimeout(copiedTimer.current);
      copiedTimer.current = null;
    }
  }

  async function runAll() {
    if (!native || running) return;
    cleanup();
    setState({ phase: "running" });
    try {
      const checks = await invoke<DoctorCheck[]>("run_doctor", { remoteTransport });
      setState({ phase: "done", checks });
    } catch {
      setState({ phase: "failed" });
    }
  }

  async function retry(id: string) {
    setRetrying(id);
    try {
      const check = await invoke<DoctorCheck>("run_doctor_check", { id, remoteTransport });
      setState((current) =>
        current.phase === "done"
          ? {
              phase: "done",
              checks: current.checks.map((existing) =>
                existing.id === id ? check : existing,
              ),
            }
          : current,
      );
    } catch {
      // Keep the previous result; the row still shows why it needs attention.
    } finally {
      setRetrying(null);
    }
  }

  async function copyReport() {
    if (state.phase !== "done") return;
    try {
      const report = await invoke<string>("doctor_report", { checks: state.checks });
      try {
        await navigator.clipboard.writeText(report);
      } catch {
        const field = document.createElement("textarea");
        field.value = report;
        field.style.position = "fixed";
        field.style.opacity = "0";
        document.body.appendChild(field);
        field.select();
        document.execCommand("copy");
        field.remove();
      }
      setCopied(true);
      copiedTimer.current = window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Copying is best effort; the checks themselves stay usable.
    }
  }

  if (!native) {
    return (
      <section className="doctor-section" aria-label="Swapper Doctor">
        <p className="support-text">Swapper Doctor runs in the Windows tray app. Start it with npm run tauri dev.</p>
      </section>
    );
  }

  return (
    <section className="doctor-section" aria-label="Swapper Doctor">
      <div className="setting-row doctor-heading">
        <div className="setting-copy">
          <strong>Swapper Doctor</strong>
          <button
            className="info-button"
            aria-label="About Swapper Doctor"
            aria-expanded={openInfo}
            onClick={() => setOpenInfo((open) => !open)}
          >
            <Info size={13} />
          </button>
          {openInfo && (
            <p>
              Checks the clients, providers and connections Swapper depends on. Nothing
              identifiable is collected, and copied diagnostics are sanitized.
            </p>
          )}
        </div>
        <div className="doctor-actions">
          <Button
            variant="outline"
            disabled={state.phase !== "done"}
            onClick={() => void copyReport()}
          >
            {copied ? "Copied" : "Copy diagnostics"}
          </Button>
          <Button variant="outline" disabled={running} onClick={() => void runAll()}>
            {running ? <LoaderCircle className="spin" size={15} /> : <Play size={14} />}
            {running ? "Running…" : "Run checks"}
          </Button>
        </div>
      </div>

      {state.phase === "idle" && (
        <div className="doctor-note">
          Run checks to test the clients, providers and connections Swapper uses.
        </div>
      )}
      {running && (
        <div className="doctor-note"><LoaderCircle className="spin" size={13} /> Running checks…</div>
      )}
      {state.phase === "failed" && (
        <div className="doctor-note">The checks could not run. Run them again.</div>
      )}
      {state.phase === "done" && (
        <div className="doctor-list">
          {state.checks.map((check) => (
            <div className="doctor-check" key={check.id}>
              <span className={`doctor-dot is-${check.status}`} aria-hidden="true" />
              <div className="doctor-text">
                <strong>{check.label}</strong>
                <small>
                  {check.message}
                  {check.detail ? ` · ${check.detail}` : ""}
                </small>
              </div>
              {check.status !== "ok" && (
                <button
                  className="icon-button"
                  aria-label={`Retry ${check.label}`}
                  disabled={retrying !== null}
                  onClick={() => void retry(check.id)}
                >
                  {retrying === check.id ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
