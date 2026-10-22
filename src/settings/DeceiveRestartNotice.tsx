import { Button } from "@/components/ui/button";

export type DeceivePrompt = "ask" | "defer";

/** Shown under the Deceive switch when the change could not restart the client by itself. */
export function DeceiveRestartNotice({ prompt, onRestart, onDismiss }: {
  prompt: DeceivePrompt | null; onRestart: () => void; onDismiss: () => void;
}) {
  if (!prompt) return null;
  return <div className="deceive-notice" role="status">
    {prompt === "ask"
      ? <><p>Restart the client now to apply this? You are in champion select, so restarting means dodging and can cost LP.</p>
        <div><Button variant="outline" onClick={onRestart}>Restart client</Button><Button variant="ghost" onClick={onDismiss}>Not now</Button></div></>
      : <><p>Saved. A game is running, so this applies after the game or at the next launch.</p>
        <div><Button variant="ghost" onClick={onDismiss}>Dismiss</Button></div></>}
  </div>;
}
