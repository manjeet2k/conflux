import { ok, fail } from '@/types/Result';
import { errorText } from '@/utils/errors';
import { settingsApi } from './settings.api';

import type { SettingsRepository } from '../utils/settingsValidation';

export const settingsRepository: SettingsRepository = {
  async updateSettings(settings) {
    try {
      return ok(await settingsApi.updateSettings(settings));
    } catch (error) {
      return fail(new Error(errorText(error)));
    }
  },
};
