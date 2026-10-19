// src/features/network/index.ts
export { networkApi } from './api/network.api';
export { NetworkView } from './components/NetworkView';
export type { NetworkViewProps } from './components/NetworkView';
export { useNetwork } from './hooks/useNetwork';
export type { UseNetworkReturn } from './hooks/useNetwork';
export type {
  AdapterInfo,
  AdapterKind,
  Diagnostics,
  DisabledReason,
} from './types/network.types';
export {
  adapterColor,
  adapterProblem,
  kindIcon,
  kindLabel,
  usableAdapters,
} from './utils/adapters';
