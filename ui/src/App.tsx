import React, { useState, useEffect, useRef, useCallback } from 'react';
import { invoke, Channel } from '@tauri-apps/api/core';
import { AlertCircle, X } from 'lucide-react';
import { TitleBar } from './components/TitleBar';
import { Sidebar } from './components/Sidebar';
import { DownloadList } from './components/DownloadList';
import { InspectionDrawer } from './components/InspectionDrawer';
import { NewDownloadModal } from './components/NewDownloadModal';
import type {
  AdapterInfo,
  CategoryFilter,
  DownloadTask,
  DownloadTaskState,
  ProgressEvent,
  StartedDownload,
  TaskStatus,
} from './types';

const isFinalStatus = (status: TaskStatus) => status !== 'downloading';

/** Pure reducer: fold one backend progress event into a task. */
function applyProgressEvent(task: DownloadTask, event: ProgressEvent): DownloadTask {
  switch (event.status) {
    case 'completed': {
      const total = event.total_bytes || task.totalBytes;
      const totalChunks = event.total_chunks || task.totalChunks;
      return {
        ...task,
        status: 'completed',
        totalBytes: total,
        downloadedBytes: event.downloaded_bytes || total,
        currentSpeedBytesSec: 0,
        etaSeconds: 0,
        activeChunks: 0,
        totalChunks,
        completedChunks: event.completed_chunks || totalChunks,
        sha256: event.sha256 ?? task.sha256,
        error: undefined,
      };
    }
    case 'error':
      return {
        ...task,
        status: 'error',
        currentSpeedBytesSec: 0,
        etaSeconds: 0,
        activeChunks: 0,
        error: event.error || 'Download failed',
      };
    case 'stopped':
      return {
        ...task,
        status: 'stopped',
        downloadedBytes: event.downloaded_bytes || task.downloadedBytes,
        currentSpeedBytesSec: 0,
        etaSeconds: 0,
        activeChunks: 0,
      };
    case 'downloading':
      return {
        ...task,
        status: 'downloading',
        totalBytes: event.total_bytes || task.totalBytes,
        downloadedBytes: event.downloaded_bytes,
        currentSpeedBytesSec: event.speed_bytes_sec,
        etaSeconds: event.eta_seconds,
        activeChunks: event.active_chunks,
        completedChunks: event.completed_chunks,
        totalChunks: event.total_chunks,
      };
  }
}

function taskFromState(s: DownloadTaskState): DownloadTask {
  return {
    id: s.id,
    filename: s.filename,
    url: s.url,
    totalBytes: s.total_bytes,
    downloadedBytes: s.downloaded_bytes,
    status: s.status,
    currentSpeedBytesSec: s.speed_bytes_sec,
    etaSeconds: s.eta_seconds,
    activeChunks: s.active_chunks,
    completedChunks: s.completed_chunks,
    totalChunks: s.total_chunks,
    savePath: s.save_path,
    saveDir: s.save_dir,
    adapterIds: s.adapter_ids,
    sha256: s.sha256 ?? undefined,
    error: s.error ?? undefined,
  };
}

