import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Notice } from "./ui/States";

export function GuideSourceButton({ url, className, label, children }: { url: string; className?: string; label?: string; children: ReactNode }) {
  const [error, setError] = useState("");
  return <>
    <button type="button" className={className} aria-label={label} title={label} onClick={async () => {
      setError("");
      try { await invoke("open_guide_source", { url }); }
      catch (reason) { setError(String(reason)); }
    }}>{children}</button>
    {error && <Notice tone="error">Could not open source: {error}</Notice>}
  </>;
}
