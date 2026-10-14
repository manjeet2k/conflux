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
    case 'speed': {
      const aSpeed = a.status === 'downloading' ? a.speed_bytes_sec : (a.avg_speed_bytes_sec ?? 0);
      const bSpeed = b.status === 'downloading' ? b.speed_bytes_sec : (b.avg_speed_bytes_sec ?? 0);
      return aSpeed - bSpeed;
    }
    case 'eta': {
      const aVal = a.status === 'downloading' && a.eta_seconds !== null && a.eta_seconds > 0 ? a.eta_seconds : 0;
      const bVal = b.status === 'downloading' && b.eta_seconds !== null && b.eta_seconds > 0 ? b.eta_seconds : 0;
      return aVal - bVal;
    }
    case 'status':
      return statusOrder[a.status] - statusOrder[b.status];
    case 'added':
      return a.created_at_ms - b.created_at_ms;
  }
}

export function sortTasks(tasks: DownloadTask[], sort: SortState): DownloadTask[] {
  const sign = sort.direction === 'ascending' ? 1 : -1;
  return [...tasks].sort((a, b) => {
    if (sort.key === 'eta') {
      const aActive = a.status === 'downloading' && a.eta_seconds !== null && a.eta_seconds > 0;
      const bActive = b.status === 'downloading' && b.eta_seconds !== null && b.eta_seconds > 0;
      if (aActive !== bActive) {
        return aActive ? -1 : 1; // Active downloads with ETA always partition to the top
      }
      if (!aActive && !bActive) {
        const aDur = a.duration_ms ?? 0;
        const bDur = b.duration_ms ?? 0;
        if (aDur !== bDur) {
          return sign * (aDur - bDur) || b.created_at_ms - a.created_at_ms;
        }
        return b.created_at_ms - a.created_at_ms;
      }
      return sign * ((a.eta_seconds ?? 0) - (b.eta_seconds ?? 0)) || b.created_at_ms - a.created_at_ms;
    }
    // Tie-break on creation time so rows don't jump around while speeds fluctuate.
    return sign * compare(a, b, sort.key) || b.created_at_ms - a.created_at_ms;
  });
}