export const App: React.FC = () => {
  const [adapters, setAdapters] = useState<AdapterInfo[]>([]);
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [currentCategory, setCurrentCategory] = useState<CategoryFilter>('all');
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Task ids that exist in `tasks`. Progress events can arrive before `start_download`
  // resolves (i.e. before the task is inserted), so events for unknown ids are buffered
  // in `pendingEvents` (latest per id; a final event is never overwritten) and applied
  // when the task is inserted.
  const knownTaskIds = useRef(new Set<string>());
  const pendingEvents = useRef(new Map<string, ProgressEvent>());
  // Ids removed by the user; late events for them are dropped instead of buffered forever.
  const removedTaskIds = useRef(new Set<string>());

  const refreshAdapters = useCallback(() => {
    invoke<AdapterInfo[]>('discover_adapters')
      .then(setAdapters)
      .catch((e) => {
        console.error('Failed to discover network adapters:', e);
        setErrorMessage(`Failed to discover network adapters: ${String(e)}`);
      });
  }, []);

  // Startup: discover adapters and restore any tasks the backend already knows about
  // (e.g. after a webview reload).
  useEffect(() => {
    refreshAdapters();
    invoke<DownloadTaskState[]>('list_tasks')
      .then((states) => {
        const restored = states.map(taskFromState);
        restored.forEach((t) => knownTaskIds.current.add(t.id));
        setTasks((prev) => {
          const existing = new Set(prev.map((t) => t.id));
          return [...prev, ...restored.filter((t) => !existing.has(t.id))];
        });
      })
      .catch((e) => console.error('Failed to list tasks:', e));
  }, [refreshAdapters]);

  const handleProgressEvent = useCallback((event: ProgressEvent) => {
    if (removedTaskIds.current.has(event.task_id)) return;
    if (!knownTaskIds.current.has(event.task_id)) {
      const buffered = pendingEvents.current.get(event.task_id);
      if (!buffered || !isFinalStatus(buffered.status)) {
        pendingEvents.current.set(event.task_id, event);
      }
      return;
    }
    setTasks((prev) => prev.map((t) => (t.id === event.task_id ? applyProgressEvent(t, event) : t)));
  }, []);

  /** Starts a download and inserts its task. Throws the backend error message on failure. */
  const startTask = useCallback(
    async (url: string, saveDir: string, adapterIds: string[]): Promise<string> => {
      const onProgress = new Channel<ProgressEvent>();
      onProgress.onmessage = handleProgressEvent;

      const started = await invoke<StartedDownload>('start_download', {
        url,
        saveDir,
        adapterIds,
        onProgress,
      });

      let newTask: DownloadTask = {
        id: started.task_id,
        filename: started.filename,
        url,
        totalBytes: started.total_bytes,
        downloadedBytes: 0,
        status: 'downloading',
        currentSpeedBytesSec: 0,
        etaSeconds: 0,
        activeChunks: 0,
        completedChunks: 0,
        totalChunks: 0,
        savePath: started.save_path,
        saveDir,
        adapterIds,
      };

      const buffered = pendingEvents.current.get(started.task_id);
      if (buffered) {
        pendingEvents.current.delete(started.task_id);
        newTask = applyProgressEvent(newTask, buffered);
      }
      knownTaskIds.current.add(started.task_id);
      setTasks((prev) => [newTask, ...prev]);
      return started.task_id;
    },
    [handleProgressEvent]
  );

  const totalSpeed = tasks
    .filter((t) => t.status === 'downloading')
    .reduce((sum, t) => sum + t.currentSpeedBytesSec, 0);

  const taskCounts = {
    all: tasks.length,
    downloading: tasks.filter((t) => t.status === 'downloading').length,
    completed: tasks.filter((t) => t.status === 'completed').length,
    stopped: tasks.filter((t) => t.status === 'stopped').length,
  };

  const filteredTasks = tasks.filter((t) => {
    if (currentCategory === 'all') return true;
    return t.status === currentCategory;
  });

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) || null;

  /** Returns an error message for the modal to display, or null on success. */
  const handleStartDownload = async (
    url: string,
    adapterIds: string[],
    saveDir: string
  ): Promise<string | null> => {
    try {
      const taskId = await startTask(url, saveDir, adapterIds);
      setSelectedTaskId(taskId);
      return null;
    } catch (e) {
      console.error('Failed to start download:', e);
      return `Failed to start download: ${String(e)}`;
    }
  };

  const handleStopTask = async (taskId: string) => {
    try {
      const status = await invoke<TaskStatus>('pause_download', { taskId });
      setTasks((prev) =>
        prev.map((t) =>
          t.id === taskId && t.status === 'downloading'
            ? { ...t, status, currentSpeedBytesSec: 0, etaSeconds: 0, activeChunks: 0 }
            : t
        )
      );
    } catch (e) {
      console.error('Failed to stop download:', e);
      setErrorMessage(`Failed to stop download: ${String(e)}`);
    }
  };

  const removeTaskLocally = (taskId: string) => {
    knownTaskIds.current.delete(taskId);
    removedTaskIds.current.add(taskId);
    pendingEvents.current.delete(taskId);
    setTasks((prev) => prev.filter((t) => t.id !== taskId));
    setSelectedTaskId((sel) => (sel === taskId ? null : sel));
  };

  /** No resume yet: drop the old task (and its partial file), then download again from zero. */
  const handleRestartTask = async (taskId: string) => {
    const old = tasks.find((t) => t.id === taskId);
    if (!old) return;
    try {
      await invoke('cancel_download', { taskId });
    } catch (e) {
      setErrorMessage(`Failed to restart download: ${String(e)}`);
      return;
    }
    removeTaskLocally(taskId);
    try {
      const newId = await startTask(old.url, old.saveDir, old.adapterIds);
      setSelectedTaskId((sel) => (sel === null || sel === taskId ? newId : sel));
    } catch (e) {
      console.error('Failed to restart download:', e);
      setErrorMessage(`Failed to restart ${old.filename}: ${String(e)}`);
    }
  };

  const handleDeleteTask = async (taskId: string) => {
    try {
      await invoke('cancel_download', { taskId });
      removeTaskLocally(taskId);
    } catch (e) {
      console.error('Failed to remove download:', e);
      setErrorMessage(`Failed to remove download: ${String(e)}`);
    }
  };

  const handleStopAll = () => {
    tasks.filter((t) => t.status === 'downloading').forEach((t) => handleStopTask(t.id));
  };

  const handleRestartAll = () => {
    tasks.filter((t) => t.status === 'stopped').forEach((t) => handleRestartTask(t.id));
  };

  const handleOpenModal = () => {
    refreshAdapters();
    setIsModalOpen(true);
  };

  return (
    <div className="flex flex-col h-screen bg-fluent-bg text-neutral-100 font-sans overflow-hidden">
      {/* TitleBar */}
      <TitleBar
        totalSpeed={totalSpeed}
        onOpenNewModal={handleOpenModal}
        onRestartAll={handleRestartAll}
        onStopAll={handleStopAll}
        canRestartAll={taskCounts.stopped > 0}
        canStopAll={taskCounts.downloading > 0}
      />

      {/* Inline error banner */}
      {errorMessage && (
        <div
          role="alert"
          className="flex items-start justify-between gap-3 px-4 py-2 bg-rose-500/10 border-b border-rose-500/30 text-xs text-rose-300"
        >
          <div className="flex items-start gap-2 min-w-0">
            <AlertCircle className="w-4 h-4 shrink-0 mt-px" />
            <span className="break-words select-text">{errorMessage}</span>
          </div>
          <button
            onClick={() => setErrorMessage(null)}
            className="p-0.5 rounded hover:bg-white/10 text-rose-300 hover:text-white shrink-0"
            title="Dismiss"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* Main Workspace Layout */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <Sidebar
          currentCategory={currentCategory}
          onSelectCategory={setCurrentCategory}
          taskCounts={taskCounts}
          adapters={adapters}
        />

        {/* Central Stage & Inspection Drawer */}
        <main className="flex-1 flex flex-col bg-fluent-bg overflow-hidden">
          <DownloadList
            tasks={filteredTasks}
            selectedTaskId={selectedTaskId}
            onSelectTask={setSelectedTaskId}
            onStopTask={handleStopTask}
            onRestartTask={handleRestartTask}
            onDeleteTask={handleDeleteTask}
          />

          <InspectionDrawer task={selectedTask} />
        </main>
      </div>

      {/* New Download Modal */}
      <NewDownloadModal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        adapters={adapters}
        onStartDownload={handleStartDownload}
      />
    </div>
  );
};

export default App;
