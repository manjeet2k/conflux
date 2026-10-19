// src/features/settings/hooks/useSettings.ts
import { useCallback, useEffect, useRef, useState } from 'react';
import type { Settings } from '../types/settings.types';
import { settingsApi } from '../api/settings.api';
import { sanitizeSettings } from '../utils/settingsValidation';

export const DEFAULT_SETTINGS: Settings = {
  theme: 'system',
  backdrop: 'mica_alt',
  default_save_dir: null,
  connections_per_adapter: 4,
  slow_mode: false,
  slow_mode_limit_kib: 1024,
  notify_on_complete: true,
  close_to_tray: true,
  auto_aggregate_adapters: true,
  check_updates_on_start: true,
};

export interface UseSettingsReturn {
  settings: Settings;
  loaded: boolean;
  update: (patch: Partial<Settings>) => void;
}

/**
 * Settings state hook.
 *
 * Edits are rejected until the backend copy has loaded (`loaded`), so
 * defaults never overwrite real settings. Saves are serialized; each one sends the last
 * confirmed settings plus the patch, so a failed save is dropped and the UI reverts
 * to what the native backend actually holds.
 */
export function useSettings(onError: (message: string) => void): UseSettingsReturn {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);
  const confirmed = useRef<Settings | null>(null);
  const pending = useRef<Partial<Settings>[]>([]);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());

  const display = useCallback(() => {
    if (!confirmed.current) return;
    setSettings(pending.current.reduce<Settings>((s, p) => ({ ...s, ...p }), confirmed.current));
  }, []);

  useEffect(() => {
    let disposed = false;
    settingsApi
      .getSettings()
      .then((s) => {
        if (disposed) return;
        confirmed.current = s;
        setLoaded(true);
        display();
      })
      .catch((e) => {
        if (!disposed) {
          confirmed.current = DEFAULT_SETTINGS;
          setLoaded(true);
          display();
          onError(`Failed to load settings: ${String(e)}`);
        }
      });

    return () => {
      disposed = true;
    };
  }, [onError, display]);

  const update = useCallback(
    (patch: Partial<Settings>) => {
      if (!confirmed.current) return;
      pending.current.push(patch);
      display();

      saveQueue.current = saveQueue.current
        .then(async () => {
          const next: Settings = sanitizeSettings({
            ...(confirmed.current as Settings),
            ...patch,
          });
          confirmed.current = await settingsApi.updateSettings(next);
        })
        .catch((e) => {
          onError(`Failed to save settings: ${String(e)}`);
        })
        .finally(() => {
          const idx = pending.current.indexOf(patch);
          if (idx !== -1) {
            pending.current.splice(idx, 1);
          }
          display();
        });
    },
    [onError, display]
  );

  return { settings, loaded, update };
}
