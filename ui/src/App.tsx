import React, { useState } from 'react';
import { TitleBar } from './components/TitleBar';
import { Sidebar } from './components/Sidebar';
import { DownloadList } from './components/DownloadList';
import { InspectionDrawer } from './components/InspectionDrawer';
import { NewDownloadModal } from './components/NewDownloadModal';
import type { CategoryFilter, DownloadTask, NetworkAdapterState } from './types';

// Mock initial data showcasing channel bonding across Ethernet + Wi-Fi + 4G
const initialAdapters: NetworkAdapterState[] = [
  {
    id: 'eth0',
    name: 'Realtek PCIe GbE Family Controller',
    ip: '192.168.1.100',
    type: 'ethernet',
    enabled: true,
    currentSpeedBytesSec: 88.5 * 1024 * 1024,
    totalBytesDownloaded: 14.2 * 1024 * 1024 * 1024,
    activeSockets: 8,
  },
  {
    id: 'wlan0',
    name: 'Intel(R) Wi-Fi 6 AX200 160MHz',
    ip: '192.168.0.50',
    type: 'wifi',
    enabled: true,
    currentSpeedBytesSec: 42.1 * 1024 * 1024,
    totalBytesDownloaded: 6.8 * 1024 * 1024 * 1024,
    activeSockets: 4,
  },
  {
    id: 'usb0',
    name: 'Remote NDIS 5G Cellular Device',
    ip: '192.168.42.10',
    type: 'cellular',
    enabled: true,
    currentSpeedBytesSec: 24.3 * 1024 * 1024,
    totalBytesDownloaded: 2.1 * 1024 * 1024 * 1024,
    activeSockets: 4,
  },
];

const generateMockChunks = (totalChunks = 48) => {
  const chunks = [];
  const types: ('ethernet' | 'wifi' | 'cellular')[] = ['ethernet', 'wifi', 'cellular'];

  for (let i = 0; i < totalChunks; i++) {
    const isCompleted = i < 34;
    const isDownloading = i >= 34 && i < 38;
    const status = isCompleted ? 'completed' : isDownloading ? 'downloading' : 'pending';
    const adapterType = isCompleted || isDownloading ? types[i % 3] : undefined;

    chunks.push({
      id: i,
      start: i * 100 * 1024 * 1024,
      end: (i + 1) * 100 * 1024 * 1024 - 1,
      status: status as 'completed' | 'downloading' | 'pending',
      adapterType,
    });
  }
  return chunks;
};

const initialTasks: DownloadTask[] = [
  {
    id: 'task-1',
    filename: 'ubuntu-24.04-desktop-amd64.iso',
    url: 'https://releases.ubuntu.com/noble/ubuntu-24.04-desktop-amd64.iso',
    mirrors: ['https://mirror.us.kernel.org/ubuntu/noble.iso', 'https://cloudflare.mirror.org/ubuntu.iso'],
    totalBytes: 4.8 * 1024 * 1024 * 1024,
    downloadedBytes: 3.4 * 1024 * 1024 * 1024,
    status: 'downloading',
    currentSpeedBytesSec: 154.9 * 1024 * 1024,
    etaSeconds: 9,
    chunks: generateMockChunks(48),
    adapterBreakdown: {
      ethernet: 88.5 * 1024 * 1024,
      wifi: 42.1 * 1024 * 1024,
      cellular: 24.3 * 1024 * 1024,
    },
    savePath: 'C:\\Downloads\\ubuntu-24.04-desktop-amd64.iso',
    sha256: '2a6a199e1031a5c279cb346646d594993f35b1c03dd4a82aaa0323980dd92451',
  },
  {
    id: 'task-2',
    filename: 'unreal_engine_5.5_setup.exe',
    url: 'https://launcher-public-service-prod.ol.epicgames.com/UE5.exe',
    mirrors: [],
    totalBytes: 18.5 * 1024 * 1024 * 1024,
    downloadedBytes: 18.5 * 1024 * 1024 * 1024,
    status: 'completed',
    currentSpeedBytesSec: 0,
    etaSeconds: 0,
    chunks: generateMockChunks(60).map((c) => ({ ...c, status: 'completed' })),
    adapterBreakdown: {
      ethernet: 0,
      wifi: 0,
      cellular: 0,
    },
    savePath: 'C:\\Downloads\\unreal_engine_5.5_setup.exe',
    sha256: '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08',
  },
];

