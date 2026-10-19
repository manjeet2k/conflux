// src/utils/downloadQueue.ts
import type { DownloadQueue, DownloadRequest } from '@/features/downloads/types/downloads.types';

export function enqueueDownload(state: DownloadQueue, request: DownloadRequest, blocked: boolean): DownloadQueue {
  return !state.current && !blocked
    ? { current: request, queued: state.queued }
    : { ...state, queued: [...state.queued, request] };
}

export function advanceDownload(state: DownloadQueue, requestId: string): DownloadQueue {
  if (state.current?.id !== requestId) return state;
  return { current: state.queued[0] ?? null, queued: state.queued.slice(1) };
}

export function presentQueuedDownload(state: DownloadQueue): DownloadQueue {
  if (state.current || state.queued.length === 0) return state;
  return { current: state.queued[0], queued: state.queued.slice(1) };
}

export const ManageQueueUseCase = {
  enqueue: enqueueDownload,
  advance: advanceDownload,
  presentQueued: presentQueuedDownload,
};
