import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api } from '../api';
import { PROGRESS_EVENT } from '../types';
import type { DownloadTask } from '../types';

/**
 * Download list state. `download-progress` events carry full task snapshots and are
 * authoritative; command results only insert tasks that no event has delivered yet
 * (an event can arrive before the command resolves).
 */
export function useDownloads(onError: (message: string) => void) {
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  // Ids removed by the user; late events for them are dropped.
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
    const unlisten = listen<DownloadTask>(PROGRESS_EVENT, (e) => upsert(e.payload, true));
    api
      .listTasks()
      .then((list) => {
        if (!disposed) list.forEach((t) => upsert(t, false));
      })
      .catch((e) => onError(`Failed to load downloads: ${String(e)}`));
    return () => {
      disposed = true;
      unlisten.then((fn) => fn());
    };
  }, [upsert, onError]);

  const start = useCallback(
    async (args: Parameters<typeof api.startDownload>[0]) => {
      const task = await api.startDownload(args);
      upsert(task, false);
      return task;
    },
    [upsert]
  );

  const run = useCallback(
    async (label: string, ids: string[], action: (id: string) => Promise<unknown>) => {
      const results = await Promise.allSettled(ids.map(action));
      const failed = results.filter((r): r is PromiseRejectedResult => r.status === 'rejected');
      if (failed.length > 0) onError(`${label} failed: ${String(failed[0].reason)}`);
      return failed.length;
    },
    [onError]
  );

  const pause = useCallback(
    (ids: string[]) => run('Pause', ids, (id) => api.pauseDownload(id)),
    [run]
  );

  const resume = useCallback(
    (ids: string[]) =>
      run('Resume', ids, async (id) => {
        const task = await api.resumeDownload(id);
        upsert(task, false);
      }),
    [run, upsert]
  );

  /** Drops tasks the backend no longer has (e.g. removed before the command failed). */
  const reconcile = useCallback(async () => {
    try {
      const list = await api.listTasks();
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
        await api.removeDownload(id, deleteFiles);
        removedIds.current.add(id);
        setTasks((prev) => prev.filter((t) => t.id !== id));
      });
      // A rejected remove may still have removed the task (e.g. only the file delete failed).
      if (failed > 0) await reconcile();
    },
    [run, reconcile]
  );

  return { tasks, start, pause, resume, remove };
}
