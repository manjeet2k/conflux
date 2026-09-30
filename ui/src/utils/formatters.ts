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
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const mins = Math.floor(seconds / 60);
  const secs = Math.round(seconds % 60);
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const remMins = mins % 60;
  return `${hours}h ${remMins}m`;
}
