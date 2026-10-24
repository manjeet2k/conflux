// src/features/downloads/api/downloads.api.ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  PROGRESS_EVENT,
  EXTERNAL_DOWNLOAD_EVENT,
  HISTORY_STATUS_EVENT,
} from '@/utils/events.constants';

import type {
  DownloadTask,
  ExternalDownloadPayload,
  ProbeResult,
  RequestHeaders,
} from '../types/downloads.types';
import type { StartDownloadParams } from '../utils/downloadValidation';

export type StartDownloadArgs = StartDownloadParams;

/**
 * Downloads API
 *
 * Direct IPC bindings to the native Conflux desktop download commands and progress events.
 */
export const downloadsApi = {
  listTasks: () => invoke<DownloadTask[]>('list_tasks'),

  probeUrl: (url: string, headers?: RequestHeaders | null) =>
    invoke<ProbeResult>('probe_url', { url, headers: headers ?? null }),

  startDownload: (args: StartDownloadArgs) =>
    invoke<DownloadTask>('start_download', {
      ...args,
      headers: args.headers ?? null,
      handoffId: args.handoffId ?? null,
    }),

  pauseDownload: (taskId: string) =>
    invoke<DownloadTask>('pause_download', { taskId }),

  resumeDownload: (taskId: string) =>
    invoke<DownloadTask>('resume_download', { taskId }),

  pauseAll: () => invoke<void>('pause_all'),

  resumeAll: () => invoke<void>('resume_all'),

  removeDownload: (taskId: string, deleteFile: boolean) =>
    invoke<void>('remove_download', { taskId, deleteFile }),

  openFile: (taskId: string) => invoke<void>('open_file', { taskId }),

  revealFile: (taskId: string) => invoke<void>('reveal_file', { taskId }),

  takePendingDownload: () =>
    invoke<ExternalDownloadPayload | null>('take_pending_download'),

  dismissExternalDownload: (handoffId: string) =>
    invoke<void>('dismiss_external_download', { handoffId }),

  onProgress: (callback: (task: DownloadTask) => void) =>
    listen<DownloadTask>(PROGRESS_EVENT, (event) => callback(event.payload)),

  onExternalDownload: (callback: () => void) =>
    listen(EXTERNAL_DOWNLOAD_EVENT, () => callback()),

  onHistoryStatus: (callback: (status: string | null) => void) =>
    listen<string | null>(HISTORY_STATUS_EVENT, (event) => callback(event.payload)),
};
