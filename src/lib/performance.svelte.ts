/**
 * Display refresh measurement, frame pacing, and measured render stats.
 *
 * The visualization canvas draws on demand, so its cost scales with the display's
 * callback rate. This module separates three things the UI must never conflate:
 * - the panel/compositor rate observed through `requestAnimationFrame` (`displayHz`),
 * - the user's frame-rate target (`FrameTarget`),
 * - the rate actually rendered after unsupported targets fall back (`effectiveHz`).
 *
 * Animation speed stays real-time: `createFramePacer` passes the elapsed time
 * between drawn frames to the renderer, so a 30 fps cap advances animation by
 * ~33 ms per frame instead of playing it in slow motion.
 *
 * Browser APIs are guarded so importing this module never touches `window`,
 * `document`, or `localStorage` at module scope.
 */

export type FrameTarget = "auto" | 30 | 60 | 90 | 120 | 144 | 165 | 240;

export const FRAME_TARGETS: readonly FrameTarget[] = ["auto", 30, 60, 90, 120, 144, 165, 240];

/** `vsync` = no frames are skipped (target meets the display estimate); `fixed` = capped. */
export type PacingMode = "vsync" | "fixed" | "idle";
export type EstimateConfidence = "high" | "medium" | "low";

export const TARGET_STORAGE_KEY = "veil.performance.target.v1";
export const OVERLAY_STORAGE_KEY = "veil.performance.overlay.v1";

/** Shown next to the display estimate; the measurement cannot see the physical panel. */
export const DISPLAY_LIMITATION =
  "Measured through requestAnimationFrame. WebKitGTK may cap callbacks at ~60 Hz even on a faster panel, so this is the render-loop rate, not a panel query.";

export const AUTO_TARGET_POLICY =
  "Auto follows the display estimate but caps the CPU-drawn canvas at 60 fps. If measured draw cost stays above 13 ms it drops to 30 fps, then recovers after cost falls below 7 ms. Animation speed remains real-time at every cap.";

const DISPLAY_DETECTION_MS = 1000;
const DISPLAY_DETECTION_TIMEOUT_MS = 1500;
const STATS_INTERVAL_MS = 250;
const CONFIG_REFRESH_MS = 250;
/**
 * Treat callbacks this close before a slot boundary as due. Without it, floating-point
 * rounding plus sub-millisecond vsync jitter can make a 60 Hz callback on a 60 Hz target
 * miss every boundary (measured ~58.6 fps instead of 60). 0.5 ms is far below any frame.
 */
const SLOT_TOLERANCE_MS = 0.5;
const AUTO_HOT_COST_MS = 13;
const AUTO_COOL_COST_MS = 7;
const AUTO_HOT_PUBLICATIONS = 3;
const AUTO_COOL_PUBLICATIONS = 8;

/** `24` covers low-power panels; `240` is the highest target we offer. */
const PLAUSIBLE_REFRESH_RATES = [24, 30, 48, 50, 60, 75, 90, 100, 120, 144, 165, 240];

const SSR_PACER: FramePacer = {
  get running() {
    return false;
  },
  get fps() {
    return null;
  },
  get frameMs() {
    return null;
  },
  start() {},
  stop() {},
  requestDraw() {},
  invalidate() {},
  dispose() {},
};

/** A refresh estimate derived from rAF deltas. Never a panel/OS query. */
export interface DisplayEstimate {
  /** Rounded to the closest plausible refresh rate when within 5%, else rounded raw. */
  hz: number;
  /** Unrounded `1000 / median delta`. */
  rawHz: number;
  confidence: EstimateConfidence;
  samples: number;
  medianDeltaMs: number;
  /** Median absolute deviation of the frame deltas, in milliseconds. */
  jitterMs: number;
  measuredAt: number;
  /** `true` for every rAF-based estimate; see `DISPLAY_LIMITATION`. */
  vsyncLimited: true;
  note: string;
}

export interface TargetOption {
  value: FrameTarget;
  label: string;
  allowed: boolean;
  /** Why the option is unavailable, or `null` when it is selectable. */
  reason: string | null;
  /** Effective rate of this option, or `null` for Auto (dynamic). */
  hz: number | null;
}

