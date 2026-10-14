import React from 'react';
import { makeStyles, mergeClasses, tokens } from '@fluentui/react-components';
import type { TaskStatus } from '../types';
import { statusText } from '../utils/status';

const useStyles = makeStyles({
  track: {
    position: 'relative',
    overflow: 'hidden',
    backgroundColor: 'var(--cfx-track)',
  },
  row: { height: '4px', borderRadius: '2px' },
  large: { height: '6px', borderRadius: '3px' },
  fill: {
    position: 'absolute',
    top: 0,
    bottom: 0,
    left: 0,
    borderRadius: 'inherit',
    transitionProperty: 'width, background-color',
    transitionDuration: '300ms',
    transitionTimingFunction: 'linear',
  },
  downloading: { backgroundColor: tokens.colorCompoundBrandBackground },
  paused: { backgroundColor: tokens.colorPaletteMarigoldBackground3 },
  completed: { backgroundColor: tokens.colorPaletteGreenBackground3 },
  error: { backgroundColor: tokens.colorPaletteRedBackground3 },
  // A soft highlight that travels along a running bar, as in the Windows copy dialog.
  sheen: {
    '::after': {
      content: '""',
      position: 'absolute',
      inset: 0,
      backgroundImage: 'linear-gradient(90deg, transparent 0%, rgba(255, 255, 255, 0.45) 50%, transparent 100%)',
      backgroundSize: '50% 100%',
      backgroundRepeat: 'no-repeat',
      animationName: { from: { backgroundPosition: '-100% 0' }, to: { backgroundPosition: '200% 0' } },
      animationDuration: '1.8s',
      animationTimingFunction: 'ease-in-out',
      animationIterationCount: 'infinite',
      '@media (prefers-reduced-motion: reduce)': { animationName: 'none' },
    },
  },
  indeterminate: {
    width: '33%',
    animationName: { from: { left: '-33%' }, to: { left: '100%' } },
    animationDuration: '1.6s',
    animationTimingFunction: 'ease-in-out',
    animationIterationCount: 'infinite',
    '@media (prefers-reduced-motion: reduce)': { animationName: 'none', width: '100%', opacity: 0.6 },
  },
});

interface DownloadProgressProps {
  status: TaskStatus;
  /** 0..1 */
  fraction: number;
  /** Size unknown while downloading: show motion instead of a fill. */
  indeterminate?: boolean;
  size?: 'row' | 'large';
  className?: string;
}

/** Status-colored progress bar with a visible track; a running bar carries a moving sheen. */
export const DownloadProgress: React.FC<DownloadProgressProps> = ({
  status,
  fraction,
  indeterminate = false,
  size = 'row',
  className,
}) => {
  const styles = useStyles();
  const pct = Math.max(0, Math.min(1, fraction)) * 100;
  // Any progress at all stays visible as at least a dot, so a fresh download reads as started.
  const width = pct > 0 ? `max(${pct}%, ${size === 'large' ? 6 : 4}px)` : '0';
  return (
    <div
      className={mergeClasses(styles.track, styles[size], className)}
      role="progressbar"
      aria-label={statusText[status]}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : Math.floor(pct)}
    >
      <div
        className={mergeClasses(
          styles.fill,
          styles[status],
          indeterminate ? styles.indeterminate : status === 'downloading' && pct > 0 && styles.sheen
        )}
        style={indeterminate ? undefined : { width }}
      />
    </div>
  );
};
