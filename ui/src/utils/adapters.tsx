import {
  PlugConnected20Regular,
  Wifi120Regular,
  CellularData120Regular,
  Cloud20Regular,
  ArrowRepeatAll20Regular,
  Globe20Regular,
} from '@fluentui/react-icons';
import type { FluentIcon } from '@fluentui/react-icons';
import type { AdapterInfo, AdapterKind, AdapterStat } from '../types';
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

/**
 * One-line, human-readable explanation of why a per-task adapter stat is stalled or dropped,
 * or null when it is healthy. The strings come from the engine and never contain URLs.
 */
export function adapterProblem(a: Pick<AdapterStat, 'dropped' | 'last_error' | 'drop_reason'>): string | null {
  if (a.dropped) {
    return [a.drop_reason ?? 'no longer used for this download', a.last_error].filter(Boolean).join('. Last error: ');
  }
  return a.last_error ? `Last error: ${a.last_error}` : null;
}
