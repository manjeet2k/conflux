// src/features/settings/types/settings.types.ts

export type ThemePreference = 'system' | 'light' | 'dark';

export type Backdrop = 'none' | 'mica' | 'acrylic' | 'mica_alt';

export interface Settings {
  theme: ThemePreference;
  backdrop: Backdrop;
  default_save_dir: string | null;
  connections_per_adapter: number;
  slow_mode: boolean;
  slow_mode_limit_kib: number;
  notify_on_complete: boolean;
  close_to_tray: boolean;
  auto_aggregate_adapters: boolean;
  check_updates_on_start: boolean;
  adapter_overrides?: Record<string, boolean>;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
  date: string | null;
}

export interface BrowserIntegrationStatus {
  registered: boolean;
}
