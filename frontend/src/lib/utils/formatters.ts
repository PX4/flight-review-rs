export function formatDuration(seconds: number | null): string {
  if (seconds == null) return '—';
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = Math.round(seconds % 60);
    return `${m}m ${s}s`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/**
 * Try to extract a flight date/time from a PX4 log filename.
 * PX4 default naming: `log_N_YYYY-M-D-HH-MM-SS.ulg` or `YYYY-MM-DD/HH_MM_SS.ulg`.
 * Returns a Date if a plausible timestamp is found, else null.
 */
export function parseFlightDateFromFilename(filename: string | null | undefined): Date | null {
  if (!filename) return null;
  // Pattern A: YYYY-M-D-HH-MM-SS (PX4 default, e.g. log_13_2024-3-15-12-34-56.ulg)
  const a = filename.match(/(\d{4})-(\d{1,2})-(\d{1,2})-(\d{1,2})-(\d{1,2})-(\d{1,2})/);
  if (a) {
    const [, y, mo, d, h, mi, s] = a;
    const date = new Date(Date.UTC(+y, +mo - 1, +d, +h, +mi, +s));
    if (!isNaN(date.getTime())) return date;
  }
  // Pattern B: YYYY-MM-DD separate from HH_MM_SS (e.g. 2024-03-15/12_34_56.ulg)
  const b = filename.match(/(\d{4})-(\d{2})-(\d{2})[^\d]+(\d{2})[_-](\d{2})[_-](\d{2})/);
  if (b) {
    const [, y, mo, d, h, mi, s] = b;
    const date = new Date(Date.UTC(+y, +mo - 1, +d, +h, +mi, +s));
    if (!isNaN(date.getTime())) return date;
  }
  return null;
}

export function formatFlightDateTime(date: Date): { date: string; time: string } {
  const d = date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
  const t = date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  return { date: d, time: t };
}

export function formatRelativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);

  if (diffSec < 60) return 'just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHr < 24) return `${diffHr}h ago`;
  if (diffDay < 30) return `${diffDay}d ago`;
  return date.toLocaleDateString();
}
