import React from 'react';
import { Button, makeStyles, tokens } from '@fluentui/react-components';
import { ArrowDown12Regular } from '@fluentui/react-icons';
import { formatSpeed } from '../utils/formatters';
import type { SpeedUnit } from '../utils/formatters';
import type { Settings } from '@/features/settings/types/settings.types';
import { TurtleIcon } from '@/components/TurtleIcon';
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
  controls: { marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: '12px', flexShrink: 0 },
  speed: { display: 'inline-flex', alignItems: 'center', gap: '4px' },
});

export interface StatusBarProps {
  speedUnit: SpeedUnit;
  onSpeedUnitChange: (unit: SpeedUnit) => void;
  active: number;
  queued: number;
  speed: number;
  settings: Settings;
  loaded: boolean;
  onChange: (patch: Partial<Settings>) => void;
  dark?: boolean;
}

export const StatusBar: React.FC<StatusBarProps> = ({
  speedUnit,
  onSpeedUnitChange,
  active,
  queued,
  speed,
  settings,
  loaded,
  onChange,
  dark,
}) => {
  const styles = useStyles();
  const [springCount, setSpringCount] = React.useState(0);

  const handleToggleSlowMode = () => {
    setSpringCount((c) => c + 1);
    onChange({ slow_mode: !settings.slow_mode });
  };

  return (
    <footer className={styles.root} data-theme={dark ? 'dark' : 'light'}>
      <div className={styles.metrics} role="group" aria-label="Download metrics">
        <span>{active} downloading</span>
        {queued > 0 && <span>{queued} queued</span>}
        <span className={styles.speed} title="Total download speed">
          <ArrowDown12Regular />
          {formatSpeed(active > 0 ? speed : 0, speedUnit)}
        </span>
      </div>
      <div className={styles.controls}>
        <div className="cfx-speed-units" data-speed-unit={speedUnit} role="group" aria-label="Speed display unit">
          <Button
            appearance="transparent"
            aria-pressed={speedUnit === 'bytes'}
            title="Display speeds in bytes per second"
            onClick={() => onSpeedUnitChange('bytes')}
          >
            MB/s
          </Button>
          <Button
            appearance="transparent"
            aria-pressed={speedUnit === 'bits'}
            title="Display speeds in megabits per second"
            onClick={() => onSpeedUnitChange('bits')}
          >
            Mb/s
          </Button>
        </div>
        <button
          type="button"
          className={`cfx-slow-mode-tortoise${settings.slow_mode ? ' cfx-slow-mode-active' : ''}`}
          data-active={settings.slow_mode}
          disabled={!loaded}
          aria-label="Slow mode"
          aria-pressed={settings.slow_mode}
          title={`Turn slow mode ${settings.slow_mode ? 'off' : 'on'}`}
          onClick={handleToggleSlowMode}
        >
          <TurtleIcon
            key={springCount}
            active={settings.slow_mode}
            width={34}
            height={28}
            className={`cfx-slow-mode-icon${springCount > 0 ? ' cfx-slow-mode-spring' : ''}`}
          />
        </button>
      </div>
    </footer>
  );
};
