// src/features/network/hooks/useNetwork.ts
import { useCallback, useEffect, useState } from 'react';
import type { AdapterInfo, Diagnostics } from '../types/network.types';
import { networkApi } from '../api/network.api';

export interface UseNetworkReturn {
  adapters: AdapterInfo[];
  diagnostics: Diagnostics | null;
  refreshAdapters: () => Promise<void>;
  toggleAdapter: (id: string, enabled: boolean) => Promise<void>;
  loadDiagnostics: () => Promise<Diagnostics | null>;
  openLogsFolder: () => Promise<void>;
}

export function useNetwork(onError: (message: string) => void): UseNetworkReturn {
  const [adapters, setAdapters] = useState<AdapterInfo[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);

  const refreshAdapters = useCallback(async () => {
    try {
      const list = await networkApi.discoverAdapters();
      setAdapters(list);
    } catch (e) {
      onError(`Failed to discover network adapters: ${String(e)}`);
    }
  }, [onError]);

  useEffect(() => {
    let disposed = false;
    const unlistenPromise = networkApi.onAdaptersChanged((nextAdapters) => {
      setAdapters(nextAdapters);
    });

    networkApi
      .discoverAdapters()
      .then((list) => {
        if (!disposed) setAdapters(list);
      })
      .catch((e) => {
        onError(`Failed to load adapters: ${String(e)}`);
      });

    return () => {
      disposed = true;
      unlistenPromise.then((fn) => fn());
    };
  }, [onError]);

  const toggleAdapter = useCallback(
    async (id: string, enabled: boolean) => {
      // Optimistic update
      setAdapters((prev) =>
        prev.map((a) => (a.id === id ? { ...a, enabled } : a))
      );
      try {
        const updated = await networkApi.setAdapterEnabled(id, enabled);
        setAdapters(updated);
      } catch (e) {
        onError(`Failed to toggle adapter: ${String(e)}`);
        await refreshAdapters();
      }
    },
    [onError, refreshAdapters]
  );

  const loadDiagnostics = useCallback(async () => {
    try {
      const diag = await networkApi.getDiagnostics();
      setDiagnostics(diag);
      return diag;
    } catch (e) {
      onError(`Failed to load diagnostics: ${String(e)}`);
      return null;
    }
  }, [onError]);

  const openLogsFolder = useCallback(async () => {
    try {
      await networkApi.openLogsFolder();
    } catch (e) {
      onError(`Failed to open logs folder: ${String(e)}`);
    }
  }, [onError]);

  return {
    adapters,
    diagnostics,
    refreshAdapters,
    toggleAdapter,
    loadDiagnostics,
    openLogsFolder,
  };
}
