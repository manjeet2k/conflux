// src/features/settings/api/settings.api.ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  Backdrop,
  BrowserIntegrationStatus,
  Settings,
  UpdateInfo,
} from '../types/settings.types';
import { UPDATE_AVAILABLE_EVENT } from '@/utils/events.constants';

export type AboutLink =
  | 'licenses'
  | 'repo'
  | 'releases'
  | 'extension'
  | 'extension-chrome'
  | 'extension-firefox';

export interface WindowBackdrop {
  backdrop: Backdrop;
}

/**
 * Settings API
 *
 * IPC bindings for application preferences, native window theme/materials,
 * software updates, and browser extension integration.
 */
export const settingsApi = {
  getSettings: () => invoke<Settings>('get_settings'),

  updateSettings: (settings: Settings) =>
    invoke<Settings>('update_settings', { settings }),

  takeStartupNotices: () => invoke<string[]>('take_startup_notices'),

  folderExists: (path: string) => invoke<boolean>('folder_exists', { path }),

  checkForUpdate: () => invoke<UpdateInfo | null>('check_for_update'),

  installUpdate: () => invoke<void>('install_update'),

  openAboutLink: (target: AboutLink) => invoke<void>('open_about_link', { target }),

  applyWindowTheme: (dark: boolean, backdrop: Backdrop) =>
    invoke<WindowBackdrop>('apply_window_theme', { dark, backdrop }),

  getBrowserIntegrationStatus: () =>
    invoke<BrowserIntegrationStatus>('get_browser_integration_status'),

  registerBrowserExtension: () => invoke<void>('register_browser_extension'),

  onUpdateAvailable: (callback: (info: UpdateInfo) => void) =>
    listen<UpdateInfo>(UPDATE_AVAILABLE_EVENT, (event) => callback(event.payload)),
};
