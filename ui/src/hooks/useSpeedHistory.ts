import { useEffect, useRef, useState } from 'react';
import type { DownloadTask } from '../types';

export const HISTORY_SECONDS = 60;

export interface SpeedSample {
  total: number;
  /** Keyed by task id so a selected download never includes another task's traffic. */
  byTask: Record<string, number>;
  /** Keyed by adapter id ('default' for unbound routing). */
  byAdapter: Record<string, number>;
}

export const adapterKey = (adapterId: string | null) => adapterId ?? 'default';

interface SpeedHistory {
  /** Wall-clock samples used by the aggregate and adapter network charts. */
  history: SpeedSample[];
  /** Real measurements only; inactive tasks keep their last sixty active seconds. */
  taskHistory: Record<string, number[]>;
}

const validSpeed = (speed: number) => Number.isFinite(speed) && speed > 0 ? speed : 0;

/** Samples throughput once per second, preserving each task's graph while it is inactive. */
export function useSpeedHistory(tasks: DownloadTask[]): SpeedHistory {
  const latest = useRef(tasks);
  const [history, setHistory] = useState<SpeedHistory>(() => ({
    history: [],
    taskHistory: Object.fromEntries(tasks.map((task) => [task.id, []])),
  }));

  useEffect(() => {
    latest.current = tasks;
  }, [tasks]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const sample: SpeedSample = { total: 0, byTask: {}, byAdapter: {} };
      const currentTasks = latest.current;
      for (const t of currentTasks) {
        if (t.status !== 'downloading') continue;
        const speed = validSpeed(t.speed_bytes_sec);
        sample.total += speed;
        sample.byTask[t.id] = speed;
        for (const a of t.adapters) {
          const key = adapterKey(a.adapter_id);
          sample.byAdapter[key] = (sample.byAdapter[key] ?? 0) + validSpeed(a.speed_bytes_sec);
        }
      }
      setHistory((prev) => ({
        history: [...prev.history, sample].slice(-HISTORY_SECONDS),
        taskHistory: Object.fromEntries(currentTasks.map((task) => {
          const previous = prev.taskHistory[task.id] ?? [];
          return [task.id, task.status === 'downloading'
            ? [...previous, sample.byTask[task.id]].slice(-HISTORY_SECONDS)
            : previous];
        })),
      }));
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);

  return history;
}
