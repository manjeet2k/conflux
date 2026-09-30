import React from 'react';
import type { DownloadTask } from '../types';
import { Layers, ShieldCheck, HardDrive, ExternalLink } from 'lucide-react';
import { openPath } from '@tauri-apps/plugin-opener';

interface InspectionDrawerProps {
  task: DownloadTask | null;
}

export const InspectionDrawer: React.FC<InspectionDrawerProps> = ({ task }) => {
  if (!task) {
    return (
      <div className="h-44 bg-fluent-subnav/90 border-t border-fluent-border flex items-center justify-center text-xs text-neutral-500">
        Select a download above to inspect its real-time multi-adapter chunk map
      </div>
    );
  }

  const handleOpenFolder = async () => {
    try {
      await openPath(task.savePath);
    } catch (e) {
      console.error('Failed to open file path:', e);
    }
  };

  const totalChunks = Math.max(1, task.totalChunks || 16);
  const completedCount = task.completedChunks;
  const activeCount = task.activeChunks;

  return (
    <div className="h-52 bg-fluent-subnav/95 border-t border-fluent-border p-3.5 flex flex-col justify-between select-none">
      <div>
        {/* Drawer Header & Legend */}
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center space-x-2">
            <Layers className="w-4 h-4 text-cyan-400" />
            <span className="text-xs font-semibold text-white">Visual Chunk Map</span>
            <span className="text-2xs text-neutral-400 font-mono">
              ({task.status === 'completed' ? totalChunks : completedCount}/{totalChunks} blocks completed)
            </span>
          </div>

          {/* Color Legend */}
          <div className="flex items-center space-x-3 text-3xs">
            <div className="flex items-center space-x-1">
              <span className="w-2.5 h-2.5 rounded-sm bg-cyan-400" />
              <span className="text-neutral-300">Completed</span>
            </div>
            <div className="flex items-center space-x-1">
              <span className="w-2.5 h-2.5 rounded-sm bg-white animate-pulse" />
              <span className="text-neutral-300">Active</span>
            </div>
            <div className="flex items-center space-x-1">
              <span className="w-2.5 h-2.5 rounded-sm bg-neutral-800/80 border border-white/10" />
              <span className="text-neutral-300">Pending</span>
            </div>
          </div>
        </div>

        {/* The Visual Segmented Chunk Grid */}
        <div className="p-2 rounded-lg bg-black/50 border border-white/5 overflow-hidden">
          <div className="grid grid-flow-col auto-cols-fr gap-1 h-9 items-center">
            {Array.from({ length: totalChunks }).map((_, idx) => {
              const isCompleted = task.status === 'completed' || idx < completedCount;
              const isActive = !isCompleted && idx < completedCount + activeCount && task.status === 'downloading';

              return (
                <div
                  key={idx}
                  className={`h-full rounded-sm border transition-all duration-200 ${
                    isCompleted
                      ? 'bg-cyan-400 border-cyan-300 shadow-sm shadow-cyan-500/30'
                      : isActive
                      ? 'bg-white animate-pulse border-white'
                      : 'bg-neutral-800/80 border-white/5'
                  }`}
                  title={`Chunk #${idx} - ${isCompleted ? 'Completed' : isActive ? 'Downloading' : 'Pending'}`}
                />
              );
            })}
          </div>
        </div>
      </div>

      {/* Detail Footer: File location & Hash */}
      <div className="flex items-center justify-between text-2xs pt-2 border-t border-white/5 text-neutral-400">
        <div className="flex items-center space-x-4">
          <button
            onClick={handleOpenFolder}
            className="flex items-center space-x-1.5 font-mono truncate max-w-md hover:text-cyan-400 transition"
            title="Open folder in File Explorer"
          >
            <HardDrive className="w-3.5 h-3.5 text-neutral-400" />
            <span className="truncate">{task.savePath}</span>
            <ExternalLink className="w-3 h-3 text-neutral-500" />
          </button>
        </div>

        {task.sha256 ? (
          <div className="flex items-center space-x-1 text-emerald-400 font-mono text-3xs">
            <ShieldCheck className="w-3.5 h-3.5" />
            <span className="truncate max-w-[280px]" title={task.sha256}>
              SHA-256: {task.sha256}
            </span>
          </div>
        ) : task.status === 'downloading' ? (
          <span className="text-neutral-500 font-mono text-3xs">Verifying chunks in real-time...</span>
        ) : null}
      </div>
    </div>
  );
};
