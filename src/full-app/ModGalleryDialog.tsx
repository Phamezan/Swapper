import { useEffect, useState, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Dialog } from "radix-ui";
import { Download, Heart, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip } from "./ui/Tooltip";
import { EmptyState, Notice, Skeleton } from "./ui/States";
import { BackIcon, ForwardIcon } from "./ui/icons";
import { fmtCompact, fmtCount } from "./ui/format";
import { categoryLabel, formatModDate, type RfDetail, type RfMod } from "./customModsModel";

/** Popup with a RuneForge mod's preview images; `footer` carries the card's Install / Open buttons. */
export function ModGalleryDialog({ mod, footer, onClose }: { mod: RfMod; footer: ReactNode; onClose: () => void }) {
  const [detail, setDetail] = useState<RfDetail | null>(null);
  const [failed, setFailed] = useState(false);
  const [index, setIndex] = useState(0);
  const [images, setImages] = useState<Record<string, string>>({});
  const native = isTauri();

  useEffect(() => {
    if (!native) return;
    let live = true;
    invoke<RfDetail>("runeforge_mod", { slug: mod.slug })
      .then((d) => { if (live) setDetail(d); })
      .catch(() => { if (live) setFailed(true); });
    return () => { live = false; };
  }, [mod.slug, native]);

  const gallery = detail?.gallery ?? (mod.thumbnailUrl ? [mod.thumbnailUrl] : []);
  const count = gallery.length;
  const current = Math.min(index, Math.max(count - 1, 0));
  const step = (by: number) => count > 1 && setIndex((current + by + count) % count);

  // Current image first, then its neighbours so the arrows feel instant.
  useEffect(() => {
    if (!native) return;
    let live = true;
    for (const at of [current, current + 1, current - 1]) {
      const url = gallery[(at + count) % count];
      if (!url || images[url]) continue;
      invoke<string>("runeforge_thumbnail", { url })
        .then((src) => { if (live) setImages((prev) => ({ ...prev, [url]: src })); })
        .catch(() => undefined);
    }
    return () => { live = false; };
  }, [current, detail]);

  const shownUrl = gallery[current];
  const src = native ? images[shownUrl] : shownUrl;
  const date = formatModDate(mod.updatedAt ?? mod.publishedAt);
  const kind = [categoryLabel(mod.category), ...mod.champions.slice(0, 3)].join(" · ");

  return (
    <Dialog.Root open onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="cm-dialog-overlay" />
        <Dialog.Content
          className="cm-dialog"
          aria-describedby={undefined}
          onKeyDown={(e) => {
            if (e.key === "ArrowLeft") { e.preventDefault(); step(-1); }
            else if (e.key === "ArrowRight") { e.preventDefault(); step(1); }
          }}
        >
          <Dialog.Title className="cm-dialog-title">{mod.name}</Dialog.Title>
          <Dialog.Close asChild>
            <Button variant="ghost" size="icon-sm" className="cm-dialog-close" aria-label="Close"><X aria-hidden /></Button>
          </Dialog.Close>
          {native && !detail && !failed ? (
            <Skeleton label="Loading previews" rows={1} height={320} />
          ) : failed ? (
            <Notice tone="error" action={<Button variant="outline" onClick={() => invoke("runeforge_open", { url: mod.pageUrl }).catch(() => undefined)}>Open on RuneForge</Button>}>
              Couldn&apos;t load this mod&apos;s previews.
            </Notice>
          ) : count === 0 ? (
            <EmptyState title="No previews">This mod has no preview images.</EmptyState>
          ) : (
            <div className="cm-stage">
              <div className="cm-shot" aria-live="polite">
                {src ? <img src={src} alt={`${mod.name} preview ${current + 1} of ${count}`} decoding="async" /> : <span className="cm-shot-wait" role="status" aria-label="Loading image" />}
              </div>
              {count > 1 && <>
                <Tooltip label="Previous"><Button variant="outline" size="icon" className="cm-nav is-prev" aria-label="Previous image" onClick={() => step(-1)}><BackIcon aria-hidden /></Button></Tooltip>
                <Tooltip label="Next"><Button variant="outline" size="icon" className="cm-nav is-next" aria-label="Next image" onClick={() => step(1)}><ForwardIcon aria-hidden /></Button></Tooltip>
                <div className="cm-dots" role="group" aria-label="Preview images">
                  <span className="cm-count">{current + 1} / {count}</span>
                  {gallery.map((url, i) => (
                    <button key={url} type="button" className={i === current ? "on" : ""} aria-label={`Show image ${i + 1}`} aria-current={i === current} onClick={() => setIndex(i)} />
                  ))}
                </div>
              </>}
            </div>
          )}
          <div className="cm-dialog-info">
            <div className="cm-dialog-copy">
              <strong>{mod.name}</strong>
              <small>By {mod.author || "Unknown author"}</small>
              <small>{kind}</small>
              <span className="cm-stats">
                <span title={`${fmtCount(mod.downloadCount)} downloads`}><Download size={14} aria-hidden /><span className="cm-sr">Downloads: </span>{fmtCompact(mod.downloadCount)}</span>
                {mod.likeCount > 0 && <span title={mod.likeCount === 1 ? "1 like" : `${fmtCount(mod.likeCount)} likes`}><Heart size={14} aria-hidden /><span className="cm-sr">Likes: </span>{fmtCompact(mod.likeCount)}</span>}
                {date && <span><span className="cm-sr">Updated </span>{date}</span>}
              </span>
            </div>
            <div className="cm-dialog-actions">{footer}</div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
