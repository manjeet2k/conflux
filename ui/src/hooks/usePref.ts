// src/hooks/usePref.ts
import { useCallback, useState } from 'react';

/**
 * usePref
 *
 * Synchronizes a boolean preference value with browser local storage.
 */
export function usePref(key: string, fallback: boolean): [boolean, (val: boolean | ((prev: boolean) => boolean)) => void] {
  const [value, setValue] = useState<boolean>(() => {
    const stored = localStorage.getItem(key);
    return stored === null ? fallback : stored === '1';
  });

  const update = useCallback(
    (nextVal: boolean | ((prev: boolean) => boolean)) => {
      setValue((current) => {
        const resolved = typeof nextVal === 'function' ? nextVal(current) : nextVal;
        localStorage.setItem(key, resolved ? '1' : '0');
        return resolved;
      });
    },
    [key]
  );

  return [value, update];
}
