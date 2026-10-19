// src/features/downloads/components/DownloadProgress.tsx
import React from 'react';
import { makeStyles, mergeClasses, tokens } from '@fluentui/react-components';
import type { TaskStatus } from '../types/downloads.types';
import { statusText } from '@/utils/status';

const useStyles = makeStyles({
  track: {
    position: 'relative',
    overflow: 'hidden',
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
    // Hue-shifting gradient keeps the entire pill saturated end-to-end
    backgroundImage: 'var(--cfx-bar-gradient)',
    transitionProperty: 'width',
    transitionDuration: '400ms',
    transitionTimingFunction: 'cubic-bezier(0.2, 0, 0, 1)',
    '@media (prefers-reduced-motion: reduce)': { transitionDuration: '0ms' },
  },
  // Vibrant Pill Gradients (Clean hue-shifts keeping the entire bar saturated end-to-end, reversed)
  downloading: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #22D3EE 0%, #3B82F6 100%)',
    '--cfx-bar': '#3B82F6',
  },
  stopping: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #FDE047 0%, #F59E0B 100%)',
    '--cfx-bar': '#F59E0B',
  },
  paused: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #FDE047 0%, #F59E0B 100%)',
    '--cfx-bar': '#F59E0B',
  },
  completed: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #A3E635 0%, #10B981 100%)',
    '--cfx-bar': '#10B981',
  },
  error: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #FB923C 0%, #E11D48 100%)',
    '--cfx-bar': '#E11D48',
  },
  // Multi-stop mesh / liquid variants (Electric Aurora & Neon Citrus, reversed)
  auroraDownloading: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #F59E0B 0%, #EC4899 50%, #6366F1 100%)',
    '--cfx-bar': '#6366F1',
  },
  auroraPaused: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #FEF08A 0%, #FACC15 45%, #F97316 100%)',
    '--cfx-bar': '#F97316',
  },
  // Muted / earthy pastel variants (for soft frosted light mode, reversed)
  pastelDownloading: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #43978D 0%, #264D59 100%)',
    '--cfx-bar': '#264D59',
  },
  pastelCompleted: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #F9E07F 0%, #43978D 100%)',
    '--cfx-bar': '#43978D',
  },
  pastelPaused: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #F9E07F 0%, #F9AD6A 100%)',
    '--cfx-bar': '#F9AD6A',
  },
  pastelError: {
    '--cfx-bar-gradient': 'linear-gradient(90deg, #F9AD6A 0%, #D46C4E 100%)',
    '--cfx-bar': '#D46C4E',
  },
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
      backgroundImage:
        'linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.16) 30%, rgba(255, 255, 255, 0.40) 50%, rgba(255, 255, 255, 0.16) 70%, transparent)',
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

export type GradientVariant = 'vibrant' | 'aurora' | 'pastel';

interface DownloadProgressProps {
  status: TaskStatus;
  /** 0..1 */
  fraction: number;
  /** Size unknown while downloading: show motion instead of a fill. */
  indeterminate?: boolean;
  size?: 'row' | 'large';
  /** Text shown inside the bar, e.g. the percentage. */
  label?: string;
  /** Gradient theme: vibrant (default hue-shift), aurora (liquid mesh), or pastel (soft muted). */
  variant?: GradientVariant;
  className?: string;
}

/** Status-colored gradient progress bar; shifting adjacent hues keep the bar saturated end-to-end. */
export const DownloadProgress: React.FC<DownloadProgressProps> = ({
  status,
  fraction,
  indeterminate = false,
  size = 'row',
  variant = 'vibrant',
  label,
  className,
}) => {
  const styles = useStyles();
  const pct = Math.max(0, Math.min(1, fraction)) * 100;
  // Any progress at all stays visible as at least a dot, so a fresh download reads as started.
  const minFill = label !== undefined ? 16 : size === 'large' ? 10 : 8;
  const width = pct > 0 ? `max(${pct}%, ${minFill}px)` : '0';

  const defaultStatusClass =
    status === 'downloading'
      ? styles.downloading
      : status === 'stopping'
        ? styles.stopping
        : status === 'paused'
          ? styles.paused
          : status === 'completed'
            ? styles.completed
            : styles.error;

  const statusVariantClass =
    variant === 'aurora'
      ? status === 'downloading'
        ? styles.auroraDownloading
        : status === 'paused' || status === 'stopping'
          ? styles.auroraPaused
          : defaultStatusClass
      : variant === 'pastel'
        ? status === 'downloading'
          ? styles.pastelDownloading
          : status === 'completed'
            ? styles.pastelCompleted
            : status === 'paused' || status === 'stopping'
              ? styles.pastelPaused
              : styles.pastelError
        : defaultStatusClass;

  return (
    <div
      className={mergeClasses(
        styles.track,
        styles[size],
        label !== undefined && styles.labeled,
        statusVariantClass,
        indeterminate && styles.clip,
        className
      )}
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
