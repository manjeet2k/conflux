import { invoke } from '@tauri-apps/api/core';
import type {
  AboutLink,
  AdapterInfo,
  BrowserIntegrationStatus,
  Diagnostics,
  DownloadTask,
  ExternalDownloadPayload,
  ProbeResult,
  RequestHeaders,
  Settings,
  UpdateInfo,
  WindowBackdrop,
} from './types';

/** Typed wrappers around the Tauri commands in crates/conflux-desktop/src/commands.rs. */
export const api = {
  discoverAdapters: () => invoke<AdapterInfo[]>('discover_adapters'),
  setAdapterEnabled: (id: string, enabled: boolean) =>
    invoke<AdapterInfo[]>('set_adapter_enabled', { id, enabled }),
  probeUrl: (url: string, headers?: RequestHeaders | null) =>
    invoke<ProbeResult>('probe_url', { url, headers: headers ?? null }),
  startDownload: (args: {
    url: string;
    saveDir: string;
    filename: string | null;
    headers?: RequestHeaders | null;
  }) => invoke<DownloadTask>('start_download', { ...args, headers: args.headers ?? null }),
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
  takeStartupNotices: () => invoke<string[]>('take_startup_notices'),
  takePendingDownload: () => invoke<ExternalDownloadPayload | null>('take_pending_download'),
  folderExists: (path: string) => invoke<boolean>('folder_exists', { path }),
  getDiagnostics: () => invoke<Diagnostics>('get_diagnostics'),
  openLogsFolder: () => invoke<void>('open_logs_folder'),
  checkForUpdate: () => invoke<UpdateInfo | null>('check_for_update'),
  /** Downloads, pauses running downloads, installs and relaunches; resolves only on failure. */
  installUpdate: () => invoke<void>('install_update'),
  openAboutLink: (target: AboutLink) => invoke<void>('open_about_link', { target }),
  applyWindowTheme: (dark: boolean) => invoke<WindowBackdrop>('apply_window_theme', { dark }),
  getBrowserIntegrationStatus: () =>
    invoke<BrowserIntegrationStatus>('get_browser_integration_status'),
  registerBrowserExtension: () => invoke<void>('register_browser_extension'),
};

/** Tauri rejects with a plain string for `Result<_, String>` commands. */
export function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
