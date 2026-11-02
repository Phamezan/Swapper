import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleDot } from "lucide-react";
import { cachedIcon, loadIcon } from "../runes/iconLoader";
import type { MomentKind } from "./matchupGuide";
import { momentArt } from "./matchupVodModel";

/** Reuse the recap's objective_icon service; images resolve locally or as data URLs. */
export function MomentIcon({ kind, size = 20 }: { kind?: MomentKind; size?: number }) {
  const art = momentArt(kind);
  const [loaded, setLoaded] = useState<{ art: string; src: string } | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const src = art?.startsWith("/") ? art : art ? cachedIcon(`objective:${art}`) ?? (loaded?.art === art ? loaded.src : null) : null;
  useEffect(() => {
    if (!art || art.startsWith("/")) return;
    let live = true;
    void loadIcon(`objective:${art}`, () => invoke<string>("objective_icon", { name: art }))
      .then(src => { if (live) setLoaded({ art, src }); }).catch(() => {});
    return () => { live = false; };
  }, [art]);
  return <span className="mv-kind-icon" data-kind={kind ?? "none"} aria-hidden style={{ width: size, height: size }}>
    {src && failed !== src ? <img src={src} alt="" onError={() => setFailed(src)} /> : <CircleDot size={size} />}
  </span>;
}
