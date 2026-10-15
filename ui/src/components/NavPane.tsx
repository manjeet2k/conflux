import React from 'react';
import { makeStyles, mergeClasses, tokens, Tooltip } from '@fluentui/react-components';
import {
  ArrowDownload20Regular,
  ArrowDownload20Filled,
  Apps20Regular,
  Apps20Filled,
  Pause20Regular,
  Pause20Filled,
  CheckmarkCircle20Regular,
  CheckmarkCircle20Filled,
  ErrorCircle20Regular,
  ErrorCircle20Filled,
  Router20Regular,
  Router20Filled,
  Settings20Regular,
  Settings20Filled,
  Navigation20Regular,
  bundleIcon,
} from '@fluentui/react-icons';
import type { FluentIcon } from '@fluentui/react-icons';
import type { ViewId } from '../types';

const icons: Record<ViewId, FluentIcon> = {
  all: bundleIcon(Apps20Filled, Apps20Regular),
  active: bundleIcon(ArrowDownload20Filled, ArrowDownload20Regular),
  paused: bundleIcon(Pause20Filled, Pause20Regular),
  completed: bundleIcon(CheckmarkCircle20Filled, CheckmarkCircle20Regular),
  failed: bundleIcon(ErrorCircle20Filled, ErrorCircle20Regular),
  network: bundleIcon(Router20Filled, Router20Regular),
  settings: bundleIcon(Settings20Filled, Settings20Regular),
};

const labels: Record<ViewId, string> = {
  all: 'All downloads',
  active: 'Downloading',
  paused: 'Paused',
  completed: 'Completed',
  failed: 'Failed',
  network: 'Network',
  settings: 'Settings',
};

const useStyles = makeStyles({
  root: {
    position: 'relative',
    display: 'flex',
    flexDirection: 'column',
    padding: '4px 4px 8px',
    gap: '2px',
    flexShrink: 0,
    transitionProperty: 'width',
    transitionDuration: tokens.durationNormal,
    transitionTimingFunction: tokens.curveEasyEase,
    overflow: 'hidden',
  },
  expanded: { width: '240px' },
  compact: { width: '48px' },
  item: {
    position: 'relative',
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    height: '36px',
    padding: '0 10px 0 12px',
    border: 'none',
    borderRadius: tokens.borderRadiusMedium,
    background: 'transparent',
    color: tokens.colorNeutralForeground1,
    fontFamily: tokens.fontFamilyBase,
    fontSize: tokens.fontSizeBase300,
    textAlign: 'left',
    cursor: 'default',
    whiteSpace: 'nowrap',
    flexShrink: 0,
    transitionProperty: 'background-color, color',
    transitionDuration: `${tokens.durationFaster}, ${tokens.durationFaster}`,
    transitionTimingFunction: `${tokens.curveEasyEase}, ${tokens.curveEasyEase}`,
    ':hover': { backgroundColor: 'var(--cfx-subtle-hover)' },
    ':active': {
      backgroundColor: 'var(--cfx-subtle-pressed)',
      color: tokens.colorNeutralForeground2,
    },
    ':focus-visible': {
      outline: `2px solid ${tokens.colorStrokeFocus2}`,
      outlineOffset: '-2px',
    },
  },
  selected: {
    backgroundColor: 'var(--cfx-subtle-hover)',
  },
  indicator: {
    position: 'absolute',
    left: '4px',
    top: 0,
    width: '3px',
    height: '16px',
    borderRadius: '2px',
    backgroundColor: tokens.colorBrandBackground,
    pointerEvents: 'none',
    zIndex: 1,
    transformOrigin: '50% 50%',
    animationName: 'win11NavPill',
    animationDuration: 'var(--cfx-nav-dur, 240ms)',
    animationTimingFunction: 'cubic-bezier(0.1, 0.9, 0.2, 1.0)',
    animationFillMode: 'forwards',
    '@media (prefers-reduced-motion: reduce)': {
      animation: 'none',
      transform: 'translate3d(0, var(--cfx-nav-to, 0px), 0)',
    },
  },
  indicatorImmediate: {
    animation: 'none',
  },
  label: { flex: 1, overflow: 'hidden', textOverflow: 'ellipsis' },
  count: {
    fontSize: tokens.fontSizeBase200,
    color: tokens.colorNeutralForeground3,
    fontVariantNumeric: 'tabular-nums',
  },
  icon: { display: 'flex', flexShrink: 0 },
  spacer: { flex: 1 },
  divider: {
    height: '1px',
    margin: '6px 8px',
    backgroundColor: 'var(--cfx-divider)',
    flexShrink: 0,
  },
});

interface NavPaneProps {
  view: ViewId;
  onSelect: (view: ViewId) => void;
  counts: Record<'all' | 'active' | 'paused' | 'completed' | 'failed', number>;
  compact: boolean;
  onToggleCompact: () => void;
}

