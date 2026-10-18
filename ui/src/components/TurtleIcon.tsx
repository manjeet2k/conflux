import React from 'react';

/** Outlined at full speed; warm shell and green body when slow mode is on. */
export const TurtleIcon: React.FC<{ active: boolean }> = ({ active }) => (
  <svg width="32" height="26" viewBox="0 0 80 60" fill="none" aria-hidden="true">
    <g stroke={active ? '#20251b' : 'currentColor'} strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round">
      <path
        d="M12 35 5 38 7 32 14 29 M14 37C8 42 7 47 7 54H15L18 45H24L23 54H31L34 43H45L46 54H54L55 41C61 37 64 32 66 26C74 25 78 20 77 14C76 6 69 3 63 5C56 7 54 12 53 18L43 22 16 27Z"
        fill={active ? '#a8d77b' : 'none'}
      />
      <path d="M10 35C10 17 21 5 34 5C47 5 56 16 57 31C45 38 27 39 10 35Z" fill={active ? '#c58b50' : 'none'} />
      <path d="M29 14 40 13 47 23 42 33 29 34 22 24Z" fill={active ? '#e2ab6b' : 'none'} />
      <path d="M29 14 25 7 M40 13 44 8 M47 23 55 21 M42 33 46 35 M29 34 27 37 M22 24 12 24 M29 14 18 14" />
    </g>
    <circle cx="68" cy="14" r="1.7" fill={active ? '#20251b' : 'currentColor'} />
  </svg>
);
