import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { downloadsApi } from '@/features/downloads/api/downloads.api';
import { settingsApi } from '@/features/settings/api/settings.api';
import type { ExternalDownloadPayload } from '@/features/downloads/types/downloads.types';
import type { UpdateInfo } from '@/features/settings/types/settings.types';

// Attach rejection handling immediately, even if registration finishes after unmount.
function cleanupSubscription(registration: Promise<() => void>): () => void {
  const ready = registration.catch((error) => {
    console.error('Failed to register desktop listener:', error);
    return () => {};
  });
  return () => {
    ready.then((off) => off()).catch((error) => console.error('Failed to remove desktop listener:', error));
  };
}

interface DesktopEventHandlers {
  onAdd: () => void;
  onSettings: () => void;
  onExternalDownload: (payload: ExternalDownloadPayload) => void;
  onUpdate: (info: UpdateInfo) => void;
}

export function useDesktopEvents({ onAdd, onSettings, onExternalDownload, onUpdate }: DesktopEventHandlers) {
  const [historyError, setHistoryError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let pending = Promise.resolve();
    const drain = () => {
      pending = pending.then(async () => {
        while (!disposed) {
          const payload = await downloadsApi.takePendingDownload();
          if (!payload) break;
          if (!disposed) onExternalDownload(payload);
        }
      }).catch((error) => console.error('Failed to take pending download:', error));
    };
    // The event is a wake-up signal. Subscribe before draining the native queue
    // so requests arriving during registration or draining cannot be lost/doubled.
    const registration = downloadsApi.onExternalDownload(drain);
    const cleanup = cleanupSubscription(registration.then((off) => {
      if (!disposed) drain();
      return off;
    }));
    return () => {
      disposed = true;
      cleanup();
    };
  }, [onExternalDownload]);

  useEffect(() => cleanupSubscription(downloadsApi.onHistoryStatus(setHistoryError)), []);
  useEffect(() => cleanupSubscription(settingsApi.onUpdateAvailable(onUpdate)), [onUpdate]);

  useEffect(() => {
    const stopAdd = cleanupSubscription(listen('open-add-dialog', () => onAdd()));
    const stopSettings = cleanupSubscription(listen('open-settings', () => onSettings()));
    return () => {
      stopAdd();
      stopSettings();
    };
  }, [onAdd, onSettings]);

  return historyError;
}
