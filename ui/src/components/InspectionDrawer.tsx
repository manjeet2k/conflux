import React from 'react';
import type { DownloadTask } from '../types';
import { Layers, ShieldCheck, HardDrive, Link2 } from 'lucide-react';
import { formatBytes } from '../utils/formatters';

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

  const getChunkColor = (status: string, adapterType?: string) => {
    if (status === 'completed') {
      switch (adapterType) {
        case 'ethernet':
          return 'bg-cyan-400 border-cyan-300 shadow-sm shadow-cyan-500/30';
        case 'wifi':
          return 'bg-fuchsia-400 border-fuchsia-300 shadow-sm shadow-fuchsia-500/30';
        case 'cellular':
          return 'bg-amber-400 border-amber-300 shadow-sm shadow-amber-500/30';
        default:
          return 'bg-emerald-400 border-emerald-300';
      }
    }
    if (status === 'downloading') {
      return 'bg-white animate-pulse border-white';
    }
    return 'bg-neutral-800/80 border-white/5';
  };

  return (
    <div className="h-52 bg-fluent-subnav/95 border-t border-fluent-border p-3.5 flex flex-col justify-between select-none">
      <div>
        {/* Drawer Header & Legend */}
        <div className="flex items-center justify-between mb-2">
          <div className="flex items-center space-x-2">
            <Layers className="w-4 h-4 text-cyan-400" />
            <span className="text-xs font-semibold text-white">Visual Chunk Map</span>
            <span className="text-2xs text-neutral-400 font-mono">
              ({task.chunks.filter((c) => c.status === 'completed').length}/{task.chunks.length} blocks completed)
            </span>
          </div>

          {/* Color Legend */}
          <div className="flex items-center space-x-3 text-3xs">
            <div className="flex items-center space-x-1">
              <span className="w-2.5 h-2.5 rounded-sm bg-cyan-400" />
              <span className="text-neutral-300">Ethernet</span>
            </div>
            <div className="flex items-center space-x-1">
              <span className="w-2.5 h-2.5 rounded-sm bg-fuchsia-400" />
              <span className="text-neutral-300">Wi-Fi</span>
            </div>
            <div className="flex items-center space-x-1">
              <span className="w-2.5 h-2.5 rounded-sm bg-amber-400" />
              <span className="text-neutral-300">4G Cellular</span>
            </div>
            <div className="flex items-center space-x-1">
              <span className="w-2.5 h-2.5 rounded-sm bg-white animate-pulse" />
              <span className="text-neutral-300">Active</span>
            </div>
          </div>
        </div>

        {/* The Visual Segmented Chunk Grid */}
        <div className="p-2 rounded-lg bg-black/50 border border-white/5 overflow-hidden">
          <div className="grid grid-flow-col auto-cols-fr gap-1 h-9 items-center">
            {task.chunks.map((chunk) => (
              <div
                key={chunk.id}
                className={`h-full rounded-sm border transition-all duration-200 ${getChunkColor(
                  chunk.status,
                  chunk.adapterType
                )}`}
                title={`Chunk #${chunk.id} (${formatBytes(chunk.end - chunk.start + 1)}) - ${chunk.status} ${
                  chunk.adapterType ? `via ${chunk.adapterType.toUpperCase()}` : ''
                }`}
              />
            ))}
          </div>
        </div>
      </div>

      {/* Detail Footer: File location, Mirrors, Hash */}
      <div className="flex items-center justify-between text-2xs pt-2 border-t border-white/5 text-neutral-400">
        <div className="flex items-center space-x-4">
          <div className="flex items-center space-x-1 font-mono truncate max-w-xs">
            <HardDrive className="w-3 h-3 text-neutral-400" />
            <span className="truncate" title={task.savePath}>
              {task.savePath}
            </span>
          </div>

          <div className="flex items-center space-x-1">
            <Link2 className="w-3 h-3 text-neutral-400" />
            <span>{task.mirrors.length + 1} mirror endpoint(s)</span>
          </div>
        </div>

        {task.sha256 ? (
          <div className="flex items-center space-x-1 text-emerald-400 font-mono text-3xs">
            <ShieldCheck className="w-3.5 h-3.5" />
            <span className="truncate max-w-[200px]" title={task.sha256}>
              SHA-256: {task.sha256}
            </span>
          </div>
        ) : (
          <span className="text-neutral-500 font-mono text-3xs">Streaming SHA-256 verification...</span>
        )}
      </div>
    </div>
  );
};
