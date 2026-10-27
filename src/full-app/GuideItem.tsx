import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cachedIcon, loadIcon } from "../runes/iconLoader";
import { Slot } from "./ui/Slot";

export function GuideItem({ id, size = 26, label }: { id: number; size?: number; label?: string }) {
  const key = `item:${id}`;
  const [known, setKnown] = useState(() => ({ id, ready: !!cachedIcon(key) }));
  useEffect(() => {
    if (id <= 0) return;
    let live = true;
    void loadIcon(key, () => invoke<string>("item_icon", { id }))
      .then(icon => { if (live) setKnown({ id, ready: !!icon }); })
      .catch(() => { if (live) setKnown({ id, ready: false }); });
    return () => { live = false; };
  }, [id, key]);
  return id > 0 && known.id === id && known.ready ? <Slot kind="item" id={id} size={size} label={label} /> : null;
}
