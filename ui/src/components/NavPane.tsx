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
    ':hover': { backgroundColor: 'var(--cfx-subtle-hover)' },
    ':active': { backgroundColor: 'var(--cfx-subtle-pressed)', color: tokens.colorNeutralForeground2 },
    ':focus-visible': {
      outline: `2px solid ${tokens.colorStrokeFocus2}`,
      outlineOffset: '-2px',
    },
  },
  selected: {
    backgroundColor: 'var(--cfx-subtle-hover)',
    '::before': {
      content: '""',
      position: 'absolute',
      left: 0,
      top: '10px',
      height: '16px',
      width: '3px',
      borderRadius: '2px',
      backgroundColor: tokens.colorBrandBackground,
    },
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

export const NavPane: React.FC<NavPaneProps> = ({ view, onSelect, counts, compact, onToggleCompact }) => {
  const styles = useStyles();

  const item = (id: ViewId, count?: number) => {
    const Icon = icons[id];
    const selected = view === id;
    const button = (
      <button
        key={id}
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
    <nav className={mergeClasses(styles.root, compact ? styles.compact : styles.expanded)}>
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
