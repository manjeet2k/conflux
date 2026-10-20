import { ok, fail } from '@/types/Result';
import { errorText } from '@/utils/errors';
import { DownloadError } from '../utils/downloadValidation';
import { downloadsApi } from './downloads.api';

import type { DownloadRepository } from '../utils/downloadValidation';

/** Adapt throwing IPC calls to the application-layer Result contract. */
export const downloadsRepository: DownloadRepository = {
  async startDownload(params) {
    try {
      return ok(await downloadsApi.startDownload(params));
    } catch (error) {
      return fail(new DownloadError('UNKNOWN', errorText(error)));
    }
  },
};