/** Central measured snapshot, republished at ~4 Hz by the active pacer. */
export interface PerformanceStats {
  /** Measured frames per second, or `null` before the first window / while idle. */
  fps: number | null;
  /** Measured milliseconds between drawn frames (`1000 / fps`). */
  frameMs: number | null;
  /** Measured synchronous `onFrame` duration, averaged over the window. */
  costMs: number | null;
  /** Drawn frames in the measurement window. */
  samples: number;
  target: FrameTarget;
  effectiveHz: number;
  displayHz: number | null;
  pacing: PacingMode;
  /** `Date.now()` of this snapshot. */
  updatedAt: number;
}

export interface DisplayDetectionOptions {
  /** Sampling window in milliseconds; clamped to at least 400 ms. Default 1000. */
  durationMs?: number;
}

export interface FramePacerOptions {
  /**
   * Draw one frame. `timestampMs` is the real rAF timestamp; `deltaMs` is the real
   * elapsed time since the previous drawn frame (0 on the first/one-off frame), so
   * animations advance at wall-clock speed regardless of pacing.
   */
  onFrame: (timestampMs: number, deltaMs: number) => void;
  /** Effective target reader. Defaults to `performanceStore.effectiveHz`. */
  targetHz?: () => number;
  /** Optional gate (visibility/intersection); frames are skipped while false. */
  shouldRun?: () => boolean;
}

export interface FramePacer {
  readonly running: boolean;
  /** Last measured window FPS, or `null` before the first window / after stop. */
  readonly fps: number | null;
  /** Last measured window frame time in ms, or `null`. */
  readonly frameMs: number | null;
  /** Start drawing on rAF at the effective target. Idempotent. */
  start(): void;
  /** Stop drawing and cancel pending callbacks. Idempotent. */
  stop(): void;
  /** Draw the next animation frame even if the target interval has not elapsed. */
  requestDraw(): void;
  /** Re-read target/display configuration on the next frame. */
  invalidate(): void;
  /** Stop, cancel callbacks, and unregister from configuration updates. */
  dispose(): void;
}

function median(values: number[]): number {
  if (values.length === 0) return 0;
  const middle = values.length >> 1;
  return values.length % 2 === 1 ? values[middle] : (values[middle - 1] + values[middle]) / 2;
}

/**
 * Turn rAF timestamp deltas into a refresh estimate.
 * Exported for tests/tools; returns `null` with fewer than two samples.
 */
export function estimateDisplayRefresh(deltas: number[]): DisplayEstimate | null {
  if (deltas.length < 2) return null;

  const sorted = [...deltas].sort((a, b) => a - b);
  const medianDeltaMs = median(sorted);
  if (!Number.isFinite(medianDeltaMs) || medianDeltaMs <= 0) return null;

  const jitterMs = median(sorted.map((delta) => Math.abs(delta - medianDeltaMs)));
  const rawHz = 1000 / medianDeltaMs;
  const snapped = PLAUSIBLE_REFRESH_RATES.find((rate) => Math.abs(rawHz - rate) / rate <= 0.05);
  const hz = snapped ?? Math.max(1, Math.round(rawHz));
  const relativeJitter = jitterMs / medianDeltaMs;
  const confidence: EstimateConfidence =
    sorted.length >= 30 && relativeJitter <= 0.02
      ? "high"
      : sorted.length >= 12 && relativeJitter <= 0.06
        ? "medium"
        : "low";

  return {
    hz,
    rawHz,
    confidence,
    samples: sorted.length,
    medianDeltaMs,
    jitterMs,
    measuredAt: Date.now(),
    vsyncLimited: true,
    note: `${sorted.length} rAF samples, ${medianDeltaMs.toFixed(2)} ms median delta, ${jitterMs.toFixed(2)} ms jitter. ${DISPLAY_LIMITATION}`,
  };
}

/**
 * Measure display refresh for ~1 second. Resolution is null when hidden,
 * unavailable, or too few frames were delivered. On demand only.
 */
