import React from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';
import { ArrowDown12Regular } from '@fluentui/react-icons';
import { formatSpeed } from '../utils/formatters';

const useStyles = makeStyles({
  root: {
    height: '28px',
    flexShrink: 0,
    display: 'flex',
    alignItems: 'center',
    gap: '16px',
    padding: '0 16px',
    borderTop: '1px solid var(--cfx-divider)',
    fontSize: tokens.fontSizeBase200,
    color: tokens.colorNeutralForeground2,
    fontVariantNumeric: 'tabular-nums',
  },
  spacer: { flex: 1 },
  speed: { display: 'inline-flex', alignItems: 'center', gap: '4px' },
});

export const StatusBar: React.FC<{ items: number; selected: number; active: number; speed: number }> = ({
  items,
  selected,
  active,
  speed,
}) => {
  const styles = useStyles();
  return (
    <footer className={styles.root}>
      <span>
        {items} item{items === 1 ? '' : 's'}
      </span>
      {selected > 0 && <span>{selected} selected</span>}
      <span className={styles.spacer} />
      {active > 0 && <span>{active} downloading</span>}
      <span className={styles.speed} title="Total download speed">
        <ArrowDown12Regular />
        {formatSpeed(speed)}
      </span>
    </footer>
  );
};
