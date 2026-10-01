import { invoke } from '@tauri-apps/api/core';
import type { AdapterInfo, DownloadTask, ProbeResult, Settings, WindowBackdrop } from './types';

/** Typed wrappers around the Tauri commands in crates/conflux-desktop/src/commands.rs. */
export const api = {
  discoverAdapters: () => invoke<AdapterInfo[]>('discover_adapters'),
  setAdapterEnabled: (id: string, enabled: boolean) =>
    invoke<AdapterInfo[]>('set_adapter_enabled', { id, enabled }),
  probeUrl: (url: string) => invoke<ProbeResult>('probe_url', { url }),
  startDownload: (args: { url: string; saveDir: string; filename: string | null }) =>
    invoke<DownloadTask>('start_download', args),
  pauseDownload: (taskId: string) => invoke<DownloadTask>('pause_download', { taskId }),
  resumeDownload: (taskId: string) => invoke<DownloadTask>('resume_download', { taskId }),
  pauseAll: () => invoke<void>('pause_all'),
  resumeAll: () => invoke<void>('resume_all'),
  removeDownload: (taskId: string, deleteFile: boolean) =>
    invoke<void>('remove_download', { taskId, deleteFile }),
  listTasks: () => invoke<DownloadTask[]>('list_tasks'),
  openFile: (taskId: string) => invoke<void>('open_file', { taskId }),
  revealFile: (taskId: string) => invoke<void>('reveal_file', { taskId }),
  getSettings: () => invoke<Settings>('get_settings'),
  updateSettings: (settings: Settings) => invoke<Settings>('update_settings', { settings }),
  applyWindowTheme: (dark: boolean) => invoke<WindowBackdrop>('apply_window_theme', { dark }),
};

/** Tauri rejects with a plain string for `Result<_, String>` commands. */
export function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