export function measureDisplayRefresh(
  options: DisplayDetectionOptions = {},
): Promise<DisplayEstimate | null> {
  if (
    typeof window === "undefined" ||
    typeof document === "undefined" ||
    typeof requestAnimationFrame !== "function"
  ) {
    return Promise.resolve(null);
  }
  if (document.hidden) return Promise.resolve(null);

  const durationMs = Math.max(400, options.durationMs ?? DISPLAY_DETECTION_MS);

  return new Promise((resolve) => {
    const deltas: number[] = [];
    let startedAt = 0;
    let previous = 0;
    let frame = 0;
    let timer = 0;
    let settled = false;

    const settle = () => {
      if (settled) return;
      settled = true;
      if (frame) cancelAnimationFrame(frame);
      clearTimeout(timer);
      const usable = deltas.length >= 6 && !document.hidden;
      resolve(usable ? estimateDisplayRefresh(deltas) : null);
    };

    const tick = (now: number) => {
      if (settled) return;
      if (document.hidden) {
        settle();
        return;
      }
      if (startedAt === 0) startedAt = now;
      if (previous !== 0) {
        const delta = now - previous;
        if (delta > 0 && delta <= 500) deltas.push(delta);
      }
      previous = now;
      if (now - startedAt >= durationMs) {
        settle();
        return;
      }
      frame = requestAnimationFrame(tick);
    };

    timer = setTimeout(settle, durationMs + DISPLAY_DETECTION_TIMEOUT_MS);
    frame = requestAnimationFrame(tick);
  });
}

function readTargetPreference(): FrameTarget {
  if (typeof localStorage === "undefined") return "auto";
  try {
    const raw = localStorage.getItem(TARGET_STORAGE_KEY);
    for (const target of FRAME_TARGETS) {
      if (String(target) === raw) return target;
    }
  } catch {
    /* storage can be unavailable in hardened webviews */
  }
  return "auto";
}

