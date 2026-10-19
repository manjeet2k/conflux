// src/features/settings/utils/settingsValidation.ts
import type { Result } from '@/types/Result';
import type { Settings } from '../types/settings.types';

export interface SettingsRepository {
  updateSettings(settings: Settings): Promise<Result<Settings, Error>>;
}

export function sanitizeSettings(settings: Settings): Settings {
  const sanitized: Settings = { ...settings };
  delete sanitized.adapter_overrides;
  return sanitized;
}

export class UpdateSettingsUseCase {
  private repo: SettingsRepository;

  constructor(repo: SettingsRepository) {
    this.repo = repo;
  }

  async execute(settings: Settings): Promise<Result<Settings, Error>> {
    const sanitized = sanitizeSettings(settings);
    return this.repo.updateSettings(sanitized);
  }
}
