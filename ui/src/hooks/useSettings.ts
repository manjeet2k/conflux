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
};

export function useSettings(onError: (message: string) => void) {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  // Latest settings for `update`, which must not close over stale state.
  const current = useRef(settings);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        current.current = s;
        setSettings(s);
      })
      .catch((e) => onError(`Failed to load settings: ${String(e)}`));
  }, [onError]);

  /** Applies optimistically, then adopts the backend's validated (clamped) copy. */
  const update = useCallback(
    (patch: Partial<Settings>) => {
      const prev = current.current;
      const next = { ...prev, ...patch };
      current.current = next;
      setSettings(next);
      api
        .updateSettings(next)
        .then((saved) => {
          current.current = saved;
          setSettings(saved);
        })
        .catch((e) => {
          onError(`Failed to save settings: ${String(e)}`);
          current.current = prev;
          setSettings(prev);
        });
    },
    [onError]
  );

  return { settings, update };
}
