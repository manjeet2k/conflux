// src/features/downloads/hooks/useDownloads.ts
import { useCallback, useEffect, useRef, useState } from 'react';
import type { DownloadTask } from '../types/downloads.types';
import { downloadsApi, type StartDownloadArgs } from '../api/downloads.api';
import { downloadsRepository } from '../api/downloads.repository';
import { StartDownloadUseCase } from '../utils/downloadValidation';

const startDownload = new StartDownloadUseCase(downloadsRepository);

export interface UseDownloadsReturn {
  tasks: DownloadTask[];
  start: (args: StartDownloadArgs) => Promise<DownloadTask>;
  pause: (ids: string[]) => Promise<number>;
  resume: (ids: string[]) => Promise<number>;
  remove: (ids: string[], deleteFiles: boolean) => Promise<void>;
}

/**
 * Custom hook managing the active download tasks list.
 *
 * Responsibilities:
 * - Subscribes to authoritative progress event stream from native backend.
 * - Reconciles in-memory tasks against native task registry.
 * - Enforces optimistic updates with fallback on failure.
 */
export function useDownloads(onError: (message: string) => void): UseDownloadsReturn {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const removedIds = useRef(new Set<string>());

  const upsert = useCallback((task: DownloadTask, overwrite: boolean) => {
    if (removedIds.current.has(task.id)) return;

    setTasks((prev) => {
      const idx = prev.findIndex((t) => t.id === task.id);
      if (idx === -1) return [task, ...prev];
      if (!overwrite) return prev;

      const next = prev.slice();
      next[idx] = task;
      return next;
    });
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlistenPromise = downloadsApi.onProgress((task) => {
      upsert(task, true);
    });

    downloadsApi
      .listTasks()
      .then((list) => {
        if (!disposed) {
          list.forEach((t) => upsert(t, false));
        }
      })
      .catch((e) => {
        onError(`Failed to load downloads: ${String(e)}`);
      });

    return () => {
      disposed = true;
      unlistenPromise.then((fn) => fn());
    };
  }, [upsert, onError]);

  const start = useCallback(
    async (args: StartDownloadArgs) => {
      const result = await startDownload.execute(args);
      if (!result.success) throw result.error;
      const task = result.data;
      upsert(task, false);
      return task;
    },
    [upsert]
  );

  const run = useCallback(
    async (label: string, ids: string[], action: (id: string) => Promise<unknown>) => {
      const results = await Promise.allSettled(ids.map(action));
      const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected');

      if (failed.length > 0) {
        onError(`${label} failed: ${String(failed[0].reason)}`);
      }

      return failed.length;
    },
    [onError]
  );

  const pause = useCallback(
    (ids: string[]) => run('Pause', ids, (id) => downloadsApi.pauseDownload(id)),
    [run]
  );

  const resume = useCallback(
    (ids: string[]) =>
      run('Resume', ids, async (id) => {
        const task = await downloadsApi.resumeDownload(id);
        upsert(task, false);
      }),
    [run, upsert]
  );

  const reconcile = useCallback(async () => {
    try {
      const list = await downloadsApi.listTasks();
      const live = new Set(list.map((t) => t.id));

      setTasks((prev) =>
        prev.filter((t) => {
          if (live.has(t.id)) return true;
          removedIds.current.add(t.id);
          return false;
        })
      );

      list.forEach((t) => upsert(t, false));
    } catch (e) {
      onError(`Failed to refresh downloads: ${String(e)}`);
    }
  }, [upsert, onError]);

  const remove = useCallback(
    async (ids: string[], deleteFiles: boolean) => {
      const failed = await run('Remove', ids, async (id) => {
        await downloadsApi.removeDownload(id, deleteFiles);
        removedIds.current.add(id);
        setTasks((prev) => prev.filter((t) => t.id !== id));
      });

      if (failed > 0) {
        await reconcile();
      }
    },
    [run, reconcile]
  );

  return { tasks, start, pause, resume, remove };
}