function readOverlayPreference(): boolean {
  if (typeof localStorage === "undefined") return false;
  try {
    return localStorage.getItem(OVERLAY_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function writePreference(key: string, value: string): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(key, value);
  } catch {
    /* persistence is best-effort in hardened webviews */
  }
}

function emptyStats(
  target: FrameTarget,
  effectiveHz: number,
  displayHz: number | null,
): PerformanceStats {
  return {
    fps: null,
    frameMs: null,
    costMs: null,
    samples: 0,
    target,
    effectiveHz,
    displayHz,
    pacing: "idle",
    updatedAt: 0,
  };
}

const pacerInvalidators: Array<() => void> = [];

function notifyPacers(): void {
  for (const invalidate of pacerInvalidators) invalidate();
}

class PerformanceStore {
  /** Persisted target preference. */
  target = $state<FrameTarget>("auto");
  /** Persisted overlay preference; off by default. */
  overlay = $state(false);
  /** Last on-demand display estimate. */
  display = $state<DisplayEstimate | null>(null);
  detecting = $state(false);
  detectionError = $state<string | null>(null);
  /** Central measured snapshot (read-only from UI). */
  stats = $state<PerformanceStats>(emptyStats("auto", 60, null));

  /** Auto policy's current rate; observable so the UI can explain fallbacks. */
  private autoHz = $state(60);

  private detection: Promise<DisplayEstimate | null> | null = null;
  private statsOwner: symbol | null = null;
  private hotStreak = 0;
  private coolStreak = 0;

  constructor() {
    this.target = readTargetPreference();
    this.overlay = readOverlayPreference();
    this.autoHz = this.autoBaseHz();
    this.syncStats();
  }

  /** Auto never exceeds 60 fps and never exceeds the display estimate. */
  private autoBaseHz(): number {
    const hz = this.display?.hz;
    if (hz === undefined || hz === null) return 60;
    return Math.max(24, Math.min(60, Math.round(hz)));
  }

  /** Detected/estimated refresh rate, or `null` until measured. */
  get displayHz(): number | null {
    return this.display?.hz ?? null;
  }

  /** Confidence of the last estimate, or `null` until measured. */
  get displayConfidence(): EstimateConfidence | null {
    return this.display?.confidence ?? null;
  }

  /** Whether `value` is selectable with the current display estimate. */
  isTargetAllowed(value: FrameTarget): boolean {
    if (value === "auto" || value === 30) return true;
    const hz = this.display?.hz ?? null;
    return hz !== null && value <= hz * 1.03;
  }

  /** Rate the renderer should use; falls back to Auto when the selection is unsupported. */
  get effectiveHz(): number {
    if (this.target === "auto") return this.autoHz;
    if (this.target === 30) return 30;
    const hz = this.display?.hz ?? null;
    if (hz !== null && this.target <= hz * 1.03) return this.target;
    return this.autoHz;
  }

  get targetOptions(): TargetOption[] {
    const displayHz = this.display?.hz ?? null;
    return FRAME_TARGETS.map((value) => {
      if (value === "auto") {
        return { value, label: "Auto", allowed: true, reason: null, hz: null };
      }
      if (value === 30) {
        return { value, label: "30 fps", allowed: true, reason: null, hz: 30 };
      }
      const allowed = displayHz !== null && value <= displayHz * 1.03;
      const reason = allowed
        ? null
        : displayHz === null
          ? "Display refresh not measured yet. Detect it to enable this target."
          : `Display estimate is ${displayHz} Hz, so this target cannot be shown.`;
      return {
        value,
        label: allowed ? `${value} fps` : `${value} fps — unavailable`,
        allowed,
        reason,
        hz: allowed ? value : null,
      };
    });
  }

  /** Human explanation that distinguishes the frame cap from the monitor rate. */
  get effectiveNote(): string {
    if (this.target === "auto") return AUTO_TARGET_POLICY;
    if (this.target === 30) {
      return "30 fps is always selectable. Animation stays frame-time based, so it does not slow down.";
    }
    if (this.isTargetAllowed(this.target)) {
      return `${this.target} fps cap, within the ${this.display?.hz ?? "unknown"} Hz display estimate.`;
    }
    const hz = this.display?.hz ?? null;
    return hz === null
      ? `Display refresh is unknown, so ${this.target} fps is unavailable; Auto renders at ${Math.round(this.autoHz)} fps.`
      : `A ${hz} Hz display estimate cannot show ${this.target} fps; Auto renders at ${Math.round(this.autoHz)} fps instead.`;
  }

  get displaySummary(): string {
    if (this.detecting) return "Measuring display refresh…";
    if (this.display === null) {
      return this.detectionError ?? "Display refresh not measured.";
    }
    const { hz, confidence, samples } = this.display;
    return `${hz} Hz (${confidence} confidence, ${samples} rAF samples). ${DISPLAY_LIMITATION}`;
  }

  setTarget(next: FrameTarget): void {
    if (!FRAME_TARGETS.includes(next) || !this.isTargetAllowed(next)) return;
    this.target = next;
    this.autoHz = this.autoBaseHz();
    this.hotStreak = 0;
    this.coolStreak = 0;
    writePreference(TARGET_STORAGE_KEY, String(next));
    this.syncStats();
    notifyPacers();
  }

  setOverlay(next: boolean): void {
    this.overlay = next;
    writePreference(OVERLAY_STORAGE_KEY, next ? "true" : "false");
  }

  /**
   * Measure display refresh once. Concurrent calls share one measurement; call
   * again later to re-detect. Never runs continuously.
   */
  detectDisplay(): Promise<DisplayEstimate | null> {
    if (this.detection !== null) return this.detection;

    this.detecting = true;
    this.detectionError = null;

    const run = measureDisplayRefresh()
      .then((estimate) => {
        this.display = estimate;
        if (estimate === null) {
          this.detectionError =
            typeof document !== "undefined" && document.hidden
              ? "Display detection needs a visible window."
              : "Display refresh could not be measured (too few animation frames).";
        } else {
          this.autoHz = this.autoBaseHz();
          this.hotStreak = 0;
          this.coolStreak = 0;
          if (!this.isTargetAllowed(this.target)) {
            this.setTarget("auto");
          } else {
            this.syncStats();
            notifyPacers();
          }
        }
        return estimate;
      })
      .finally(() => {
        this.detecting = false;
        this.detection = null;
      });

    this.detection = run;
    return run;
  }

  /** Kick off one detection at mount when visible; no-op once measured or in flight. */
  ensureDisplayEstimate(): void {
    if (this.display !== null || this.detecting) return;
    if (typeof document !== "undefined" && document.hidden) return;
    void this.detectDisplay();
  }

  private pacingFor(effectiveHz: number): PacingMode {
    const displayHz = this.display?.hz ?? null;
    if (displayHz === null) return "fixed";
    return Math.abs(effectiveHz - displayHz) <= Math.max(1, displayHz * 0.03) ? "vsync" : "fixed";
  }

  private syncStats(): void {
    const effectiveHz = this.effectiveHz;
    const displayHz = this.display?.hz ?? null;
    if (
      this.stats.target === this.target &&
      this.stats.effectiveHz === effectiveHz &&
      this.stats.displayHz === displayHz
    ) {
      return;
    }
    this.stats = {
      ...this.stats,
      target: this.target,
      effectiveHz,
      displayHz,
      pacing: this.stats.pacing === "idle" ? "idle" : this.pacingFor(effectiveHz),
    };
  }

  /** Auto policy feedback. Called at the 4 Hz stats cadence, never per frame. */
  private adaptAuto(costMs: number): void {
    if (this.target !== "auto") {
      this.hotStreak = 0;
      this.coolStreak = 0;
      return;
    }

    const base = this.autoBaseHz();
    const floor = Math.min(30, base);

    if (costMs > AUTO_HOT_COST_MS) {
      this.coolStreak = 0;
      this.hotStreak += 1;
      if (this.hotStreak >= AUTO_HOT_PUBLICATIONS && this.autoHz > floor) {
        this.autoHz = floor;
        this.hotStreak = 0;
      }
    } else if (costMs < AUTO_COOL_COST_MS) {
      this.hotStreak = 0;
      this.coolStreak += 1;
      if (this.coolStreak >= AUTO_COOL_PUBLICATIONS && this.autoHz < base) {
        this.autoHz = base;
        this.coolStreak = 0;
      }
    } else {
      this.hotStreak = 0;
      this.coolStreak = 0;
    }

    if (this.autoHz < floor) this.autoHz = floor;
    if (this.autoHz > base) this.autoHz = base;
  }

  /** @internal A pacer claims ownership while its loop is running. */
  claimStats(owner: symbol): void {
    this.statsOwner = owner;
  }

  /** @internal Stop publishing from `owner`; only the current owner clears stats. */
  releaseStats(owner: symbol): void {
    if (this.statsOwner !== owner) return;
    this.statsOwner = null;
    this.stats = emptyStats(this.target, this.effectiveHz, this.display?.hz ?? null);
  }

  /** @internal Publish a measured window; ignored unless `owner` is current. */
  reportFrames(owner: symbol, fps: number, frameMs: number, costMs: number, samples: number): void {
    if (this.statsOwner !== owner) return;
    this.adaptAuto(costMs);
    const effectiveHz = this.effectiveHz;
    this.stats = {
      fps,
      frameMs,
      costMs,
      samples,
      target: this.target,
      effectiveHz,
      displayHz: this.display?.hz ?? null,
      pacing: this.pacingFor(effectiveHz),
      updatedAt: Date.now(),
    };
  }
}

export const performanceStore = new PerformanceStore();

/** Reject non-target strings from DOM inputs. */
export function parseFrameTarget(value: string | null): FrameTarget | null {
  if (value === null) return null;
  for (const target of FRAME_TARGETS) {
    if (String(target) === value) return target;
  }
  return null;
}

/**
 * Create the reusable canvas loop. It draws on rAF at the effective target,
 * skips surplus display callbacks, and passes real elapsed time to `onFrame`.
 *
 * The hot path allocates nothing: all scheduling and measurement state lives in
 * closure numbers; only the 4 Hz stats publication creates an object.
 */
export function createFramePacer(options: FramePacerOptions): FramePacer {
  if (
    typeof window === "undefined" ||
    typeof requestAnimationFrame !== "function" ||
    typeof performance === "undefined"
  ) {
    return SSR_PACER;
  }

  const token = Symbol("veil-frame-pacer");
  let running = false;
  let disposed = false;
  let frameId = 0;
  let singleId = 0;
  let pending = false;
  let lastDrawnAt = 0;
  let intervalMs = 1000 / 60;
  let originAt = 0;
  let lastSlot = 0;
  let configAt = 0;
  let windowStart = 0;
  let drawn = 0;
  let busyTotal = 0;
  let measuredFps: number | null = null;
  let measuredFrameMs: number | null = null;

  const readConfig = (now: number, force: boolean): void => {
    if (!force && now - configAt < CONFIG_REFRESH_MS) return;
    configAt = now;
    const target = options.targetHz ? options.targetHz() : performanceStore.effectiveHz;
    const hz = Number.isFinite(target) && target > 0 ? target : 30;
    const nextInterval = 1000 / hz;
    if (nextInterval === intervalMs) return;

    intervalMs = nextInterval;
    originAt = now;
    lastSlot = 0;
    pending = true;
    // A rate change starts a fresh measurement window so one window never mixes rates.
    windowStart = 0;
    drawn = 0;
    busyTotal = 0;
  };

  const publish = (now: number): void => {
    const elapsed = now - windowStart;
    if (drawn === 0 || elapsed <= 0) return;
    const fps = (drawn * 1000) / elapsed;
    const frameMs = elapsed / drawn;
    const costMs = busyTotal / drawn;
    measuredFps = fps;
    measuredFrameMs = frameMs;
    performanceStore.reportFrames(token, fps, frameMs, costMs, drawn);
    windowStart = now;
    drawn = 0;
    busyTotal = 0;
  };

  const drawFrame = (now: number, deltaMs: number, measured: boolean): void => {
    const started = performance.now();
    try {
      options.onFrame(now, deltaMs);
    } finally {
      if (measured && running) {
        const busy = performance.now() - started;
        if (windowStart === 0) {
          windowStart = now;
        } else {
          drawn += 1;
          busyTotal += busy;
          if (now - windowStart >= STATS_INTERVAL_MS) publish(now);
        }
      }
    }
  };

  const tick = (now: number): void => {
    frameId = 0;
    if (!running || disposed) return;
    if (options.shouldRun && !options.shouldRun()) {
      // Start a clean measurement window and redraw immediately when visible again,
      // rather than publishing FPS that spans the hidden period.
      windowStart = 0;
      drawn = 0;
      busyTotal = 0;
      lastDrawnAt = 0;
      originAt = now;
      lastSlot = 0;
      pending = true;
      frameId = requestAnimationFrame(tick);
      return;
    }

    readConfig(now, false);

    // Slot accounting holds the long-run rate at the target: a callback draws only
    // when floor(elapsed / interval) advances. Non-multiple display rates (e.g. 60 on
    // 144 Hz) therefore alternate cadence without drifting above or below target. The
    // tolerance absorbs float rounding and vsync jitter at exact boundaries.
    const slot = Math.floor((now - originAt + SLOT_TOLERANCE_MS) / intervalMs);
    if (!pending && slot <= lastSlot) {
      frameId = requestAnimationFrame(tick);
      return;
    }

    pending = false;
    if (slot > lastSlot) lastSlot = slot;
    const delta = lastDrawnAt === 0 ? 0 : Math.max(0, now - lastDrawnAt);
    lastDrawnAt = now;

    try {
      drawFrame(now, delta, true);
    } finally {
      if (running && !disposed) frameId = requestAnimationFrame(tick);
    }
  };

  const start = (): void => {
    if (disposed || running) return;
    running = true;
    pending = true;
    lastDrawnAt = 0;
    windowStart = 0;
    drawn = 0;
    busyTotal = 0;
    measuredFps = null;
    measuredFrameMs = null;
    configAt = 0;
    performanceStore.claimStats(token);
    const now = performance.now();
    readConfig(now, true);
    originAt = now;
    lastSlot = 0;
    if (frameId) cancelAnimationFrame(frameId);
    frameId = requestAnimationFrame(tick);
  };

  const stop = (): void => {
    if (!running) return;
    running = false;
    pending = false;
    if (frameId) cancelAnimationFrame(frameId);
    frameId = 0;
    if (singleId) cancelAnimationFrame(singleId);
    singleId = 0;
    lastDrawnAt = 0;
    originAt = 0;
    lastSlot = 0;
    windowStart = 0;
    drawn = 0;
    busyTotal = 0;
    measuredFps = null;
    measuredFrameMs = null;
    performanceStore.releaseStats(token);
  };

  const requestDraw = (): void => {
    if (disposed) return;
    if (running) {
      pending = true;
      return;
    }
    if (singleId) return;
    singleId = requestAnimationFrame((now) => {
      singleId = 0;
      if (running || disposed) return;
      readConfig(now, true);
      drawFrame(now, 0, false);
    });
  };

  const invalidate = (): void => {
    configAt = 0;
  };

  const dispose = (): void => {
    stop();
    disposed = true;
    if (singleId) cancelAnimationFrame(singleId);
    singleId = 0;
    const index = pacerInvalidators.indexOf(invalidate);
    if (index >= 0) pacerInvalidators.splice(index, 1);
  };

  pacerInvalidators.push(invalidate);

  return {
    get running() {
      return running;
    },
    get fps() {
      return measuredFps;
    },
    get frameMs() {
      return measuredFrameMs;
    },
    start,
    stop,
    requestDraw,
    invalidate,
    dispose,
  };
}