export const App: React.FC = () => {
  const [adapters, setAdapters] = useState<NetworkAdapterState[]>(initialAdapters);
  const [tasks, setTasks] = useState<DownloadTask[]>(initialTasks);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>('task-1');
  const [currentCategory, setCurrentCategory] = useState<CategoryFilter>('all');
  const [isModalOpen, setIsModalOpen] = useState(false);

  // Calculate global total speed across all active downloads
  const totalSpeed = tasks
    .filter((t) => t.status === 'downloading')
    .reduce((sum, t) => sum + t.currentSpeedBytesSec, 0);

  // Category counts
  const taskCounts = {
    all: tasks.length,
    downloading: tasks.filter((t) => t.status === 'downloading').length,
    completed: tasks.filter((t) => t.status === 'completed').length,
    paused: tasks.filter((t) => t.status === 'paused').length,
  };

  // Filter tasks based on selected category
  const filteredTasks = tasks.filter((t) => {
    if (currentCategory === 'all') return true;
    return t.status === currentCategory;
  });

  const selectedTask = tasks.find((t) => t.id === selectedTaskId) || null;

  // Toggle adapter enablement
  const handleToggleAdapter = (adapterId: string) => {
    setAdapters((prev) =>
      prev.map((a) => {
        if (a.id === adapterId) {
          const nextEnabled = !a.enabled;
          return {
            ...a,
            enabled: nextEnabled,
            currentSpeedBytesSec: nextEnabled ? a.currentSpeedBytesSec : 0,
          };
        }
        return a;
      })
    );
  };

  // Pause a task
  const handlePauseTask = (taskId: string) => {
    setTasks((prev) =>
      prev.map((t) => (t.id === taskId ? { ...t, status: 'paused', currentSpeedBytesSec: 0 } : t))
    );
  };

  // Resume a task
  const handleResumeTask = (taskId: string) => {
    setTasks((prev) =>
      prev.map((t) =>
        t.id === taskId
          ? {
              ...t,
              status: 'downloading',
              currentSpeedBytesSec: 154.9 * 1024 * 1024,
            }
          : t
      )
    );
  };

  // Delete a task
  const handleDeleteTask = (taskId: string) => {
    setTasks((prev) => prev.filter((t) => t.id !== taskId));
    if (selectedTaskId === taskId) {
      setSelectedTaskId(null);
    }
  };

  // Global pause/resume all
  const handlePauseAll = () => {
    setTasks((prev) =>
      prev.map((t) => (t.status === 'downloading' ? { ...t, status: 'paused', currentSpeedBytesSec: 0 } : t))
    );
  };

  const handleResumeAll = () => {
    setTasks((prev) =>
      prev.map((t) =>
        t.status === 'paused'
          ? { ...t, status: 'downloading', currentSpeedBytesSec: 120 * 1024 * 1024 }
          : t
      )
    );
  };

  // Start a new download
  const handleStartDownload = (url: string, mirrors: string[]) => {
    const filename = url.split('/').pop() || 'download.bin';
    const newTask: DownloadTask = {
      id: `task-${Date.now()}`,
      filename,
      url,
      mirrors,
      totalBytes: 2.4 * 1024 * 1024 * 1024,
      downloadedBytes: 0,
      status: 'downloading',
      currentSpeedBytesSec: 110.5 * 1024 * 1024,
      etaSeconds: 22,
      chunks: generateMockChunks(32).map((c, i) => ({
        ...c,
        status: i < 2 ? 'downloading' : 'pending',
        adapterType: i < 2 ? 'ethernet' : undefined,
      })),
      adapterBreakdown: {
        ethernet: 75 * 1024 * 1024,
        wifi: 35.5 * 1024 * 1024,
        cellular: 0,
      },
      savePath: `C:\\Downloads\\${filename}`,
    };

    setTasks((prev) => [newTask, ...prev]);
    setSelectedTaskId(newTask.id);
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
          onToggleAdapter={handleToggleAdapter}
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
