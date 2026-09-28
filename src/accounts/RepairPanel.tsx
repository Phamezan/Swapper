import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LoaderCircle, RefreshCw, Wrench } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Account, AppState, DetectOutcome } from "../App";

const wait = (ms: number) =>
  new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });

type RepairPanelProps = {
  account: Account | undefined;
  onDone: (next: AppState) => void;
  onCancel: () => void;
};

type RepairPhase =
  | { kind: "waiting"; reason: "noClient" | "signingIn" }
  | { kind: "saving" }
  | { kind: "failed"; problem: string };

// Repair flow for a saved account whose session could not be restored:
// the user signs in to that account in Riot Client, Swapper detects the
// identity, and the backend replaces the saved session only when the PUUID
// matches. A different sign-in keeps every saved session untouched.
export function RepairPanel({ account, onDone, onCancel }: RepairPanelProps) {
  const [phase, setPhase] = useState<RepairPhase>({ kind: "waiting", reason: "noClient" });
  const [attempt, setAttempt] = useState(0);
  const name = account?.name ?? "the account";

  useEffect(() => {
    if (account === undefined) return;
    let cancelled = false;
    (async () => {
      while (!cancelled) {
        let outcome: DetectOutcome;
        try {
          outcome = await invoke<DetectOutcome>("detect_account_identity");
        } catch {
          outcome = { state: "unavailable" };
        }
        if (cancelled) return;
        if (outcome.state === "unavailable") {
          setPhase({
            kind: "failed",
            problem: "Could not read account identity. Make sure Riot Client is open and signed in.",
          });
          return;
        }
        if (outcome.state === "identified") {
          setPhase({ kind: "saving" });
          try {
            const next = await invoke<AppState>("complete_repair", { id: account.id });
            if (!cancelled) onDone(next);
          } catch (reason) {
            if (!cancelled) setPhase({ kind: "failed", problem: String(reason) });
          }
          return;
        }
        setPhase({
          kind: "waiting",
          reason: outcome.state === "notRunning" ? "noClient" : "signingIn",
        });
        await wait(2000);
      }
    })();
    return () => {
      cancelled = true;
    };
    // The poll restarts only on retry; sign-in progress updates stay inside
    // the running loop so an in-flight save is never cancelled by a rerender.
    // Callback props are stable in practice and safe to omit, like the
    // add-flow detection loop in App.tsx.
  }, [attempt, account]);

  const retry = () => {
    setPhase({ kind: "waiting", reason: "signingIn" });
    setAttempt((round) => round + 1);
  };

  return (
    <div className="form-content add-flow">
      <section className="add-stage">
        <span className="step-number"><Wrench size={13} /></span>
        {phase.kind === "failed" ? <>
          <h2>Could not repair the account</h2>
          <p>{phase.problem}</p>
          <Button className="primary-action" onClick={retry}><RefreshCw size={16} /> Try Again</Button>
          <button className="text-action" onClick={onCancel}>Back to accounts</button>
        </> : phase.kind === "saving" ? <>
        <h2>Replacing the saved session…</h2>
        <p>Verifying the signed-in account and saving a fresh session for {name}. The account stays in place — nothing is duplicated or removed.</p>
          <div className="detect-status"><LoaderCircle className="spin" size={14} /> Working…</div>
          <button className="text-action" onClick={onCancel}>Cancel</button>
        </> : phase.reason === "noClient" ? <>
          <h2>Open Riot Client</h2>
          <p>Sign in to {name} with “Stay signed in”. Swapper detects the sign-in and replaces the account’s saved session automatically.</p>
          <div className="detect-status"><LoaderCircle className="spin" size={14} /> Looking for Riot Client…</div>
          <button className="text-action" onClick={onCancel}>Cancel</button>
        </> : <>
          <h2>Waiting for the {name} sign-in…</h2>
          <p>Keep Riot Client open while you finish signing in to {name}. If a different account is signed in, sign out first — Swapper only replaces the session when the sign-in matches.</p>
          <div className="detect-status"><LoaderCircle className="spin" size={14} /> Watching for your account…</div>
          <button className="text-action" onClick={onCancel}>Cancel</button>
        </>}
      </section>
    </div>
  );
}
