import { useEffect, useState } from 'react';
import { api } from '../api';
import type { ThemePreference } from '../types';

const darkQuery = '(prefers-color-scheme: dark)';

/** Resolves the effective light/dark mode and applies it (plus Mica) to the native window. */
export function useWindowTheme(preference: ThemePreference) {
  const [systemDark, setSystemDark] = useState(() => window.matchMedia(darkQuery).matches);
  const [mica, setMica] = useState(false);

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
      .applyWindowTheme(dark)
      .then((b) => !cancelled && setMica(b.mica))
      .catch((e) => {
        // Not fatal: fall back to an opaque background.
        console.warn('apply_window_theme failed:', e);
        if (!cancelled) setMica(false);
      });
    return () => {
      cancelled = true;
    };
  }, [dark]);

  return { dark, mica };
}
