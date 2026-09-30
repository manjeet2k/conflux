// Matches Rust AdapterInfo struct from commands.rs
export interface AdapterInfo {
  id: string;
  name: string;
  ip: string;
  is_ipv4: boolean;
  is_loopback: boolean;
  enabled: boolean;
}

// Matches Rust ProbeResult struct
export interface ProbeResult {
  url: string;
  filename: string;
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
  status: string;
  sha256: string | null;
  error: string | null;
}

export type TaskStatus = 'downloading' | 'paused' | 'completed' | 'error';

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
  sha256?: string;
  error?: string;
}

export type CategoryFilter = 'all' | 'downloading' | 'completed' | 'paused';
