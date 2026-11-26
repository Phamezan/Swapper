// Animated Canvas 2D backdrop for Swapper's hero and closing CTA/footer band.
// Graphite Horizon Lattice: perspective dot-and-crosshair ridge in neutral graphite + #c5e39e crests.

export type BgSlot = "hero" | "closing";

interface SlotPerf {
  frames: number;
  avgMs: number;
  maxMs: number;
  lastMs: number;
  width: number;
  height: number;
  dpr: number;
  running: boolean;
}

interface BgPerfState {
  reducedMotion: boolean;
  phoneStatic: boolean;
  slots: Partial<Record<BgSlot, SlotPerf>>;
}

declare global {
  interface Window {
    __swapperBgPerf?: BgPerfState;
  }
}

interface SlotController {
  slot: BgSlot;
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  w: number;
  h: number;
  dpr: number;
  visible: boolean;
  mx: number;
  my: number;
  mStrength: number;
  rectTop: number;
  rectLeft: number;
  perfSamples: number[];
  perf: SlotPerf;
}

const SAGE = "197, 227, 158"; // #c5e39e
const GRAPHITE = "214, 217, 224"; // neutral graphite ink

function drawLattice(sc: SlotController, t: number, light: boolean): void {
  const { ctx, w, h, slot, mx, my, mStrength } = sc;
  ctx.clearRect(0, 0, w, h);

  const horizonY = slot === "hero" ? h * 0.29 : h * 0.34;
  const fieldDepth = h - horizonY + 40;

  // Faint sage crest glow resting just along the central ridge horizon.
  const crownX = w * (0.5 + Math.sin(t * 0.45) * 0.14);
  const crownY = horizonY + fieldDepth * 0.24;
  const crownR = Math.max(w * 0.38, 320);
  const crown = ctx.createRadialGradient(crownX, crownY, 0, crownX, crownY, crownR);
  crown.addColorStop(0, `rgba(${SAGE}, ${slot === "closing" ? 0.11 : 0.085})`);
  crown.addColorStop(0.55, `rgba(${SAGE}, 0.025)`);
  crown.addColorStop(1, "rgba(0, 0, 0, 0)");
  ctx.fillStyle = crown;
  ctx.fillRect(0, 0, w, h);

  const rows = light ? 18 : 30;
  const cols = light ? 38 : 74;

  // 5 batched fill buckets: 3 graphite depths + 2 sage crest tiers.
  const b0: number[] = [];
  const b1: number[] = [];
  const b2: number[] = [];
  const s1: number[] = [];
  const s2: number[] = [];

  const ridgeTraces: number[][] = [];
  const ptrRadiusSq = 185 * 185;

  for (let r = 0; r < rows; r++) {
    const zNorm = r / (rows - 1);
    const persp = 0.24 + Math.pow(zNorm, 1.48) * 0.96;
    const baseY = horizonY + Math.pow(zNorm, 1.38) * fieldDepth;
    const rowSpan = w * (1.06 + (persp - 0.5) * 0.42);
    const startX = (w - rowSpan) * 0.5;
    const dotSize = 1.2 + persp * 1.45;
    const recordTrace = !light && r >= 4 && r % 6 === 4;
    const tracePts: number[] | null = recordTrace ? [] : null;
    const horizonFade = Math.min(zNorm * 4.5, 1);

    for (let c = 0; c < cols; c++) {
      const u = c / (cols - 1);
      const x = startX + u * rowSpan;
      if (x < -8 || x > w + 8) continue;

      const w1 = Math.sin(u * 6.4 - zNorm * 3.0 - t * 1.05) * 0.48;
      const w2 = Math.cos(u * 4.4 + zNorm * 4.6 + t * 0.78) * 0.34;
      const w3 = Math.sin(u * 9.0 - t * 0.65) * Math.sin(zNorm * Math.PI) * 0.26;
      const ridgeEnvelope = (0.58 + 0.42 * Math.sin(u * Math.PI)) * horizonFade;
      let elev = (w1 + w2 + w3 + 0.12) * ridgeEnvelope;

      const lift = elev * (36 + persp * 34);
      let y = baseY - lift;

      let ptrBoost = 0;
      if (mStrength > 0.01) {
        const dx = x - mx;
        const dy = y - my;
        const dSq = dx * dx + dy * dy;
        if (dSq < ptrRadiusSq) {
          const factor = Math.exp(-dSq / (ptrRadiusSq * 0.28)) * mStrength;
          y -= factor * 22;
          elev += factor * 0.55;
          ptrBoost = factor;
        }
      }

      if (tracePts) tracePts.push(x, y);
      if (y < -6 || y > h + 6) continue;

      const edgeX = Math.min(u * 8, (1 - u) * 8, 1);
      if (edgeX < 0.08) continue;

      if (elev > 0.58 || ptrBoost > 0.48) {
        s2.push(x, y, dotSize);
      } else if (elev > 0.30 || ptrBoost > 0.24) {
        s1.push(x, y, dotSize);
      } else if (zNorm > 0.52) {
        b2.push(x, y, dotSize);
      } else if (zNorm > 0.20) {
        b1.push(x, y, dotSize);
      } else {
        b0.push(x, y, dotSize);
      }
    }

    if (tracePts && tracePts.length >= 4) ridgeTraces.push(tracePts);
  }

  if (ridgeTraces.length > 0) {
    ctx.strokeStyle = `rgba(${GRAPHITE}, 0.06)`;
    ctx.lineWidth = 0.85;
    ctx.beginPath();
    for (const pts of ridgeTraces) {
      ctx.moveTo(pts[0], pts[1]);
      for (let i = 2; i < pts.length; i += 2) {
        ctx.lineTo(pts[i], pts[i + 1]);
      }
    }
    ctx.stroke();
  }

  const flushRects = (buf: number[], color: string) => {
    if (buf.length === 0) return;
    ctx.fillStyle = color;
    for (let i = 0; i < buf.length; i += 3) {
      const sz = buf[i + 2];
      ctx.fillRect(buf[i] - sz * 0.5, buf[i + 1] - sz * 0.5, sz, sz);
    }
  };

  flushRects(b0, `rgba(${GRAPHITE}, 0.14)`);
  flushRects(b1, `rgba(${GRAPHITE}, 0.27)`);
  flushRects(b2, `rgba(${GRAPHITE}, 0.38)`);
  flushRects(s1, `rgba(${SAGE}, 0.52)`);
  flushRects(s2, `rgba(${SAGE}, 0.88)`);

  // Precision micro-crosshairs on the highest sage crest nodes.
  if (s2.length > 0) {
    ctx.strokeStyle = `rgba(${SAGE}, 0.42)`;
    ctx.lineWidth = 0.9;
    ctx.beginPath();
    for (let i = 0; i < s2.length; i += 9) {
      const cx = s2[i];
      const cy = s2[i + 1];
      const arm = 4.2;
      ctx.moveTo(cx - arm, cy);
      ctx.lineTo(cx + arm, cy);
      ctx.moveTo(cx, cy - arm);
      ctx.lineTo(cx, cy + arm);
    }
    ctx.stroke();
  }
}

