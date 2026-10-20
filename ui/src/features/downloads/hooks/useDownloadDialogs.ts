import { useCallback, useEffect, useRef, useState } from 'react';
import { advanceDownload, enqueueDownload, presentQueuedDownload } from '../utils/downloadQueue';
import { downloadsApi } from '../api/downloads.api';
import { GRID_ID } from '../utils/downloadGrid';

import type { DownloadQueue, DownloadRequest, DownloadTask, ExternalDownloadPayload, RequestHeaders } from '../types/downloads.types';

interface DownloadDialogOptions {
  tasks: DownloadTask[];
  refreshAdapters: () => unknown;
  notify: (intent: 'info', message: string) => void;
  onError: (message: string) => void;
}

export function useDownloadDialogs({ tasks, refreshAdapters, notify, onError }: DownloadDialogOptions) {
  const [downloadDialogs, setDownloadDialogs] = useState<DownloadQueue>({ current: null, queued: [] });
  const downloadDialogsRef = useRef(downloadDialogs);
  const dialogOpenRef = useRef(false);

  const updateDownloadDialogs = useCallback((update: (state: DownloadQueue) => DownloadQueue) => {
    const next = update(downloadDialogsRef.current);
    downloadDialogsRef.current = next;
    if (next.current) dialogOpenRef.current = true;
    setDownloadDialogs(next);
  }, []);

  const addOpen = downloadDialogs.current !== null;
  const [removeIds, setRemoveIds] = useState<string[]>([]);
  const removeTasks = tasks.filter((t) => removeIds.includes(t.id));
  const removeOpen = removeTasks.length > 0;

  useEffect(() => {
    dialogOpenRef.current = downloadDialogsRef.current.current !== null || removeOpen;
    if (!removeOpen && !downloadDialogsRef.current.current) {
      updateDownloadDialogs(presentQueuedDownload);
    }
  }, [addOpen, removeOpen, updateDownloadDialogs]);

  const dismissRequest = useCallback(
    (request: DownloadRequest) => {
      if (request.payload.handoff_id) {
        downloadsApi
          .dismissExternalDownload(request.payload.handoff_id)
          .catch((error) => onError(String(error)));
      }
    },
    [onError]
  );

  const openAdd = useCallback(
    (url = '', filename: string | null = null, headers: RequestHeaders | null = null) => {
      if (dialogOpenRef.current) return;
      refreshAdapters();
      const request: DownloadRequest = { id: crypto.randomUUID(), payload: { url, filename, headers } };
      dialogOpenRef.current = true;
      updateDownloadDialogs((state) => enqueueDownload(state, request, false));
    },
    [refreshAdapters, updateDownloadDialogs]
  );

  const handleExternalDownload = useCallback(
    (payload: ExternalDownloadPayload) => {
      refreshAdapters();
      const request: DownloadRequest = { id: crypto.randomUUID(), payload };
      const blocked = dialogOpenRef.current;
      if (blocked) notify('info', 'Queued link: ' + (payload.filename || payload.url));
      updateDownloadDialogs((state) => enqueueDownload(state, request, blocked));
    },
    [refreshAdapters, notify, updateDownloadDialogs]
  );

  const handleAdvanceOrClose = useCallback(
    (requestId: string, accepted = false) => {
      const current = downloadDialogsRef.current.current;
      if (current?.id !== requestId) return;
      if (!accepted) dismissRequest(current);
      updateDownloadDialogs((state) => advanceDownload(state, requestId));
      dialogOpenRef.current = downloadDialogsRef.current.current !== null;
      if (!dialogOpenRef.current) document.getElementById(GRID_ID)?.focus();
    },
    [dismissRequest, updateDownloadDialogs]
  );

  const handleCancelAll = useCallback(() => {
    const state = downloadDialogsRef.current;
    for (const request of [...(state.current ? [state.current] : []), ...state.queued]) {
      dismissRequest(request);
    }
    updateDownloadDialogs(() => ({ current: null, queued: [] }));
    dialogOpenRef.current = false;
    document.getElementById(GRID_ID)?.focus();
  }, [dismissRequest, updateDownloadDialogs]);

  const openRemove = useCallback((ids: string[]) => {
    if (ids.length === 0) return;
    dialogOpenRef.current = true;
    setRemoveIds(ids);
  }, []);
  const closeRemove = useCallback(() => setRemoveIds([]), []);
  const isDialogOpen = useCallback(() => dialogOpenRef.current, []);

  return {
    current: downloadDialogs.current,
    queueCount: downloadDialogs.queued.length,
    removeTasks, openAdd, openRemove, closeRemove, isDialogOpen,
    handleExternalDownload, advanceOrClose: handleAdvanceOrClose, cancelAll: handleCancelAll,
  };
}
