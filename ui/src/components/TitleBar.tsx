import React from 'react';
import { Plus, Play, Pause, Settings, Minus, Square, X, Zap } from 'lucide-react';
import { formatSpeed } from '../utils/formatters';

interface TitleBarProps {
  totalSpeed: number;
  onOpenNewModal: () => void;
  onResumeAll: () => void;
  onPauseAll: () => void;
}

export const TitleBar: React.FC<TitleBarProps> = ({
  totalSpeed,
  onOpenNewModal,
  onResumeAll,
  onPauseAll,
}) => {
  return (
    <header className="h-12 bg-fluent-subnav border-b border-fluent-border flex items-center justify-between px-3 select-none">
      {/* App branding */}
      <div className="flex items-center space-x-3">
        <div className="w-7 h-7 rounded-lg bg-gradient-to-tr from-cyan-500 to-blue-600 flex items-center justify-center shadow-lg shadow-cyan-500/20">
          <Zap className="w-4 h-4 text-white fill-white" />
        </div>
        <span className="font-semibold text-sm tracking-wide text-white">Conflux</span>
        <span className="text-xs px-2 py-0.5 rounded-full bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 font-mono">
          v0.1.0 • Channel Bonding
        </span>
      </div>

      {/* Global Action Toolbar */}
      <div className="flex items-center space-x-2">
        <button
          onClick={onOpenNewModal}
          className="flex items-center space-x-1.5 bg-fluent-accent hover:bg-fluent-accent-hover text-black px-3 py-1.5 rounded-md font-medium text-xs transition shadow-sm"
        >
          <Plus className="w-3.5 h-3.5" />
          <span>New Download</span>
        </button>

        <button
          onClick={onResumeAll}
          title="Resume all downloads"
          className="flex items-center space-x-1 bg-fluent-card hover:bg-fluent-card-hover border border-fluent-border px-2.5 py-1.5 rounded-md text-xs text-neutral-300 hover:text-white transition"
        >
          <Play className="w-3 h-3 text-emerald-400 fill-emerald-400" />
          <span>Resume All</span>
        </button>

        <button
          onClick={onPauseAll}
          title="Pause all downloads"
          className="flex items-center space-x-1 bg-fluent-card hover:bg-fluent-card-hover border border-fluent-border px-2.5 py-1.5 rounded-md text-xs text-neutral-300 hover:text-white transition"
        >
          <Pause className="w-3 h-3 text-amber-400 fill-amber-400" />
          <span>Pause All</span>
        </button>

        <div className="h-4 w-px bg-fluent-border mx-1" />

        {/* Live Aggregated Bandwidth Meter */}
        <div className="flex items-center space-x-2 bg-black/40 border border-fluent-border px-3 py-1 rounded-full">
          <span className="relative flex h-2 w-2">
            {totalSpeed > 0 && (
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-cyan-400 opacity-75"></span>
            )}
            <span className={`relative inline-flex rounded-full h-2 w-2 ${totalSpeed > 0 ? 'bg-cyan-500' : 'bg-neutral-600'}`}></span>
          </span>
          <span className="text-xs font-mono text-cyan-300 font-semibold">
            {formatSpeed(totalSpeed)}
          </span>
        </div>
      </div>

      {/* Windows window management buttons */}
      <div className="flex items-center">
        <button className="p-1.5 hover:bg-white/10 rounded text-neutral-400 hover:text-white transition">
          <Settings className="w-4 h-4" />
        </button>
        <button className="p-1.5 hover:bg-white/10 rounded text-neutral-400 hover:text-white transition ml-2">
          <Minus className="w-3.5 h-3.5" />
        </button>
        <button className="p-1.5 hover:bg-white/10 rounded text-neutral-400 hover:text-white transition">
          <Square className="w-3 h-3" />
        </button>
        <button className="p-1.5 hover:bg-red-500 rounded text-neutral-400 hover:text-white transition">
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    </header>
  );
};
