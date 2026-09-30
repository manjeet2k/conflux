// IPC contract with crates/conflux-desktop/src/commands.rs. Field names are the serde names.

export type AdapterKind = 'ethernet' | 'wifi' | 'cellular' | 'virtual' | 'loopback' | 'other';

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
}

// Matches Rust DownloadTaskState. Returned by list_tasks/start/pause/resume and emitted as
// the payload of the `download-progress` event (a full snapshot each time).
export interface DownloadTask {
  id: string;
  url: string;
  filename: string;
  save_dir: string;
  save_path: string;
  adapter_ids: string[];
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
  theme: ThemePreference;
  /** null = the OS Downloads folder. */
  default_save_dir: string | null;
  connections_per_adapter: number;
  chunk_size_mb: number;
  notify_on_complete: boolean;
  close_to_tray: boolean;
  auto_aggregate_adapters: boolean;
}

// Matches Rust WindowBackdrop (returned by apply_window_theme)
export interface WindowBackdrop {
  mica: boolean;
}

export const PROGRESS_EVENT = 'download-progress';
export const NETWORK_ADAPTERS_CHANGED_EVENT = 'network-adapters-changed';

export type ViewId = 'all' | 'active' | 'paused' | 'completed' | 'failed' | 'network' | 'settings';

