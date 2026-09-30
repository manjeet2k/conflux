import React, { useState, useEffect } from 'react';
import { invoke, Channel } from '@tauri-apps/api/core';
import { TitleBar } from './components/TitleBar';
import { Sidebar } from './components/Sidebar';
import { DownloadList } from './components/DownloadList';
import { InspectionDrawer } from './components/InspectionDrawer';
import { NewDownloadModal } from './components/NewDownloadModal';
import type { AdapterInfo, CategoryFilter, DownloadTask, ProgressEvent, ProbeResult } from './types';

export const App: React.FC = () => {
  const [adapters, setAdapters] = useState<AdapterInfo[]>([]);
  const [tasks, setTasks] = useState<DownloadTask[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [currentCategory, setCurrentCategory] = useState<CategoryFilter>('all');
  const [isModalOpen, setIsModalOpen] = useState(false);

  // Fetch real adapters from Rust backend on startup
  useEffect(() => {
    invoke<AdapterInfo[]>('discover_adapters')
      .then((data) => {
        setAdapters(data);
      })
      .catch((e) => {
        console.error('Failed to discover network adapters:', e);
      });
  }, []);

  const totalSpeed = tasks
    .filter((t) => t.status === 'downloading')
    .reduce((sum, t) => sum + t.currentSpeedBytesSec, 0);

  const taskCounts = {
    all: tasks.length,
    downloading: tasks.filter((t) => t.status === 'downloading').length,
    completed: tasks.filter((t) => t.status === 'completed').length,
    paused: tasks.filter((t) => t.status === 'paused').length,
  };

  const filteredTasks = tasks.filter((t) => {
    if (currentCategory === 'all') return true;
    return t.status === currentCategory;
  });

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) || null;

  const handleStartDownload = async (
    url: string,
    _mirrors: string[],
    _selectedAdapterIds: string[],
    saveDir: string
  ) => {
    try {
      // 1. Probe target URL to obtain file metadata immediately
      const probe = await invoke<ProbeResult>('probe_url', { url });

      // 2. Set up high-frequency streaming channel for real-time chunk progress
      const onProgress = new Channel<ProgressEvent>();

      onProgress.onmessage = (event) => {
        setTasks((prev) =>
          prev.map((t) => {
            if (t.id !== event.task_id) return t;

            if (event.status === 'completed') {
              return {
                ...t,
                status: 'completed',
                downloadedBytes: t.totalBytes,
                currentSpeedBytesSec: 0,
                etaSeconds: 0,
                completedChunks: event.total_chunks || t.totalChunks,
                sha256: event.sha256 || t.sha256,
              };
            }

            if (event.status === 'error') {
              return {
                ...t,
                status: 'error',
                currentSpeedBytesSec: 0,
                error: event.error || 'Download failed',
              };
            }

            return {
              ...t,
              downloadedBytes: event.downloaded_bytes,
              currentSpeedBytesSec: event.speed_bytes_sec,
              etaSeconds: event.eta_seconds,
              activeChunks: event.active_chunks,
              completedChunks: event.completed_chunks,
              totalChunks: event.total_chunks,
            };
          })
        );
      };

      // 3. Initiate accelerated multi-adapter download
      const taskId = await invoke<string>('start_download', {
        url,
        saveDir,
        onProgress,
      });

      const newTask: DownloadTask = {
        id: taskId,
        filename: probe.filename,
        url,
        totalBytes: probe.total_bytes,
        downloadedBytes: 0,
        status: 'downloading',
        currentSpeedBytesSec: 0,
        etaSeconds: 0,
        activeChunks: 0,
        completedChunks: 0,
        totalChunks: 0,
        savePath: `${saveDir}/${probe.filename}`,
      };

      setTasks((prev) => [newTask, ...prev]);
      setSelectedTaskId(taskId);
    } catch (e) {
      console.error('Failed to start download:', e);
    }
  };

  const handlePauseTask = async (taskId: string) => {
    try {
      await invoke('pause_download', { taskId });
      setTasks((prev) =>
        prev.map((t) =>
          t.id === taskId
            ? { ...t, status: 'paused', currentSpeedBytesSec: 0 }
            : t
        )
      );
    } catch (e) {
      console.error('Failed to pause download:', e);
    }
  };

  const handleResumeTask = async (taskId: string) => {
    try {
      await invoke('resume_download', { taskId });
    } catch (e) {
      console.warn('Resume notice:', e);
    }
  };

  const handleDeleteTask = async (taskId: string) => {
    try {
      await invoke('cancel_download', { taskId });
      setTasks((prev) => prev.filter((t) => t.id !== taskId));
      if (selectedTaskId === taskId) {
        setSelectedTaskId(null);
      }
    } catch (e) {
      console.error('Failed to cancel download:', e);
    }
  };

  const handlePauseAll = () => {
    tasks
      .filter((t) => t.status === 'downloading')
      .forEach((t) => handlePauseTask(t.id));
  };

  const handleResumeAll = () => {
    tasks
      .filter((t) => t.status === 'paused')
      .forEach((t) => handleResumeTask(t.id));
  };

  return (
    <div className="flex flex-col h-screen bg-fluent-bg text-neutral-100 font-sans overflow-hidden">
      {/* TitleBar */}
      <TitleBar
        totalSpeed={totalSpeed}
        onOpenNewModal={() => setIsModalOpen(true)}
        onResumeAll={handleResumeAll}
        onPauseAll={handlePauseAll}
      />

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
            onPauseTask={handlePauseTask}
            onResumeTask={handleResumeTask}
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
