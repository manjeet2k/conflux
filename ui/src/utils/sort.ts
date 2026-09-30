import type { DownloadTask } from '../types';
import { progressOf } from './formatters';

export type SortKey = 'name' | 'size' | 'progress' | 'speed' | 'eta' | 'status' | 'added';
export interface SortState {
  key: SortKey;
  direction: 'ascending' | 'descending';
}

const statusOrder = { downloading: 0, paused: 1, error: 2, completed: 3 } as const;

function compare(a: DownloadTask, b: DownloadTask, key: SortKey): number {
  switch (key) {
    case 'name':
      return a.filename.localeCompare(b.filename, undefined, { numeric: true, sensitivity: 'base' });
    case 'size':
      return a.total_bytes - b.total_bytes;
    case 'progress':
      return progressOf(a.downloaded_bytes, a.total_bytes) - progressOf(b.downloaded_bytes, b.total_bytes);
    case 'speed':
      return a.speed_bytes_sec - b.speed_bytes_sec;
    case 'eta':
      return (a.eta_seconds || Infinity) - (b.eta_seconds || Infinity);
    case 'status':
      return statusOrder[a.status] - statusOrder[b.status];
    case 'added':
      return a.created_at_ms - b.created_at_ms;
  }
}

export function sortTasks(tasks: DownloadTask[], sort: SortState): DownloadTask[] {
  const sign = sort.direction === 'ascending' ? 1 : -1;
  // Tie-break on creation time so rows don't jump around while speeds fluctuate.
  return [...tasks].sort((a, b) => sign * compare(a, b, sort.key) || b.created_at_ms - a.created_at_ms);
}
