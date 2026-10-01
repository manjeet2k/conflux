export function formatBytes(bytes: number, decimals = 1): string {
  if (!Number.isFinite(bytes)) return '--';
  const sign = bytes < 0 ? '-' : '';
  const abs = Math.abs(bytes);
  // Sub-byte values (including 0) have a negative/undefined log; show them as bytes.
  if (abs < 1) return `${sign}${parseFloat(abs.toFixed(0))} B`;
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB'];
  const i = Math.min(sizes.length - 1, Math.floor(Math.log(abs) / Math.log(k)));
  return `${sign}${parseFloat((abs / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return '0 KB/s';
  return `${formatBytes(bytesPerSec, 1)}/s`;
}

export function formatEta(seconds: number): string {
  if (!seconds || seconds <= 0 || !isFinite(seconds)) return '--';
  // Round once, then split, so 59.6 s becomes "1m 0s" rather than "60s" or "0m 60s".
  const total = Math.max(1, Math.round(seconds));
  if (total < 60) return `${total}s`;
  const mins = Math.floor(total / 60);
  if (mins < 60) return `${mins}m ${total % 60}s`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'short', timeStyle: 'short' });

export function formatDate(ms: number | null | undefined): string {
  if (!ms) return '--';
  return dateFormat.format(new Date(ms));
}

/** Fraction downloaded in [0, 1]; 0 when the size is unknown. */
export function progressOf(downloaded: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(1, Math.max(0, downloaded / total));
}
