// Matches Rust AdapterInfo struct from commands.rs
export interface AdapterInfo {
  id: string;
  name: string;
  ip: string;
  is_ipv4: boolean;
  is_loopback: boolean;
  enabled: boolean;
}

// Matches Rust TaskStatus enum (serde rename_all = "lowercase")
export type TaskStatus = 'downloading' | 'completed' | 'stopped' | 'error';

// Matches Rust StartedDownload struct (returned by start_download)
export interface StartedDownload {
  task_id: string;
  filename: string;
  save_path: string;
  total_bytes: number;
  supports_ranges: boolean;
}

// Matches Rust ProgressEvent struct
export interface ProgressEvent {
  task_id: string;
  downloaded_bytes: number;
  total_bytes: number;
  speed_bytes_sec: number;
  eta_seconds: number;
  active_chunks: number;
  completed_chunks: number;
  total_chunks: number;
  status: TaskStatus;
  sha256: string | null;
  error: string | null;
}

// Matches Rust DownloadTaskState struct (returned by list_tasks)
export interface DownloadTaskState {
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
}

export interface DownloadTask {
  id: string;
  filename: string;
  url: string;
  totalBytes: number;
  downloadedBytes: number;
  status: TaskStatus;
  currentSpeedBytesSec: number;
  etaSeconds: number;
  activeChunks: number;
  completedChunks: number;
  totalChunks: number;
  savePath: string;
  // Kept so a stopped/failed task can be restarted with the same parameters.
  saveDir: string;
  adapterIds: string[];
  sha256?: string;
  error?: string;
}

export type CategoryFilter = 'all' | 'downloading' | 'completed' | 'stopped';
