import { Button } from "@/components/ui/button";

export type DeceivePrompt = "ask" | "defer" | "unsaved";

/** Shown under the Deceive switch when the change could not restart the client by itself. */
export function DeceiveRestartNotice({ prompt, onRestart, onDismiss }: {
  prompt: DeceivePrompt | null; onRestart: () => void; onDismiss: () => void;
}) {
  if (!prompt) return null;
  return <div className="deceive-notice" role="status">
    {prompt === "ask"
      ? <><p>Restart the client now to apply this? You are in champion select, so restarting means dodging and can cost LP.</p>
        <div><Button variant="outline" onClick={onRestart}>Restart client</Button><Button variant="ghost" onClick={onDismiss}>Not now</Button></div></>
      : prompt === "unsaved"
      ? <><p>Saved. Restart Riot Client to apply it. Save this account in Swapper and it restarts for you next time.</p>
        <div><Button variant="ghost" onClick={onDismiss}>Dismiss</Button></div></>
      : <><p>Saved. A game is running, so this applies after the game or at the next launch.</p>
        <div><Button variant="ghost" onClick={onDismiss}>Dismiss</Button></div></>}
  </div>;
}
