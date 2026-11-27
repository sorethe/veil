/** Formatting helpers shared by every view. Pure functions only. */

export interface NormalizedError {
  code: string | null;
  message: string;
  detail: string | null;
}

/**
 * The backend rejects with a CommandError envelope `{code, message, detail?}`.
 * Older builds rejected with a plain string; both are accepted here.
 */
export function normalizeError(error: unknown): NormalizedError {
  if (typeof error === "string") {
    return { code: null, message: error, detail: null };
  }
  if (error instanceof Error) {
    return { code: null, message: error.message, detail: null };
  }
  if (typeof error === "object" && error !== null) {
    const rawCode = "code" in error ? error.code : undefined;
    const code =
      typeof rawCode === "string" ? rawCode : typeof rawCode === "number" ? String(rawCode) : null;
    const rawMessage = "message" in error ? error.message : undefined;
    const message = typeof rawMessage === "string" && rawMessage.trim() ? rawMessage : null;
    const rawDetail = "detail" in error ? error.detail : undefined;
    const detail = typeof rawDetail === "string" && rawDetail.trim() ? rawDetail : null;
    if (message) return { code, message, detail };

    const inner = "error" in error ? error.error : undefined;
    if (typeof inner === "string" && inner.trim()) return { code, message: inner, detail };
    if (typeof inner === "object" && inner !== null) return normalizeError(inner);

    try {
      return { code, message: JSON.stringify(error), detail };
    } catch {
      return { code, message: "Unserializable backend error", detail };
    }
  }
  if (error === null || error === undefined) {
    return {
      code: null,
      message: "The backend reported an error without a message.",
      detail: null,
    };
  }
  return { code: null, message: String(error), detail: null };
}

export function errorMessage(error: unknown): string {
  const normalized = normalizeError(error);
  const prefix = normalized.code ? `${normalized.code}: ` : "";
  const detail =
    normalized.detail && normalized.detail !== normalized.message ? `\n${normalized.detail}` : "";
  return `${prefix}${normalized.message}${detail}`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value >= 100 || exponent === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[exponent]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** HH:MM:SS for session durations. */
export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(Number.isFinite(totalSeconds) ? totalSeconds : 0));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  return [hours, minutes, rest].map((part) => String(part).padStart(2, "0")).join(":");
}

/** Accepts epoch seconds or milliseconds, as backends differ. */
export function toMillis(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 0;
  return value < 1_000_000_000_000 ? value * 1000 : value;
}

export function formatClock(value: number): string {
  const millis = toMillis(value);
  if (!millis) return "—";
  const date = new Date(millis);
  return date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

export function formatDateTime(value: number): string {
  const millis = toMillis(value);
  if (!millis) return "—";
  return new Date(millis).toLocaleString([], {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

export function formatRelative(value: number): string {
  const millis = toMillis(value);
  if (!millis) return "—";
  const delta = Date.now() - millis;
  if (delta < 45_000) return "just now";
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} d ago`;
  return formatDateTime(millis);
}

export function formatAge(start: number, now: number = Date.now()): string {
  const millis = toMillis(start);
  if (!millis) return "—";
  return formatDuration(Math.max(0, (now - millis) / 1000));
}

export function formatDelay(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) return "—";
  if (ms < 0) return "timeout";
  return `${Math.round(ms)} ms`;
}

export function hostOf(source: string): string {
  if (!source) return "—";
  try {
    const url = new URL(source.includes("://") ? source : `https://${source}`);
    return url.host || source;
  } catch {
    return source;
  }
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Shared log scale for speed arcs and sparklines: 1 KiB/s .. 1 GiB/s. */
export function logSpeedRatio(bytesPerSecond: number): number {
  const min = Math.log10(1024);
  const max = Math.log10(1024 ** 3);
  const value = Math.max(bytesPerSecond, 0);
  if (value <= 0) return 0;
  return clamp((Math.log10(value) - min) / (max - min), 0, 1);
}

export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* fall through to the textarea fallback */
  }
  try {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(area);
    return ok;
  } catch {
    return false;
  }
}

export function statusWord(status: string): string {
  switch (status) {
    case "starting":
      return "LINKING";
    case "connected":
      return "LIVE";
    case "stopping":
      return "STOPPING";
    case "error":
      return "FAULT";
    default:
      return "IDLE";
  }
}

export function modeLabel(mode: string | null | undefined): string {
  if (mode === "tun") return "TUN";
  if (mode === "system") return "System proxy";
  return "—";
}