interface IndicatorState {
  fromY: number;
  toY: number;
  stretch: number;
  durationMs: number;
  animKey: number;
  ready: boolean;
  animate: boolean;
}

export const NavPane: React.FC<NavPaneProps> = ({ view, onSelect, counts, compact, onToggleCompact }) => {
  const styles = useStyles();
  const navRef = React.useRef<HTMLElement | null>(null);
  const itemRefs = React.useRef<Partial<Record<ViewId, HTMLButtonElement | null>>>({});
  const isMountedRef = React.useRef(false);
  const [indicatorPos, setIndicatorPos] = React.useState<IndicatorState>({
    fromY: 0,
    toY: 0,
    stretch: 1,
    durationMs: 0,
    animKey: 0,
    ready: false,
    animate: false,
  });

  const updateIndicator = React.useCallback(
    (shouldAnimate: boolean) => {
      const button = itemRefs.current[view];
      if (!button) return;
      const targetY = button.offsetTop + Math.round((button.offsetHeight - 16) / 2);

      setIndicatorPos((prev) => {
        if (prev.ready && prev.toY === targetY && !shouldAnimate) {
          return prev;
        }
        if (!shouldAnimate || !prev.ready) {
          return {
            fromY: targetY,
            toY: targetY,
            stretch: 1,
            durationMs: 0,
            animKey: prev.animKey,
            ready: true,
            animate: false,
          };
        }
        if (prev.toY === targetY) {
          return prev;
        }

        const distance = Math.abs(targetY - prev.toY);
        // WinUI 3 stretch factor: scales with travel distance up to ~2.2x
        const stretch = Math.min(1.0 + (distance / 36) * 0.45, 2.2);
        // Fluent motion timing: 200ms for adjacent tabs, up to 270ms for distant tabs
        const durationMs = distance < 50 ? 200 : 270;

        return {
          fromY: prev.toY,
          toY: targetY,
          stretch,
          durationMs,
          animKey: prev.animKey + 1,
          ready: true,
          animate: true,
        };
      });
    },
    [view]
  );

  React.useLayoutEffect(() => {
    if (!isMountedRef.current) {
      updateIndicator(false);
      isMountedRef.current = true;
    } else {
      updateIndicator(true);
    }
  }, [view, updateIndicator]);

  React.useEffect(() => {
    const navEl = navRef.current;
    if (!navEl || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      updateIndicator(false);
    });
    observer.observe(navEl);
    return () => observer.disconnect();
  }, [updateIndicator]);

  const item = (id: ViewId, count?: number) => {
    const Icon = icons[id];
    const selected = view === id;
    const button = (
      <button
        key={id}
        ref={(el) => {
          itemRefs.current[id] = el;
        }}
        className={mergeClasses(styles.item, selected && styles.selected)}
        onClick={() => onSelect(id)}
        aria-current={selected ? 'page' : undefined}
        aria-label={labels[id]}
      >
        <span className={styles.icon}>
          <Icon filled={selected} />
        </span>
        {!compact && <span className={styles.label}>{labels[id]}</span>}
        {!compact && count !== undefined && count > 0 && <span className={styles.count}>{count}</span>}
      </button>
    );
    return compact ? (
      <Tooltip key={id} content={labels[id]} relationship="label" positioning="after">
        {button}
      </Tooltip>
    ) : (
      button
    );
  };

  return (
    <nav ref={navRef} className={mergeClasses(styles.root, compact ? styles.compact : styles.expanded)}>
      <div
        key={indicatorPos.animKey}
        className={mergeClasses(styles.indicator, !indicatorPos.animate && styles.indicatorImmediate)}
        style={
          {
            '--cfx-nav-from': `${indicatorPos.fromY}px`,
            '--cfx-nav-to': `${indicatorPos.toY}px`,
            '--cfx-nav-stretch': indicatorPos.stretch,
            '--cfx-nav-dur': `${indicatorPos.durationMs}ms`,
            opacity: indicatorPos.ready ? 1 : 0,
            transform: !indicatorPos.animate ? `translate3d(0, ${indicatorPos.toY}px, 0)` : undefined,
          } as React.CSSProperties
        }
        aria-hidden="true"
      />
      <button
        className={styles.item}
        onClick={onToggleCompact}
        aria-label={compact ? 'Expand navigation' : 'Collapse navigation'}
        title={compact ? 'Expand navigation' : 'Collapse navigation'}
      >
        <span className={styles.icon}>
          <Navigation20Regular />
        </span>
      </button>
      {item('all', counts.all)}
      {item('active', counts.active)}
      {item('paused', counts.paused)}
      {item('completed', counts.completed)}
      {item('failed', counts.failed)}
      <div className={styles.divider} />
      {item('network')}
      <div className={styles.spacer} />
      {item('settings')}
    </nav>
  );
};
