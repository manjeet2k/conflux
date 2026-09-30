import React from 'react';
import {
  Download,
  CheckCircle2,
  PauseCircle,
  FolderOpen,
  Network,
  Wifi,
  Smartphone,
  HardDrive,
} from 'lucide-react';
import type { CategoryFilter, AdapterInfo } from '../types';

interface SidebarProps {
  currentCategory: CategoryFilter;
  onSelectCategory: (category: CategoryFilter) => void;
  taskCounts: {
    all: number;
    downloading: number;
    completed: number;
    paused: number;
  };
  adapters: AdapterInfo[];
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentCategory,
  onSelectCategory,
  taskCounts,
  adapters,
}) => {
  const getAdapterIcon = (name: string) => {
    const lower = name.toLowerCase();
    if (lower.includes('wi-fi') || lower.includes('wlan') || lower.includes('wireless')) {
      return <Wifi className="w-4 h-4 text-fuchsia-400" />;
    }
    if (lower.includes('cellular') || lower.includes('mobile') || lower.includes('ndis') || lower.includes('lte') || lower.includes('5g')) {
      return <Smartphone className="w-4 h-4 text-amber-400" />;
    }
    return <Network className="w-4 h-4 text-cyan-400" />;
  };

  const categories = [
    { id: 'all' as CategoryFilter, label: 'All Downloads', icon: FolderOpen, count: taskCounts.all },
    { id: 'downloading' as CategoryFilter, label: 'Downloading', icon: Download, count: taskCounts.downloading },
    { id: 'completed' as CategoryFilter, label: 'Completed', icon: CheckCircle2, count: taskCounts.completed },
    { id: 'paused' as CategoryFilter, label: 'Paused', icon: PauseCircle, count: taskCounts.paused },
  ];

  return (
    <aside className="w-64 bg-fluent-subnav/80 border-r border-fluent-border flex flex-col justify-between p-3 select-none">
      <div className="space-y-6">
        {/* Categories Rail */}
        <div>
          <h3 className="text-xs font-semibold text-neutral-400 uppercase tracking-wider px-2 mb-2">
            Categories
          </h3>
          <nav className="space-y-1">
            {categories.map((cat) => {
              const Icon = cat.icon;
              const isSelected = currentCategory === cat.id;
              return (
                <button
                  key={cat.id}
                  onClick={() => onSelectCategory(cat.id)}
                  className={`w-full flex items-center justify-between px-3 py-2 rounded-lg text-xs font-medium transition ${
                    isSelected
                      ? 'bg-fluent-card text-white border border-fluent-border shadow-sm'
                      : 'text-neutral-400 hover:text-white hover:bg-fluent-card/50'
                  }`}
                >
                  <div className="flex items-center space-x-2.5">
                    <Icon className={`w-4 h-4 ${isSelected ? 'text-cyan-400' : 'text-neutral-400'}`} />
                    <span>{cat.label}</span>
                  </div>
                  <span
                    className={`text-2xs px-2 py-0.5 rounded-full ${
                      isSelected ? 'bg-cyan-500/20 text-cyan-300 font-semibold' : 'bg-neutral-800 text-neutral-400'
                    }`}
                  >
                    {cat.count}
                  </span>
                </button>
              );
            })}
          </nav>
        </div>

        {/* Physical Network Adapters Dock */}
        <div>
          <div className="flex items-center justify-between px-2 mb-2">
            <h3 className="text-xs font-semibold text-neutral-400 uppercase tracking-wider">
              Bonded Adapters
            </h3>
            <span className="text-3xs bg-cyan-500/10 text-cyan-400 border border-cyan-500/20 px-1.5 py-0.5 rounded">
              Active Aggregation
            </span>
          </div>

          <div className="space-y-2 max-h-56 overflow-y-auto">
            {adapters.length === 0 ? (
              <div className="text-2xs text-neutral-500 px-2 py-3 text-center">
                Discovering network interfaces...
              </div>
            ) : (
              adapters.map((adapter) => (
                <div
                  key={adapter.id}
                  className={`p-2.5 rounded-lg border transition ${
                    adapter.enabled
                      ? 'bg-fluent-card border-fluent-border shadow-fluent-card'
                      : 'bg-fluent-card/30 border-fluent-border/40 opacity-50'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                      <div className="p-1 rounded bg-black/40 border border-white/5">
                        {getAdapterIcon(adapter.name)}
                      </div>
                      <div>
                        <div className="text-xs font-medium text-white truncate max-w-[120px]" title={adapter.name}>
                          {adapter.name}
                        </div>
                        <div className="text-2xs text-neutral-400 font-mono">{adapter.ip}</div>
                      </div>
                    </div>

                    <span
                      className={`text-3xs px-1.5 py-0.5 rounded font-mono ${
                        adapter.enabled
                          ? 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/20'
                          : 'bg-neutral-800 text-neutral-400'
                      }`}
                    >
                      {adapter.enabled ? 'Ready' : 'Ignored'}
                    </span>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </div>

      {/* Storage Indicator */}
      <div className="bg-fluent-card p-3 rounded-lg border border-fluent-border">
        <div className="flex items-center space-x-2 text-xs font-medium text-neutral-300 mb-1.5">
          <HardDrive className="w-3.5 h-3.5 text-neutral-400" />
          <span>Storage</span>
        </div>
        <div className="w-full bg-neutral-800 rounded-full h-1.5 overflow-hidden mb-1">
          <div className="bg-cyan-500 h-full rounded-full w-2/3" />
        </div>
        <div className="flex justify-between text-3xs text-neutral-400">
          <span>Active Drive</span>
          <span>Ready</span>
        </div>
      </div>
    </aside>
  );
};
