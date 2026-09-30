import React, { useState } from 'react';
import { X, Download, Plus, Folder } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import type { AdapterInfo } from '../types';

interface NewDownloadModalProps {
  isOpen: boolean;
  onClose: () => void;
  adapters: AdapterInfo[];
  onStartDownload: (url: string, mirrors: string[], selectedAdapterIds: string[], saveDir: string) => void;
}

export const NewDownloadModal: React.FC<NewDownloadModalProps> = ({
  isOpen,
  onClose,
  adapters,
  onStartDownload,
}) => {
  const [url, setUrl] = useState('');
  const [mirrors, setMirrors] = useState<string[]>([]);
  const [newMirror, setNewMirror] = useState('');
  const [saveDir, setSaveDir] = useState('Downloads');
  const [selectedAdapters, setSelectedAdapters] = useState<string[]>(
    adapters.filter((a) => a.enabled).map((a) => a.id)
  );

  if (!isOpen) return null;

  const handleAddMirror = () => {
    if (newMirror.trim() && !mirrors.includes(newMirror.trim())) {
      setMirrors([...mirrors, newMirror.trim()]);
      setNewMirror('');
    }
  };

  const handleRemoveMirror = (index: number) => {
    setMirrors(mirrors.filter((_, i) => i !== index));
  };

  const toggleAdapterSelection = (id: string) => {
    if (selectedAdapters.includes(id)) {
      setSelectedAdapters(selectedAdapters.filter((a) => a !== id));
    } else {
      setSelectedAdapters([...selectedAdapters, id]);
    }
  };

  const handleBrowse = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Destination Folder',
      });
      if (selected && typeof selected === 'string') {
        setSaveDir(selected);
      }
    } catch (e) {
      console.error('Failed to open folder picker:', e);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!url.trim()) return;
    onStartDownload(url.trim(), mirrors, selectedAdapters, saveDir);
    onClose();
  };

  return (
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4 select-none">
      <div className="bg-fluent-card border border-fluent-border rounded-2xl w-full max-w-lg shadow-fluent-elevated overflow-hidden animate-in fade-in zoom-in-95 duration-150">
        {/* Modal Header */}
        <div className="flex items-center justify-between p-4 border-b border-fluent-border">
          <div className="flex items-center space-x-2">
            <div className="p-1.5 rounded-lg bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
              <Download className="w-4 h-4" />
            </div>
            <h3 className="text-sm font-semibold text-white">Add New Download</h3>
          </div>
          <button onClick={onClose} className="p-1 rounded-md text-neutral-400 hover:text-white hover:bg-white/10">
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Modal Form */}
        <form onSubmit={handleSubmit} className="p-4 space-y-4">
          {/* Target URL */}
          <div>
            <label className="block text-xs font-medium text-neutral-300 mb-1">
              Download URL <span className="text-cyan-400">*</span>
            </label>
            <input
              type="url"
              required
              autoFocus
              placeholder="https://example.com/file.iso"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="w-full bg-black/40 border border-fluent-border focus:border-cyan-400 rounded-lg px-3 py-2 text-xs text-white placeholder-neutral-500 outline-none font-mono"
            />
          </div>

          {/* Mirror URLs */}
          <div>
            <label className="block text-xs font-medium text-neutral-300 mb-1">
              Alternative Mirror URLs (Optional)
            </label>
            <div className="flex space-x-2 mb-2">
              <input
                type="url"
                placeholder="https://mirror.cdn.com/file.iso"
                value={newMirror}
                onChange={(e) => setNewMirror(e.target.value)}
                className="flex-1 bg-black/40 border border-fluent-border focus:border-cyan-400 rounded-lg px-3 py-1.5 text-xs text-white placeholder-neutral-500 outline-none font-mono"
              />
              <button
                type="button"
                onClick={handleAddMirror}
                className="px-3 py-1.5 bg-fluent-card hover:bg-fluent-card-hover border border-fluent-border text-xs rounded-lg text-neutral-300 hover:text-white flex items-center space-x-1"
              >
                <Plus className="w-3.5 h-3.5" />
                <span>Add</span>
              </button>
            </div>

            {mirrors.length > 0 && (
              <div className="space-y-1 max-h-24 overflow-y-auto">
                {mirrors.map((m, idx) => (
                  <div key={idx} className="flex items-center justify-between p-1.5 rounded bg-black/20 text-3xs font-mono text-neutral-300">
                    <span className="truncate max-w-sm">{m}</span>
                    <button type="button" onClick={() => handleRemoveMirror(idx)} className="text-neutral-500 hover:text-red-400">
                      <X className="w-3 h-3" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Adapters to Bond */}
          <div>
            <label className="block text-xs font-medium text-neutral-300 mb-1.5">
              Participating Network Adapters ({adapters.length} found)
            </label>
            <div className="grid grid-cols-2 gap-2 max-h-36 overflow-y-auto">
              {adapters.map((a) => {
                const isChecked = selectedAdapters.includes(a.id);
                return (
                  <label
                    key={a.id}
                    className={`flex items-center space-x-2 p-2 rounded-lg border cursor-pointer transition ${
                      isChecked
                        ? 'bg-cyan-500/10 border-cyan-500/30 text-white'
                        : 'bg-black/20 border-fluent-border text-neutral-400'
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={isChecked}
                      onChange={() => toggleAdapterSelection(a.id)}
                      className="rounded border-neutral-700 text-cyan-500 focus:ring-0"
                    />
                    <div className="overflow-hidden">
                      <div className="text-xs font-medium truncate">{a.name}</div>
                      <div className="text-3xs text-neutral-500 font-mono truncate">{a.ip}</div>
                    </div>
                  </label>
                );
              })}
            </div>
          </div>

          {/* Destination Directory */}
          <div>
            <label className="block text-xs font-medium text-neutral-300 mb-1">
              Save Destination
            </label>
            <div className="flex space-x-2">
              <input
                type="text"
                readOnly
                value={saveDir}
                className="flex-1 bg-black/20 border border-fluent-border rounded-lg px-3 py-1.5 text-xs text-neutral-300 font-mono truncate"
              />
              <button
                type="button"
                onClick={handleBrowse}
                className="px-3 py-1.5 bg-fluent-card hover:bg-fluent-card-hover border border-fluent-border text-xs rounded-lg text-neutral-300 hover:text-white flex items-center space-x-1"
              >
                <Folder className="w-3.5 h-3.5" />
                <span>Browse</span>
              </button>
            </div>
          </div>

          {/* Action buttons */}
          <div className="flex items-center justify-end space-x-2 pt-3 border-t border-fluent-border">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-lg text-xs font-medium text-neutral-400 hover:text-white hover:bg-white/5 transition"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!url.trim()}
              className="px-4 py-2 bg-fluent-accent hover:bg-fluent-accent-hover disabled:opacity-50 text-black font-semibold text-xs rounded-lg transition shadow-sm"
            >
              Start Accelerated Download
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
