// src/features/downloads/utils/downloadValidation.ts
import { fail, type Result } from '@/types/Result';

export type DownloadErrorKind = 'INVALID_PARAMS' | 'NETWORK' | 'FILE_SYSTEM' | 'UNKNOWN';

export class DownloadError extends Error {
  readonly kind: DownloadErrorKind;

  constructor(kind: DownloadErrorKind, message: string) {
    super(message);
    this.name = 'DownloadError';
    this.kind = kind;
  }
}

export interface DownloadRepository {
  startDownload(params: {
    url: string;
    saveDir: string;
    filename: string | null;
  }): Promise<Result<any, DownloadError>>;
}

export class StartDownloadUseCase {
  private repo: DownloadRepository;

  constructor(repo: DownloadRepository) {
    this.repo = repo;
  }

  async execute(params: {
    url: string;
    saveDir: string;
    filename: string | null;
  }): Promise<Result<any, DownloadError>> {
    const trimmedUrl = params.url.trim();
    if (!trimmedUrl) {
      return fail(new DownloadError('INVALID_PARAMS', 'URL cannot be empty'));
    }

    return this.repo.startDownload({
      ...params,
      url: trimmedUrl,
    });
  }
}
