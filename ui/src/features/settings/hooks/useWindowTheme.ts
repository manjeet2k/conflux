// src/features/settings/hooks/useWindowTheme.ts
import { useEffect, useState } from 'react';
import type { Backdrop, ThemePreference } from '../types/settings.types';
import { settingsApi } from '../api/settings.api';

const darkQuery = '(prefers-color-scheme: dark)';

export interface UseWindowThemeReturn {
  dark: boolean;
  backdrop: Backdrop;
}

/**
 * Resolves the effective light/dark mode and applies it, plus the requested window material,
 * to the native window. `backdrop` is the material Windows actually gave us, which can be
 * weaker than requested (or `none` before the first answer arrives).
 */
export function useWindowTheme(
  preference: ThemePreference,
  requested: Backdrop
): UseWindowThemeReturn {
  const [systemDark, setSystemDark] = useState(() => window.matchMedia(darkQuery).matches);
  const [backdrop, setBackdrop] = useState<Backdrop>('none');

  useEffect(() => {
    const mq = window.matchMedia(darkQuery);
    const onChange = () => setSystemDark(mq.matches);
    onChange();

    if (typeof mq.addEventListener === 'function') {
      mq.addEventListener('change', onChange);
      return () => mq.removeEventListener('change', onChange);
    }

    mq.addListener(onChange);
    return () => mq.removeListener(onChange);
  }, []);

  const dark = preference === 'system' ? systemDark : preference === 'dark';

  useEffect(() => {
    let cancelled = false;

    settingsApi
      .applyWindowTheme(dark, requested)
      .then((b) => {
        if (!cancelled) setBackdrop(b.backdrop);
      })
      .catch((e) => {
        console.warn('apply_window_theme failed:', e);
        if (!cancelled) setBackdrop('none');
      });

    return () => {
      cancelled = true;
    };
  }, [dark, requested]);

  return { dark, backdrop };
}
