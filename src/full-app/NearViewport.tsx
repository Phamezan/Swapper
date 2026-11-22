import { useEffect, useRef, useState, type ReactNode } from "react";

/** Avoid fetching or decoding art until its card is near a scroll viewport. */
export function NearViewport({ children, className = "" }: { children: ReactNode; className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) { setSeen(true); observer.disconnect(); }
    }, { rootMargin: "120px" });
    if (ref.current) observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);
  return <div ref={ref} className={className}>{seen && children}</div>;
}

/** Calls `onNear` whenever this marker is near the viewport, so a long list can grow in steps as it is scrolled. */
export function NearEnd({ onNear }: { onNear: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const callback = useRef(onNear);
  callback.current = onNear;
  useEffect(() => {
    const observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) callback.current(); }, { rootMargin: "600px" });
    if (ref.current) observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);
  return <div ref={ref} aria-hidden />;
}

/** Adds `is-glow` the first time its content is fully in view, for a one-time highlight; never again while mounted,
 *  and never with reduced motion. */
export function GlowOnce({ children, className = "" }: { children: ReactNode; className?: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [glow, setGlow] = useState(false);
  useEffect(() => {
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) { setGlow(true); observer.disconnect(); }
    }, { threshold: 1 });
    if (ref.current) observer.observe(ref.current);
    return () => observer.disconnect();
  }, []);
  return <span ref={ref} className={`${className}${glow ? " is-glow" : ""}`}>{children}</span>;
}
