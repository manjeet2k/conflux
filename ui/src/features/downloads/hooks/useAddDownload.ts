// src/features/downloads/hooks/useAddDownload.ts
import { useCallback, useState } from 'react';
import type { ProbeResult, RequestHeaders } from '../types/downloads.types';
import { downloadsApi } from '../api/downloads.api';

export interface UseAddDownloadOptions {
  defaultSaveDir: string | null;
  onStart: (args: {
    url: string;
    saveDir: string;
    filename: string | null;
    headers?: RequestHeaders | null;
    handoffId?: string | null;
  }) => Promise<unknown>;
  onClose: () => void;
}

export function useAddDownload({
  defaultSaveDir,
  onStart,
  onClose,
}: UseAddDownloadOptions) {
  const [url, setUrl] = useState('');
  const [saveDir, setSaveDir] = useState(defaultSaveDir ?? '');
  const [filename, setFilename] = useState('');
  const [probing, setProbing] = useState(false);
  const [probeResult, setProbeResult] = useState<ProbeResult | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const probe = useCallback(async (targetUrl: string, headers?: RequestHeaders | null) => {
    const trimmed = targetUrl.trim();
    if (!trimmed) {
      setProbeResult(null);
      return;
    }

    setProbing(true);
    setError(null);
    try {
      const res = await downloadsApi.probeUrl(trimmed, headers);
      setProbeResult(res);
      if (res.filename && !filename) {
        setFilename(res.filename);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setProbing(false);
    }
  }, [filename]);

  const submit = useCallback(
    async (handoffId?: string | null, headers?: RequestHeaders | null) => {
      const trimmedUrl = url.trim();
      if (!trimmedUrl) {
        setError('URL is required');
        return;
      }
      if (!saveDir.trim()) {
        setError('Save folder is required');
        return;
      }

      setSubmitting(true);
      setError(null);
      try {
        await onStart({
          url: trimmedUrl,
          saveDir: saveDir.trim(),
          filename: filename.trim() || null,
          headers: headers ?? null,
          handoffId: handoffId ?? null,
        });
        onClose();
      } catch (e) {
        setError(String(e));
      } finally {
        setSubmitting(false);
      }
    },
    [url, saveDir, filename, onStart, onClose]
  );

  return {
    url,
    setUrl,
    saveDir,
    setSaveDir,
    filename,
    setFilename,
    probing,
    probeResult,
    submitting,
    error,
    setError,
    probe,
    submit,
  };
}
