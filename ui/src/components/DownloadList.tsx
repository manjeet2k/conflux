import React from 'react';
import { Play, Pause, Trash2, FileText, CheckCircle2, AlertCircle } from 'lucide-react';
import type { DownloadTask } from '../types';
import { formatBytes, formatSpeed, formatEta } from '../utils/formatters';

interface DownloadListProps {
  tasks: DownloadTask[];
  selectedTaskId: string | null;
  onSelectTask: (taskId: string) => void;
  onPauseTask: (taskId: string) => void;
  onResumeTask: (taskId: string) => void;
  onDeleteTask: (taskId: string) => void;
}

export const DownloadList: React.FC<DownloadListProps> = ({
  tasks,
  selectedTaskId,
  onSelectTask,
  onPauseTask,
  onResumeTask,
  onDeleteTask,
}) => {
  if (tasks.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center text-neutral-500 p-8 select-none">
        <FileText className="w-12 h-12 stroke-[1.2] mb-3 text-neutral-600" />
        <p className="text-sm font-medium">No downloads in this category</p>
        <p className="text-xs text-neutral-600 mt-1">Click "+ New Download" to start a multi-interface transfer</p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-2.5">
      {tasks.map((task) => {
        const isSelected = selectedTaskId === task.id;
        const percent = task.totalBytes > 0 ? (task.downloadedBytes / task.totalBytes) * 100 : 0;

        return (
          <div
            key={task.id}
            onClick={() => onSelectTask(task.id)}
            className={`p-3.5 rounded-xl border transition cursor-pointer ${
              isSelected
                ? 'bg-fluent-card-selected border-cyan-500/50 shadow-fluent-card'
                : 'bg-fluent-card hover:bg-fluent-card-hover border-fluent-border'
            }`}
          >
            {/* Top row: Filename, Status Badge, Actions */}
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center space-x-2.5 overflow-hidden pr-4">
                <div className="p-2 rounded-lg bg-black/40 border border-white/5 text-cyan-400">
                  <FileText className="w-4 h-4" />
                </div>
                <div className="overflow-hidden">
                  <h4 className="text-xs font-semibold text-white truncate" title={task.filename}>
                    {task.filename}
                  </h4>
                  <p className="text-3xs text-neutral-400 truncate max-w-md font-mono" title={task.url}>
                    {task.url}
                  </p>
                </div>
              </div>

              {/* Status and Action Buttons */}
              <div className="flex items-center space-x-2">
                {task.status === 'downloading' && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onPauseTask(task.id);
                    }}
                    className="p-1.5 hover:bg-white/10 rounded-md text-amber-400 transition"
                    title="Pause"
                  >
                    <Pause className="w-3.5 h-3.5" />
                  </button>
                )}

                {task.status === 'paused' && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onResumeTask(task.id);
                    }}
                    className="p-1.5 hover:bg-white/10 rounded-md text-emerald-400 transition"
                    title="Resume"
                  >
                    <Play className="w-3.5 h-3.5" />
                  </button>
                )}

                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteTask(task.id);
                  }}
                  className="p-1.5 hover:bg-white/10 rounded-md text-neutral-400 hover:text-red-400 transition"
                  title="Remove"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            {/* Progress Bar */}
            <div className="w-full bg-black/50 rounded-full h-2 overflow-hidden mb-2 border border-white/5">
              <div
                className={`h-full transition-all duration-300 rounded-full ${
                  task.status === 'completed'
                    ? 'bg-emerald-500'
                    : task.status === 'paused'
                    ? 'bg-amber-500'
                    : task.status === 'error'
                    ? 'bg-rose-500'
                    : 'bg-gradient-to-r from-cyan-500 to-fuchsia-500'
                }`}
                style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
              />
            </div>

            {/* Metrics row */}
            <div className="flex items-center justify-between text-2xs text-neutral-400">
              <div className="flex items-center space-x-3">
                <span className="font-semibold text-white font-mono">{percent.toFixed(1)}%</span>
                <span>
                  {formatBytes(task.downloadedBytes)} / {formatBytes(task.totalBytes)}
                </span>
                {task.status === 'downloading' && (
                  <>
                    <span className="text-cyan-400 font-mono font-semibold">
                      ⚡ {formatSpeed(task.currentSpeedBytesSec)}
                    </span>
                    <span>ETA: {formatEta(task.etaSeconds)}</span>
                  </>
                )}
                {task.status === 'completed' && (
                  <span className="text-emerald-400 font-medium flex items-center space-x-1">
                    <CheckCircle2 className="w-3 h-3" />
                    <span>Finished</span>
                  </span>
                )}
                {task.status === 'error' && (
                  <span className="text-rose-400 font-medium flex items-center space-x-1">
                    <AlertCircle className="w-3 h-3" />
                    <span>{task.error || 'Failed'}</span>
                  </span>
                )}
              </div>

              {task.totalChunks > 0 && (
                <div className="font-mono text-3xs text-neutral-500">
                  Chunks: {task.completedChunks}/{task.totalChunks} (Active: {task.activeChunks})
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
};
