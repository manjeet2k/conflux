// src/hooks/useDebounce.ts
import { useEffect, useState } from 'react';

/**
 * useDebounce
 *
 * Debounces any value changes by a specified delay in milliseconds.
 */
export function useDebounce<T>(value: T, delayMs: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedValue(value);
    }, delayMs);

    return () => {
      clearTimeout(timer);
    };
  }, [value, delayMs]);

  return debouncedValue;
}
