import React from 'react';
import { makeStyles, mergeClasses, tokens } from '@fluentui/react-components';
import type { TaskStatus } from '../types';
import { statusText } from '../utils/status';

const useStyles = makeStyles({
  // Not clipped, so the fill's glow can bloom past the track; only the indeterminate sweep needs clipping.
  track: {
    position: 'relative',
    backgroundColor: 'var(--cfx-track)',
    borderRadius: tokens.borderRadiusCircular,
  },
  clip: { overflow: 'hidden' },
  row: { height: '8px' },
  large: { height: '10px' },
  // Tall enough to carry its own percentage.
  labeled: { height: '16px' },
  label: {
    position: 'absolute',
    inset: 0,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'flex-end',
    paddingRight: '8px',
    fontSize: '11px',
    lineHeight: 1,
    fontWeight: tokens.fontWeightSemibold,
    fontVariantNumeric: 'tabular-nums',
    color: tokens.colorNeutralForeground2,
    pointerEvents: 'none',
    userSelect: 'none',
  },
  // A second copy of the label, clipped to the fill, so the text stays legible on either side of the edge.
  labelOnFill: {
    color: '#ffffff',
    textShadow: '0 1px 2px rgba(0, 0, 0, 0.35)',
    transitionProperty: 'clip-path',
    transitionDuration: '400ms',
    transitionTimingFunction: 'cubic-bezier(0.2, 0, 0, 1)',
    '@media (prefers-reduced-motion: reduce)': { transitionDuration: '0ms' },
  },
  fill: {
    position: 'absolute',
    overflow: 'hidden',
    containerType: 'inline-size',
    top: 0,
    bottom: 0,
    left: 0,
    borderRadius: 'inherit',
    // Rises from a soft tint into the full status color, with a gentle halo.
    backgroundImage: 'linear-gradient(90deg, color-mix(in srgb, var(--cfx-bar) 40%, transparent), var(--cfx-bar))',
    boxShadow: '0 0 6px color-mix(in srgb, var(--cfx-bar) 20%, transparent)',
    transitionProperty: 'width',
    transitionDuration: '400ms',
    transitionTimingFunction: 'cubic-bezier(0.2, 0, 0, 1)',
    '@media (prefers-reduced-motion: reduce)': { transitionDuration: '0ms' },
  },
  // An unfinished bar brightens slightly into a lit head at its leading edge.
  head: {
    backgroundImage:
      'linear-gradient(90deg, color-mix(in srgb, var(--cfx-bar) 40%, transparent), var(--cfx-bar) 75%, color-mix(in srgb, var(--cfx-bar) 85%, white))',
    boxShadow: '0 0 6px color-mix(in srgb, var(--cfx-bar) 25%, transparent)',
  },
  // Vivid status tones; the Fluent palette backgrounds read too dark on the acrylic surface.
  downloading: { '--cfx-bar': '#4096e0' },
  stopping: { '--cfx-bar': '#e8ae45' },
  paused: { '--cfx-bar': '#e8ae45' },
  completed: { '--cfx-bar': '#3dbb75' },
  error: { '--cfx-bar': '#e5636f' },
  // Clip the soft highlight to the fill and leave a quiet interval between sweeps.
  sheen: {
    '::after': {
      content: '""',
      // A sweep compressed into the minimum-width fill looks like a flicker.
      display: 'none',
      '@container (min-width: 48px)': { display: 'block' },
      position: 'absolute',
      top: 0,
      bottom: 0,
      left: 0,
      width: '45%',
      backgroundImage: 'linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.14) 30%, rgba(255, 255, 255, 0.35) 50%, rgba(255, 255, 255, 0.14) 70%, transparent)',
      animationName: {
        '0%': { transform: 'translateX(-100%)', opacity: 0 },
        '12%': { opacity: 1 },
        '65%': { opacity: 1 },
        '80%, 100%': { transform: 'translateX(225%)', opacity: 0 },
      },
      animationDuration: '2.6s',
      animationTimingFunction: 'linear',
      animationIterationCount: 'infinite',
      '@media (prefers-reduced-motion: reduce)': { animationName: 'none', opacity: 0 },
    },
  },
  indeterminate: {
    width: '33%',
    // Translation is relative to the segment: 304% of 33% clears the whole track.
    animationName: { from: { transform: 'translateX(-100%)' }, to: { transform: 'translateX(304%)' } },
    animationDuration: '1.8s',
    animationTimingFunction: 'cubic-bezier(0.4, 0, 0.2, 1)',
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
  /** Text shown inside the bar, e.g. the percentage. */
  label?: string;
  className?: string;
}

/** Status-colored gradient progress bar; an unfinished bar has a glowing head, a running one a moving sheen. */
export const DownloadProgress: React.FC<DownloadProgressProps> = ({
  status,
  fraction,
  indeterminate = false,
  size = 'row',
  label,
  className,
}) => {
  const styles = useStyles();
  const pct = Math.max(0, Math.min(1, fraction)) * 100;
  // Any progress at all stays visible as at least a dot, so a fresh download reads as started.
  const minFill = label !== undefined ? 16 : size === 'large' ? 10 : 8;
  const width = pct > 0 ? `max(${pct}%, ${minFill}px)` : '0';
  return (
    <div
      className={mergeClasses(styles.track, styles[size], label !== undefined && styles.labeled, styles[status], indeterminate && styles.clip, className)}
      role="progressbar"
      aria-label={statusText[status]}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : Math.floor(pct)}
      aria-valuetext={label}
    >
      <div
        className={mergeClasses(
          styles.fill,
          (indeterminate || pct < 100) && styles.head,
          indeterminate ? styles.indeterminate : status === 'downloading' && pct > 0 && styles.sheen
        )}
        style={indeterminate ? undefined : { width }}
      />
      {label && <span className={styles.label}>{label}</span>}
      {label && !indeterminate && (
        <span
          className={mergeClasses(styles.label, styles.labelOnFill)}
          style={{ clipPath: `inset(0 calc(100% - ${width === '0' ? '0px' : width}) 0 0)` }}
          aria-hidden
        >
          {label}
        </span>
      )}
    </div>
  );
};
