// src/features/network/types/network.types.ts

export type AdapterKind = 'ethernet' | 'wifi' | 'cellular' | 'virtual' | 'loopback' | 'other';

export type DisabledReason = 'loopback' | 'link_local' | 'no_ipv4' | 'virtual';

export interface AdapterInfo {
  id: string;
  name: string;
  ip: string;
  is_ipv4: boolean;
  is_loopback: boolean;
  enabled: boolean;
  usable: boolean;
  kind: AdapterKind;
  disabled_reason: DisabledReason | null;
}

export interface Diagnostics {
  app_version: string;
  os: string;
  arch: string;
  webview_version: string | null;
  adapters: {
    name: string;
    kind: AdapterKind;
    enabled: boolean;
    usable: boolean;
    disabled_reason: DisabledReason | null;
    subnet: string;
  }[];
  settings: {
    schema_version: number;
    theme: string;
    backdrop: string;
    connections_per_adapter: number;
    notify_on_complete: boolean;
    close_to_tray: boolean;
    auto_aggregate_adapters: boolean;
    custom_save_dir: boolean;
    adapter_override_count: number;
  };
  recent_errors: string[];
}
