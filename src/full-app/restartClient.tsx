import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Dialog } from "radix-ui";

type Ask = "ask" | "defer" | "unsaved";

const COPY: Record<Ask, { title: string; body: string }> = {
  defer: { title: "Can't restart during a game", body: "Restart the client after the game ends." },
  ask: { title: "Restart during champion select?", body: "Restarting now dodges champion select and can cost LP." },
  unsaved: { title: "Save this account first", body: "Swapper can't sign you back in until the account is saved. Save it in Accounts, then restart." },
};

/** The one way every "Restart client" entry point starts: the backend plan decides whether to restart, ask or refuse. */
export function useRestartClient(restart: () => boolean) {
  const [ask, setAsk] = useState<Ask | null>(null);
  const restartRef = useRef(restart);
  restartRef.current = restart;
  const request = useCallback(async () => {
    // An unreadable plan asks instead of restarting on a guess.
    const plan = await invoke<string>("restart_plan").catch(() => "ask");
    if (plan === "defer") setAsk("defer");
    else if (plan === "ask") setAsk("ask");
    else if (!restartRef.current()) setAsk("unsaved");
  }, []);
  const confirm = () => { setAsk(null); if (!restartRef.current()) setAsk("unsaved"); };
  const dialog = <Dialog.Root open={ask !== null} onOpenChange={open => { if (!open) setAsk(null); }}>
    {ask && <Dialog.Portal container={document.querySelector(".fa-frame")}>
      <Dialog.Overlay className="restart-scrim" />
      <Dialog.Content className="restart-dialog ui-card is-raised" aria-describedby="restart-body">
        <Dialog.Title>{COPY[ask].title}</Dialog.Title>
        <Dialog.Description id="restart-body">{COPY[ask].body}</Dialog.Description>
        <div className="restart-actions">
          {ask === "ask" && <button type="button" className="fa-primary is-compact" onClick={confirm}>Restart anyway</button>}
          <Dialog.Close className="fa-secondary is-compact">{ask === "ask" ? "Cancel" : "OK"}</Dialog.Close>
        </div>
      </Dialog.Content>
    </Dialog.Portal>}
  </Dialog.Root>;
  return { request, dialog };
}