export function initAmbientBackgrounds(): void {
  const canvases = Array.from(document.querySelectorAll<HTMLCanvasElement>("canvas[data-bg-slot]"));
  if (canvases.length === 0) return;

  const reducedMq = window.matchMedia("(prefers-reduced-motion: reduce)");
  const phoneMq = window.matchMedia("(max-width: 860px)");

  let reducedMotion = reducedMq.matches;
  let phoneStatic = phoneMq.matches;

  const perfState: BgPerfState = {
    reducedMotion,
    phoneStatic,
    slots: {},
  };
  window.__swapperBgPerf = perfState;

  const slots: SlotController[] = [];

  for (const canvas of canvases) {
    const ctx = canvas.getContext("2d", { alpha: true });
    if (!ctx) continue;
    const slot: BgSlot = canvas.dataset.bgSlot === "closing" ? "closing" : "hero";
    const perf: SlotPerf = {
      frames: 0,
      avgMs: 0,
      maxMs: 0,
      lastMs: 0,
      width: 0,
      height: 0,
      dpr: 1,
      running: false,
    };
    perfState.slots[slot] = perf;
    slots.push({
      slot,
      canvas,
      ctx,
      w: 0,
      h: 0,
      dpr: 1,
      visible: true,
      mx: 0,
      my: 0,
      mStrength: 0,
      rectTop: 0,
      rectLeft: 0,
      perfSamples: [],
      perf,
    });
  }

  if (slots.length === 0) return;

  let pointerActive = false;
  let pointerClientX = 0;
  let pointerClientY = 0;

  const refreshRects = () => {
    for (const sc of slots) {
      const rect = sc.canvas.getBoundingClientRect();
      sc.rectLeft = rect.left;
      sc.rectTop = rect.top;
    }
  };

  const sizeSlot = (sc: SlotController) => {
    const parent = sc.canvas.parentElement;
    const rect = (parent ?? sc.canvas).getBoundingClientRect();
    const w = Math.max(1, Math.round(rect.width));
    const h = Math.max(1, Math.round(rect.height));
    // Cap DPR gently on desktop (and 1x above 1600px wide) to keep idle GPU fill low on 1440p/4K displays.
    const maxDpr = phoneStatic || w > 1600 ? 1 : 1.5;
    const dpr = Math.min(window.devicePixelRatio || 1, maxDpr);

    if (w !== sc.w || h !== sc.h || dpr !== sc.dpr) {
      sc.w = w;
      sc.h = h;
      sc.dpr = dpr;
      sc.canvas.width = Math.round(w * dpr);
      sc.canvas.height = Math.round(h * dpr);
      sc.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      sc.perf.width = w;
      sc.perf.height = h;
      sc.perf.dpr = dpr;
    }
    sc.rectLeft = rect.left;
    sc.rectTop = rect.top;
  };

  const drawSlot = (sc: SlotController, t: number, light: boolean) => {
    if (sc.w <= 1 || sc.h <= 1) return;
    const t0 = performance.now();
    drawLattice(sc, t, light);
    const elapsed = performance.now() - t0;

    sc.perf.frames += 1;
    sc.perf.lastMs = Number(elapsed.toFixed(3));
    if (elapsed > sc.perf.maxMs) sc.perf.maxMs = Number(elapsed.toFixed(3));
    sc.perfSamples.push(elapsed);
    if (sc.perfSamples.length > 60) sc.perfSamples.shift();
    let sum = 0;
    for (let i = 0; i < sc.perfSamples.length; i++) sum += sc.perfSamples[i];
    sc.perf.avgMs = Number((sum / sc.perfSamples.length).toFixed(3));
  };

  let rafId = 0;
  let simTime = 4.2;
  let lastTs = 0;
  const FRAME_INTERVAL_MS = 32; // ~30 fps for calm ambient motion; halves idle GPU fill

  const isStaticMode = () => reducedMotion || phoneStatic;

  const stopLoop = () => {
    if (rafId !== 0) {
      cancelAnimationFrame(rafId);
      rafId = 0;
    }
    lastTs = 0;
    for (const sc of slots) sc.perf.running = false;
  };

  const syncLoop = () => {
    perfState.reducedMotion = reducedMotion;
    perfState.phoneStatic = phoneStatic;

    if (isStaticMode()) {
      stopLoop();
      for (const sc of slots) {
        sizeSlot(sc);
        sc.mStrength = 0;
        drawSlot(sc, 4.2, phoneStatic);
      }
      return;
    }

    const anyActive = !document.hidden && slots.some((sc) => sc.visible);
    if (!anyActive) {
      stopLoop();
      return;
    }

    if (rafId === 0) {
      lastTs = 0;
      rafId = requestAnimationFrame(tick);
    }
  };

  const tick = (ts: number) => {
    rafId = 0;
    if (document.hidden || isStaticMode()) {
      stopLoop();
      return;
    }

    if (lastTs !== 0 && ts - lastTs < FRAME_INTERVAL_MS) {
      rafId = requestAnimationFrame(tick);
      return;
    }
    if (lastTs === 0) lastTs = ts;
    const dt = Math.min((ts - lastTs) / 1000, 0.08);
    lastTs = ts;
    simTime += dt;

    let anyVisible = false;
    for (const sc of slots) {
      if (!sc.visible) {
        sc.perf.running = false;
        continue;
      }
      anyVisible = true;
      sc.perf.running = true;

      const localX = pointerClientX - sc.rectLeft;
      const localY = pointerClientY - sc.rectTop;
      const inBounds = pointerActive && localX >= -80 && localX <= sc.w + 80 && localY >= -80 && localY <= sc.h + 80;
      const targetStrength = inBounds ? 1 : 0;
      sc.mStrength += (targetStrength - sc.mStrength) * Math.min(1, dt * 6);
      if (inBounds) {
        if (sc.mStrength < 0.05) {
          sc.mx = localX;
          sc.my = localY;
        } else {
          sc.mx += (localX - sc.mx) * Math.min(1, dt * 10);
          sc.my += (localY - sc.my) * Math.min(1, dt * 10);
        }
      }

      drawSlot(sc, simTime, false);
    }

    if (anyVisible) {
      rafId = requestAnimationFrame(tick);
    } else {
      lastTs = 0;
    }
  };

  for (const sc of slots) {
    sizeSlot(sc);
    drawSlot(sc, simTime, phoneStatic);
  }

  if ("IntersectionObserver" in window) {
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const sc = slots.find((item) => item.canvas === entry.target);
          if (sc) sc.visible = entry.isIntersecting;
        }
        refreshRects();
        syncLoop();
      },
      { rootMargin: "0px", threshold: 0.01 },
    );
    for (const sc of slots) io.observe(sc.canvas);
  }

  if ("ResizeObserver" in window) {
    const ro = new ResizeObserver(() => {
      for (const sc of slots) {
        sizeSlot(sc);
        if (isStaticMode()) drawSlot(sc, 4.2, phoneStatic);
      }
    });
    for (const sc of slots) ro.observe(sc.canvas.parentElement ?? sc.canvas);
  }

  window.addEventListener(
    "pointermove",
    (e) => {
      pointerActive = true;
      pointerClientX = e.clientX;
      pointerClientY = e.clientY;
    },
    { passive: true },
  );
  window.addEventListener("pointerleave", () => {
    pointerActive = false;
  });
  window.addEventListener("blur", () => {
    pointerActive = false;
  });
  window.addEventListener("scroll", refreshRects, { passive: true });
  document.addEventListener("visibilitychange", syncLoop);

  const onMediaChange = () => {
    reducedMotion = reducedMq.matches;
    phoneStatic = phoneMq.matches;
    syncLoop();
  };
  reducedMq.addEventListener("change", onMediaChange);
  phoneMq.addEventListener("change", onMediaChange);

  syncLoop();
}
