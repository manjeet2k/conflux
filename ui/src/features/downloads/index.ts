// src/features/downloads/index.ts
export { downloadsApi } from './api/downloads.api';
export type { StartDownloadArgs } from './api/downloads.api';
export { AddDownloadDialog } from './components/AddDownloadDialog';
export { ChunkMap } from './components/ChunkMap';
export type { ChunkMapColors } from './components/ChunkMap';
export { CommandBar } from './components/CommandBar';
export { DetailsPane } from './components/DetailsPane';
export { DownloadProgress } from './components/DownloadProgress';
export { DownloadTable } from './components/DownloadTable';
export type { RowActions } from './components/DownloadTable';
export { DownloadsView } from './components/DownloadsView';
export type { DownloadsViewProps } from './components/DownloadsView';
export { RemoveDialog } from './components/RemoveDialog';
export { SpeedGraph } from './components/SpeedGraph';
export type { Series } from './components/SpeedGraph';
export { SpeedSparkline } from './components/SpeedSparkline';
export { useAddDownload } from './hooks/useAddDownload';
export type { UseAddDownloadOptions } from './hooks/useAddDownload';
export { useDownloads } from './hooks/useDownloads';
export type { UseDownloadsReturn } from './hooks/useDownloads';
export { useSpeedHistory, HISTORY_SECONDS, adapterKey } from './hooks/useSpeedHistory';
export type { SpeedHistory, SpeedSample } from './hooks/useSpeedHistory';
export type {
  AdapterStat,
  DownloadQueue,
  DownloadRequest,
  DownloadTask,
  ExternalDownloadPayload,
  ProbeResult,
  RequestHeaders,
  TaskStatus,
  ViewId,
} from './types/downloads.types';
export {
  advanceDownload,
  enqueueDownload,
  presentQueuedDownload,
  ManageQueueUseCase,
} from './utils/downloadQueue';
export {
  DownloadError,
  StartDownloadUseCase,
} from './utils/downloadValidation';
export type { DownloadErrorKind, DownloadRepository } from './utils/downloadValidation';
export { sortTasks } from './utils/sort';
export type { SortKey, SortState } from './utils/sort';
export {
  buildSmoothPath,
  dampCeiling,
  normalizeSpeedSamples,
  speedWindowStats,
} from './utils/sparkline';
