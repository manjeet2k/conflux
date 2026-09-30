import React, { useEffect, useState } from 'react';
import { X, Download, Folder } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { downloadDir } from '@tauri-apps/api/path';
import type { AdapterInfo } from '../types';

interface NewDownloadModalProps {
  isOpen: boolean;
  onClose: () => void;
  adapters: AdapterInfo[];
  /** Resolves to an error message on failure, or null on success. */
  onStartDownload: (url: string, selectedAdapterIds: string[], saveDir: string) => Promise<string | null>;
}

/** Wrapper: the form is mounted only while open, so every open starts from a fresh state. */
export const NewDownloadModal: React.FC<NewDownloadModalProps> = (props) => {
  if (!props.isOpen) return null;
  return <NewDownloadForm {...props} />;
};

const NewDownloadForm: React.FC<NewDownloadModalProps> = ({ onClose, adapters, onStartDownload }) => {
  const [url, setUrl] = useState('');
  const [saveDir, setSaveDir] = useState('');
  // null = user hasn't touched the selection yet -> follow the discovery defaults.
  const [userSelection, setUserSelection] = useState<string[] | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Default destination: the OS Downloads folder (fetched on every open).
  useEffect(() => {
    let cancelled = false;
    downloadDir()
      .then((dir) => {
        if (!cancelled) setSaveDir((current) => current || dir);
      })
      .catch((e) => console.error('Failed to resolve Downloads folder:', e));
    return () => {
      cancelled = true;
    };
  }, []);

  // Derived from the current adapter list, so it stays valid when adapters (re)load.
  const adapterIds = new Set(adapters.map((a) => a.id));
  const selectedAdapters =
    userSelection === null
      ? adapters.filter((a) => a.enabled).map((a) => a.id)
      : userSelection.filter((id) => adapterIds.has(id));

  const toggleAdapterSelection = (id: string) => {
    setUserSelection(
      selectedAdapters.includes(id)
        ? selectedAdapters.filter((a) => a !== id)
        : [...selectedAdapters, id]
    );
  };

  const handleBrowse = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Destination Folder',
        defaultPath: saveDir || undefined,
      });
      if (selected && typeof selected === 'string') {
        setSaveDir(selected);
      }
    } catch (e) {
      console.error('Failed to open folder picker:', e);
    }
  };

  const noAdapterSelected = adapters.length > 0 && selectedAdapters.length === 0;
  const canSubmit = !!url.trim() && !!saveDir && !noAdapterSelected && !submitting;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setSubmitError(null);
    const error = await onStartDownload(url.trim(), selectedAdapters, saveDir);
    setSubmitting(false);
    if (error) {
      setSubmitError(error);
    } else {
      onClose();
    }
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

          {/* Mirror URLs (not supported by the engine yet) */}
          <div className="opacity-50" title="Mirror aggregation is coming soon">
            <label className="block text-xs font-medium text-neutral-300 mb-1">
              Alternative Mirror URLs <span className="text-neutral-500">(coming soon)</span>
            </label>
            <input
              type="url"
              disabled
              placeholder="Multi-mirror downloads are not supported yet"
              className="w-full bg-black/20 border border-fluent-border rounded-lg px-3 py-1.5 text-xs text-neutral-500 placeholder-neutral-600 outline-none font-mono cursor-not-allowed"
            />
          </div>

          {/* Adapters to Bond */}
          <div>
            <label className="block text-xs font-medium text-neutral-300 mb-1.5">
              Participating Network Adapters ({adapters.length} found)
            </label>
            {noAdapterSelected && (
              <p className="text-3xs text-amber-400 mb-1.5">Select at least one adapter.</p>
            )}
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
                placeholder="Resolving Downloads folder..."
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

          {submitError && (
            <div role="alert" className="p-2 rounded-lg bg-rose-500/10 border border-rose-500/30 text-xs text-rose-300 break-words select-text">
              {submitError}
            </div>
          )}

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
              disabled={!canSubmit}
              className="px-4 py-2 bg-fluent-accent hover:bg-fluent-accent-hover disabled:opacity-50 text-black font-semibold text-xs rounded-lg transition shadow-sm"
            >
              {submitting ? 'Probing...' : 'Start Accelerated Download'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
