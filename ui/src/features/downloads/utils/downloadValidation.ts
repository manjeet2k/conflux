import { fail, type Result } from '@/types/Result';
import type { DownloadTask, RequestHeaders } from '../types/downloads.types';

export type DownloadErrorKind = 'INVALID_PARAMS' | 'NETWORK' | 'FILE_SYSTEM' | 'UNKNOWN';

export class DownloadError extends Error {
  readonly kind: DownloadErrorKind;

  constructor(kind: DownloadErrorKind, message: string) {
    super(message);
    this.name = 'DownloadError';
    this.kind = kind;
  }
}

export interface StartDownloadParams {
  url: string;
  saveDir: string;
  filename: string | null;
  headers?: RequestHeaders | null;
  handoffId?: string | null;
}

export interface DownloadRepository {
  startDownload(params: StartDownloadParams): Promise<Result<DownloadTask, DownloadError>>;
}

export const isHttpUrl = (value: string): boolean => {
  try {
    const url = new URL(value.trim());
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
};

export const sameOrigin = (a: string, b: string): boolean => {
  try {
    return new URL(a.trim()).origin.toLowerCase() === new URL(b.trim()).origin.toLowerCase();
  } catch {
    return false;
  }
};

export class StartDownloadUseCase {
  private repo: DownloadRepository;

  constructor(repo: DownloadRepository) {
    this.repo = repo;
  }

  async execute(params: StartDownloadParams): Promise<Result<DownloadTask, DownloadError>> {
    const url = params.url.trim();
    const saveDir = params.saveDir.trim();
    if (!isHttpUrl(url)) {
      return fail(new DownloadError('INVALID_PARAMS', 'A valid HTTP or HTTPS URL is required'));
    }
    if (!saveDir) {
      return fail(new DownloadError('INVALID_PARAMS', 'Save folder is required'));
    }
    return this.repo.startDownload({ ...params, url, saveDir });
  }
}
