// src/features/settings/index.ts
export { settingsApi } from './api/settings.api';
export type { AboutLink, WindowBackdrop } from './api/settings.api';
export { SettingsView } from './components/SettingsView';
export type { SettingsViewProps } from './components/SettingsView';
export { useSettings, DEFAULT_SETTINGS } from './hooks/useSettings';
export type { UseSettingsReturn } from './hooks/useSettings';
export { useWindowTheme } from './hooks/useWindowTheme';
export type { UseWindowThemeReturn } from './hooks/useWindowTheme';
export type {
  Backdrop,
  BrowserIntegrationStatus,
  Settings,
  ThemePreference,
  UpdateInfo,
} from './types/settings.types';
export {
  sanitizeSettings,
  UpdateSettingsUseCase,
} from './utils/settingsValidation';
export type { SettingsRepository } from './utils/settingsValidation';
