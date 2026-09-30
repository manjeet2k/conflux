import React from 'react';

/** Conflux mark: three streams converging into one downward arrow. */
export const AppLogo: React.FC<{ size?: number }> = ({ size = 16 }) => (
  <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden>
    <defs>
      <linearGradient id="cfx-logo" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stopColor="#4CC2FF" />
        <stop offset="1" stopColor="#0067C0" />
      </linearGradient>
    </defs>
    <rect width="16" height="16" rx="4" fill="url(#cfx-logo)" />
    <path
      d="M4 3.5c0 2.5 4 2.5 4 5M12 3.5c0 2.5-4 2.5-4 5M8 3.5v8M5.5 9.5 8 12l2.5-2.5"
      fill="none"
      stroke="#fff"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);
