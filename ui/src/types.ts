export type AdapterType = 'ethernet' | 'wifi' | 'cellular';

export interface NetworkAdapterState {
  id: string;
  name: string;
  ip: string;
  type: AdapterType;
  enabled: boolean;
  currentSpeedBytesSec: number;
  totalBytesDownloaded: number;
  activeSockets: number;
}

export type TaskStatus = 'downloading' | 'paused' | 'completed' | 'error';

export interface ChunkVisual {
  id: number;
  start: number;
  end: number;
  status: 'pending' | 'downloading' | 'completed';
  adapterType?: AdapterType;
}

export interface DownloadTask {
  id: string;
  filename: string;
  url: string;
  mirrors: string[];
  totalBytes: number;
  downloadedBytes: number;
  status: TaskStatus;
  currentSpeedBytesSec: number;
  etaSeconds: number;
  chunks: ChunkVisual[];
  adapterBreakdown: {
    ethernet: number;
    wifi: number;
    cellular: number;
  };
  savePath: string;
  sha256?: string;
}

export type CategoryFilter = 'all' | 'downloading' | 'completed' | 'paused';
