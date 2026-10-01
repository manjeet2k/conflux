import { useCallback, useEffect, useRef, useState } from 'react';
import { api } from '../api';
import type { Settings } from '../types';

export const DEFAULT_SETTINGS: Settings = {
  theme: 'system',
  default_save_dir: null,
  connections_per_adapter: 4,
  chunk_size_mb: 4,
  notify_on_complete: true,
  close_to_tray: true,
  auto_aggregate_adapters: true,
};

/**
 * Settings state. Edits are rejected until the backend copy has loaded (`loaded`), so
 * defaults never overwrite real settings. Saves are serialised; each one sends the last
 * confirmed settings plus the patch, so a failed save is simply dropped and the UI
 * reverts to what the backend actually holds.
 */
export function useSettings(onError: (message: string) => void) {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  // Last copy confirmed by the backend; saves build on this, never on optimistic state.
  const confirmed = useRef<Settings | null>(null);
  // Patches not yet confirmed, in order; shown optimistically on top of `confirmed`.
  const pending = useRef<Partial<Settings>[]>([]);
  const saveQueue = useRef(Promise.resolve());

  const display = useCallback(() => {
    if (!confirmed.current) return;
    setSettings(pending.current.reduce<Settings>((s, p) => ({ ...s, ...p }), confirmed.current));
  }, []);

  useEffect(() => {
    let disposed = false;
    api
      .getSettings()
      .then((s) => {
        if (disposed) return;
        confirmed.current = s;
        setLoaded(true);
        display();
      })
      .catch((e) => !disposed && onError(`Failed to load settings: ${String(e)}`));
    return () => {
      disposed = true;
    };
  }, [onError, display]);

  /** Applies optimistically, then adopts the backend's validated (clamped) copy. */
  const update = useCallback(
    (patch: Partial<Settings>) => {
      if (!confirmed.current) return;
      pending.current.push(patch);
      display();
      saveQueue.current = saveQueue.current
        .then(async () => {
          const next: Settings = { ...(confirmed.current as Settings), ...patch };
          // Adapter toggles are owned by `set_adapter_enabled`; never send a stale copy.
          delete next.adapter_overrides;
          confirmed.current = await api.updateSettings(next);
        })
        .catch((e) => onError(`Failed to save settings: ${String(e)}`))
        .finally(() => {
          pending.current.splice(pending.current.indexOf(patch), 1);
          display();
        });
    },
    [onError, display]
  );

  return { settings, loaded, update };
}
