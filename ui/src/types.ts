// IPC contract with crates/conflux-desktop/src/commands.rs. Field names are the serde names.

export type AdapterKind = 'ethernet' | 'wifi' | 'cellular' | 'virtual' | 'loopback' | 'other';

/** Why discovery disables an adapter by default (Rust DisabledReason, snake_case). */
export type DisabledReason = 'loopback' | 'link_local' | 'no_ipv4' | 'virtual';

// Matches Rust AdapterInfo
export interface AdapterInfo {
  id: string;
  name: string;
  ip: string;
  is_ipv4: boolean;
  is_loopback: boolean;
  /** Discovery default: selected for new downloads unless the user unchecks it. */
  enabled: boolean;
  /** The engine can bind to it (IPv4, not loopback, not link-local). */
  usable: boolean;
  kind: AdapterKind;
  /** Set when discovery disables the adapter by default, whatever the user chose since. */
  disabled_reason: DisabledReason | null;
}

// Matches Rust TaskStatus (serde rename_all = "lowercase")
export type TaskStatus = 'downloading' | 'paused' | 'completed' | 'error';

// Matches Rust ProbeResult (returned by probe_url)
export interface ProbeResult {
  url: string;
  filename: string;
  total_bytes: number;
  supports_ranges: boolean;
}

// Matches Rust AdapterStat: live per-adapter throughput of one task.
export interface AdapterStat {
  /** Id of the matching AdapterInfo, or null for unbound default routing. */
  adapter_id: string | null;
  name: string;
  ip: string | null;
  /** Gross bytes received through this adapter in the current run (includes retried bytes). */
  downloaded_bytes: number;
  speed_bytes_sec: number;
  active_connections: number;
  /** Dropped by the engine after repeated failures. */
  dropped: boolean;
  /** Human-readable cause of the latest failed attempt on this adapter (never a URL); null if healthy. */
  last_error?: string | null;
  /** Why the engine stopped using this adapter in this task; null if still in use. */
  drop_reason?: string | null;
}

// Matches Rust DownloadTaskState. Returned by list_tasks/start/pause/resume and emitted as
// the payload of the `download-progress` event (a full snapshot each time).
export interface DownloadTask {
  id: string;
  url: string;
  filename: string;
  save_dir: string;
  save_path: string;
  adapter_ids?: string[];
  total_bytes: number;
  supports_ranges: boolean;
  downloaded_bytes: number;
  status: TaskStatus;
  speed_bytes_sec: number;
  eta_seconds: number;
  active_chunks: number;
  completed_chunks: number;
  total_chunks: number;
  sha256: string | null;
  error: string | null;
  created_at_ms: number;
  completed_at_ms: number | null;
  adapters: AdapterStat[];
  /** One char per chunk: '.' pending, '>' downloading, '#' completed, '!' failed. */
  chunk_map: string | null;
}

export type ThemePreference = 'system' | 'light' | 'dark';

// Matches Rust Settings
export interface Settings {
  /** Version of the on-disk layout; owned by the backend. */
  schema_version?: number;
  theme: ThemePreference;
  /** null = the OS Downloads folder. */
  default_save_dir: string | null;
  connections_per_adapter: number;
  chunk_size_mb: number;
  notify_on_complete: boolean;
  close_to_tray: boolean;
  auto_aggregate_adapters: boolean;
  /** Quiet update check shortly after start; only notifies, never installs. */
  check_updates_on_start: boolean;
  adapter_overrides?: Record<string, boolean>;
}

// Matches Rust UpdateInfo (check_for_update); null from the command means up to date.
export interface UpdateInfo {
  version: string;
  notes: string | null;
  /** Publish date as the updater reports it (RFC 3339-like), if the manifest has one. */
  date: string | null;
}

/** Targets `open_about_link` accepts; the backend maps them to fixed paths/URLs. */
export type AboutLink = 'licenses' | 'repo' | 'releases';

// Matches Rust Diagnostics (get_diagnostics): no paths, user names, URLs or full IPs.
export interface Diagnostics {
  app_version: string;
  os: string;
  arch: string;
  webview_version: string | null;
  adapters: {
    name: string;
    kind: AdapterKind;
    enabled: boolean;
    usable: boolean;
    disabled_reason: DisabledReason | null;
    /** Address with the host part masked, e.g. 192.168.1.x. */
    subnet: string;
  }[];
  settings: {
    schema_version: number;
    theme: ThemePreference;
    connections_per_adapter: number;
    chunk_size_mb: number;
    notify_on_complete: boolean;
    close_to_tray: boolean;
    auto_aggregate_adapters: boolean;
    custom_save_dir: boolean;
    adapter_override_count: number;
  };
  recent_errors: string[];
}

// Matches Rust WindowBackdrop (returned by apply_window_theme)
export interface WindowBackdrop {
  mica: boolean;
}

export const PROGRESS_EVENT = 'download-progress';
export const UPDATE_AVAILABLE_EVENT = 'update-available';
export const NETWORK_ADAPTERS_CHANGED_EVENT = 'network-adapters-changed';

export type ViewId = 'all' | 'active' | 'paused' | 'completed' | 'failed' | 'network' | 'settings';
