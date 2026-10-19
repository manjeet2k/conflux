// src/features/network/api/network.api.ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AdapterInfo, Diagnostics } from '../types/network.types';
import { NETWORK_ADAPTERS_CHANGED_EVENT } from '@/utils/events.constants';

/**
 * Network API
 *
 * IPC bindings for adapter discovery, adapter toggling, and network diagnostics.
 */
export const networkApi = {
  discoverAdapters: () => invoke<AdapterInfo[]>('discover_adapters'),

  setAdapterEnabled: (id: string, enabled: boolean) =>
    invoke<AdapterInfo[]>('set_adapter_enabled', { id, enabled }),

  getDiagnostics: () => invoke<Diagnostics>('get_diagnostics'),

  openLogsFolder: () => invoke<void>('open_logs_folder'),

  onAdaptersChanged: (callback: (adapters: AdapterInfo[]) => void) =>
    listen<AdapterInfo[]>(NETWORK_ADAPTERS_CHANGED_EVENT, (event) => callback(event.payload)),
};
