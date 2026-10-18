import React from 'react';
import {
  Menu, MenuItemRadio, MenuList, MenuPopover, MenuTrigger,
  SplitButton, makeStyles, mergeClasses, tokens,
} from '@fluentui/react-components';
import { ArrowDown12Regular } from '@fluentui/react-icons';
import { formatSpeed } from '../utils/formatters';
import type { Settings } from '../types';
import { TurtleIcon } from './TurtleIcon';
import './StatusBar.css';

const useStyles = makeStyles({
  root: {
    minHeight: '46px',
    flexShrink: 0,
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    flexWrap: 'wrap',
    boxSizing: 'border-box',
    padding: '6px 12px',
    borderTop: '1px solid var(--cfx-divider)',
    fontSize: tokens.fontSizeBase200,
    color: tokens.colorNeutralForeground2,
    fontVariantNumeric: 'tabular-nums',
  },
  metrics: { display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: '12px' },
  slowMode: { marginLeft: 'auto', flexShrink: 0 },
  speed: { display: 'inline-flex', alignItems: 'center', gap: '4px' },
});

interface StatusBarProps {
  active: number;
  queued: number;
  speed: number;
  settings: Settings;
  loaded: boolean;
  onChange: (patch: Partial<Settings>) => void;
}

export const StatusBar: React.FC<StatusBarProps> = ({
  active,
  queued,
  speed,
  settings,
  loaded,
  onChange,
}) => {
  const styles = useStyles();
  const limits: Record<string, string> = Object.fromEntries(
    [128, 256, 500, 1024, 2048, 5120, 10240, settings.slow_mode_limit_kib]
      .sort((a, b) => a - b)
      .map((limit) => [String(limit), formatSpeed(limit * 1024)])
  );
  return (
    <footer className={styles.root}>
      <div className={styles.metrics} role="group" aria-label="Download metrics">
        <span>{active} downloading</span>
        {queued > 0 && <span>{queued} queued</span>}
        <span className={styles.speed} title="Total download speed">
          <ArrowDown12Regular />
          {formatSpeed(active > 0 ? speed : 0)}
        </span>
      </div>
      <Menu
        positioning={{ position: 'above', align: 'end' }}
        checkedValues={{ speed: [String(settings.slow_mode_limit_kib)] }}
      >
        <MenuTrigger disableButtonEnhancement>
          {(triggerProps) => (
            <SplitButton
              className={mergeClasses(styles.slowMode, 'cfx-slow-mode')}
              appearance="transparent"
              icon={{ children: <TurtleIcon active={settings.slow_mode} />, style: { width: 32, height: 26 } }}
              disabled={!loaded}
              primaryActionButton={{
                'aria-label': 'Slow mode',
                'aria-pressed': settings.slow_mode,
                title: `Turn slow mode ${settings.slow_mode ? 'off' : 'on'}`,
                onClick: () => onChange({ slow_mode: !settings.slow_mode }),
              }}
              menuButton={{
                ...triggerProps,
                'aria-label': 'Slow mode speed limit across all downloads',
                title: 'Choose slow mode speed limit',
              }}
            >
              Slow mode
            </SplitButton>
          )}
        </MenuTrigger>
        <MenuPopover>
          <MenuList>
            {Object.entries(limits).map(([value, label]) => (
              <MenuItemRadio
                key={value}
                name="speed"
                value={value}
                onClick={() => onChange({ slow_mode_limit_kib: Number(value) })}
              >
                {label}
              </MenuItemRadio>
            ))}
          </MenuList>
        </MenuPopover>
      </Menu>
    </footer>
  );
};
