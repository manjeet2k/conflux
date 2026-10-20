import { useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { downloadDir } from '@tauri-apps/api/path';
import { errorText } from '@/utils/errors';
import { isHttpUrl, sameOrigin } from '../utils/downloadValidation';
import { downloadsApi } from '../api/downloads.api';

import type { FormEvent } from 'react';
import type { ProbeResult, RequestHeaders } from '../types/downloads.types';
import type { StartDownloadParams } from '../utils/downloadValidation';

export interface UseAddDownloadOptions {
  initialUrl: string;
  initialFilename?: string | null;
  initialHeaders?: RequestHeaders | null;
  initialHandoffId?: string | null;
  defaultSaveDir: string | null;
  onStart: (args: StartDownloadParams) => Promise<unknown>;
  onClose: (accepted?: boolean) => void;
}

export function useAddDownload({
  initialUrl, initialFilename, initialHeaders, initialHandoffId,
  defaultSaveDir, onStart, onClose,
}: UseAddDownloadOptions) {
  const [url, setUrl] = useState(initialUrl);
  // Outcome of the last finished probe, tagged with the URL it was for.
  const [probeState, setProbeState] = useState<{ url: string; result?: ProbeResult; error?: string } | null>(null);
  // null = not edited by the user -> follow the probe's suggestion.
  const [filename, setFilename] = useState<string | null>(initialFilename ?? null);
  const [saveDir, setSaveDir] = useState(defaultSaveDir ?? '');
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  useEffect(() => {
    if (defaultSaveDir) return;
    downloadDir()
      .then((dir) => setSaveDir((cur) => cur || dir))
      .catch((e) => console.error('Failed to resolve Downloads folder:', e));
  }, [defaultSaveDir]);

  // Debounced probe: preview name, size and range support before starting. Results for a
  // URL other than the current one are ignored, so stale responses never show.
  const target = url.trim();
  const targetValid = isHttpUrl(target);
  // Only forward browser session cookies/headers if the target URL shares the same origin
  // as the initial URL from the browser. This prevents accidental cookie leakage if the user
  // edits the URL to point to a different host/domain.
  const effectiveHeaders = initialHeaders && sameOrigin(initialUrl, target) ? initialHeaders : null;

  useEffect(() => {
    if (!targetValid) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      downloadsApi
        .probeUrl(target, effectiveHeaders)
        .then((result) => !cancelled && setProbeState({ url: target, result }))
        .catch((e) => !cancelled && setProbeState({ url: target, error: errorText(e) }));
    }, 450);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [target, targetValid, effectiveHeaders]);

  const current = probeState?.url === target ? probeState : null;
  const probe = current?.result ?? null;
  const probeError = current?.error ?? null;
  const probing = targetValid && !current;

  const browse = async () => {
    try {
      const dir = await open({ directory: true, multiple: false, defaultPath: saveDir || undefined });
      if (typeof dir === 'string') setSaveDir(dir);
    } catch (e) {
      console.error('Folder picker failed:', e);
    }
  };

  const effectiveName = (filename ?? probe?.filename ?? '').trim();
  const canSubmit = isHttpUrl(url) && saveDir.trim() !== '' && !submitting;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onStart({
        url: url.trim(),
        saveDir: saveDir.trim(),
        filename: filename !== null && effectiveName ? effectiveName : null,
        headers: effectiveHeaders,
        handoffId: initialHandoffId ?? null,
      });
      onClose(true);
    } catch (err) {
      setSubmitError(errorText(err));
      setSubmitting(false);
    }
  };

  return {
    url, setUrl, filename, setFilename, saveDir, setSaveDir, submitting,
    submitError, effectiveName, canSubmit, probe, probeError, probing, browse, submit,
  };
}
