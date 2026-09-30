import { useEffect, useRef, useState } from 'react';
import type { DownloadTask } from '../types';

export const HISTORY_SECONDS = 60;

export interface SpeedSample {
  total: number;
  /** Keyed by adapter id ('default' for unbound routing). */
  byAdapter: Record<string, number>;
}

export const adapterKey = (adapterId: string | null) => adapterId ?? 'default';

/** Samples aggregate and per-adapter throughput once per second over a rolling window. */
export function useSpeedHistory(tasks: DownloadTask[]): SpeedSample[] {
  const latest = useRef(tasks);
  const [history, setHistory] = useState<SpeedSample[]>([]);

  useEffect(() => {
    latest.current = tasks;
  }, [tasks]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const sample: SpeedSample = { total: 0, byAdapter: {} };
      for (const t of latest.current) {
        if (t.status !== 'downloading') continue;
        sample.total += t.speed_bytes_sec;
        for (const a of t.adapters) {
          const key = adapterKey(a.adapter_id);
          sample.byAdapter[key] = (sample.byAdapter[key] ?? 0) + a.speed_bytes_sec;
        }
      }
      setHistory((prev) => [...prev, sample].slice(-HISTORY_SECONDS));
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);

  return history;
}
