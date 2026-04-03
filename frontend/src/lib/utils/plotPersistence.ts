import type { PlotConfig } from '$lib/types';

const KEY_PREFIX = 'fr-plots-';
const MAX_ENTRIES = 50;

let saveTimer: ReturnType<typeof setTimeout> | null = null;

export function savePlotLayout(logId: string, plots: PlotConfig[]): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    try {
      const key = KEY_PREFIX + logId;
      localStorage.setItem(key, JSON.stringify(plots));
      evictOldEntries(key);
    } catch {
      // localStorage full or unavailable — fail silently
    }
  }, 500);
}

export function loadPlotLayout(logId: string): PlotConfig[] | null {
  try {
    const raw = localStorage.getItem(KEY_PREFIX + logId);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return null;
    return parsed;
  } catch {
    return null;
  }
}

function evictOldEntries(justWritten: string): void {
  const keys: string[] = [];
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    if (k?.startsWith(KEY_PREFIX)) keys.push(k);
  }
  if (keys.length <= MAX_ENTRIES) return;
  // Remove oldest entries (simple FIFO — remove those that aren't the one we just wrote)
  const toRemove = keys.filter((k) => k !== justWritten).slice(0, keys.length - MAX_ENTRIES);
  for (const k of toRemove) localStorage.removeItem(k);
}
