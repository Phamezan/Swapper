import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Notice } from "./ui/States";
import { Tooltip } from "./ui/Tooltip";

export function GuideSourceButton({ url, className, label, children }: { url: string; className?: string; label?: string; children: ReactNode }) {
  const [error, setError] = useState("");
  return <>
    <Tooltip label={label}><button type="button" className={className} aria-label={label} onClick={async () => {
      setError("");
      try { await invoke("open_guide_source", { url }); }
      catch (reason) { setError(String(reason)); }
    }}>{children}</button></Tooltip>
    {error && <Notice tone="error">Could not open source: {error}</Notice>}
  </>;
}
