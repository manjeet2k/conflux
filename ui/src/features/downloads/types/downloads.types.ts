// src/features/downloads/types/downloads.types.ts

export type TaskStatus = 'downloading' | 'stopping' | 'paused' | 'completed' | 'error';

export interface RequestHeaders {
  cookie?: string | null;
  referer?: string | null;
  user_agent?: string | null;
}

export interface AdapterStat {
  adapter_id: string | null;
  name: string;
  ip: string | null;
  downloaded_bytes: number;
  speed_bytes_sec: number;
  active_connections: number;
  dropped: boolean;
  last_error?: string | null;
  drop_reason?: string | null;
}

export interface DownloadTask {
  id: string;
  url: string;
  filename: string;
  save_dir: string;
  save_path: string;
  final_path?: string | null;
  file_identity?: { volume: number; file: number } | null;
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
  duration_ms?: number;
  avg_speed_bytes_sec?: number | null;
  adapters: AdapterStat[];
  chunk_map: string | null;
}

export interface ProbeResult {
  url: string;
  filename: string;
  total_bytes: number;
  supports_ranges: boolean;
}

export interface ExternalDownloadPayload {
  url: string;
  handoff_id?: string | null;
  filename?: string | null;
  headers?: RequestHeaders | null;
}

export interface DownloadRequest {
  id: string;
  payload: ExternalDownloadPayload;
}

export interface DownloadQueue {
  current: DownloadRequest | null;
  queued: DownloadRequest[];
}

export type ViewId = 'all' | 'active' | 'paused' | 'completed' | 'failed' | 'network' | 'settings';
