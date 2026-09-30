import {
  PlugConnected20Regular,
  Wifi120Regular,
  CellularData120Regular,
  Cloud20Regular,
  ArrowRepeatAll20Regular,
  Globe20Regular,
} from '@fluentui/react-icons';
import type { FluentIcon } from '@fluentui/react-icons';
import type { AdapterInfo, AdapterKind } from '../types';
import { seriesColors, seriesColorsDark } from '../theme';

export const kindIcon: Record<AdapterKind, FluentIcon> = {
  ethernet: PlugConnected20Regular,
  wifi: Wifi120Regular,
  cellular: CellularData120Regular,
  virtual: Cloud20Regular,
  loopback: ArrowRepeatAll20Regular,
  other: Globe20Regular,
};

export const kindLabel: Record<AdapterKind, string> = {
  ethernet: 'Ethernet',
  wifi: 'Wi-Fi',
  cellular: 'Cellular',
  virtual: 'Virtual',
  loopback: 'Loopback',
  other: 'Network',
};

/** Adapters the engine can actually bind to, in a stable display order. */
export function usableAdapters(adapters: AdapterInfo[]): AdapterInfo[] {
  return adapters.filter((a) => a.usable).sort((a, b) => a.id.localeCompare(b.id));
}

/** Stable color per adapter id (by position among usable adapters); unbound routing is neutral. */
export function adapterColor(adapterId: string | null, adapters: AdapterInfo[], dark: boolean): string {
  const palette = dark ? seriesColorsDark : seriesColors;
  if (adapterId === null) return dark ? '#9E9E9E' : '#707070';
  const idx = usableAdapters(adapters).findIndex((a) => a.id === adapterId);
  return palette[(idx < 0 ? palette.length - 1 : idx) % palette.length];
}
