import { useEffect, useState } from 'react';
import { api } from '../api';
import type { Backdrop, ThemePreference } from '../types';

const darkQuery = '(prefers-color-scheme: dark)';

/**
 * Resolves the effective light/dark mode and applies it, plus the requested window material,
 * to the native window. `backdrop` is the material Windows actually gave us, which can be
 * weaker than requested (or `none` before the first answer arrives).
 */
export function useWindowTheme(preference: ThemePreference, requested: Backdrop) {
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
    api
      .applyWindowTheme(dark, requested)
      .then((b) => !cancelled && setBackdrop(b.backdrop))
      .catch((e) => {
        // Not fatal: fall back to opaque surfaces.
        console.warn('apply_window_theme failed:', e);
        if (!cancelled) setBackdrop('none');
      });
    return () => {
      cancelled = true;
    };
  }, [dark, requested]);

  return { dark, backdrop };
}
