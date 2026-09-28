import { useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Check, Link2, LoaderCircle } from "lucide-react";

type Props = {
  accountId: string;
  accountName: string;
  disabled?: boolean;
  onError: (reason: unknown) => void;
};

// Creates a `swapper://switch/<internal account id>` internet shortcut on the
// Windows Desktop; double-clicking it switches to this account.
export function CreateShortcutButton({ accountId, accountName, disabled, onError }: Props) {
  const [phase, setPhase] = useState<"idle" | "busy" | "done">("idle");

  async function create() {
    if (!isTauri()) {
      onError("Account actions are available in the Windows tray app. Start it with npm run tauri dev.");
      return;
    }
    setPhase("busy");
    try {
      await invoke("create_account_shortcut", { id: accountId });
      setPhase("done");
      window.setTimeout(() => setPhase("idle"), 1500);
    } catch (reason) {
      setPhase("idle");
      onError(reason);
    }
  }

  return (
    <button
      className="icon-button"
      aria-label={`Create a desktop shortcut for ${accountName}`}
      title="Create desktop shortcut"
      disabled={disabled || phase === "busy"}
      onClick={() => void create()}
    >
      {phase === "busy" ? <LoaderCircle className="spin" size={16} /> : phase === "done" ? <Check size={16} /> : <Link2 size={16} />}
    </button>
  );
}
