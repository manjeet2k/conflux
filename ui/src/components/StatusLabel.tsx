import React from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';
import {
  ArrowCircleDown16Filled,
  PauseCircle16Filled,
  CheckmarkCircle16Filled,
  ErrorCircle16Filled,
} from '@fluentui/react-icons';
import type { TaskStatus } from '../types';
import { statusText } from '../utils/status';

const useStyles = makeStyles({
  root: { display: 'inline-flex', alignItems: 'center', gap: '6px', whiteSpace: 'nowrap' },
  downloading: { color: tokens.colorBrandForeground1 },
  paused: { color: tokens.colorPaletteMarigoldForeground1 },
  completed: { color: tokens.colorPaletteGreenForeground1 },
  error: { color: tokens.colorPaletteRedForeground1 },
});

export const StatusLabel: React.FC<{ status: TaskStatus }> = ({ status }) => {
  const styles = useStyles();
  const Icon = {
    downloading: ArrowCircleDown16Filled,
    paused: PauseCircle16Filled,
    completed: CheckmarkCircle16Filled,
    error: ErrorCircle16Filled,
  }[status];
  return (
    <span className={styles.root}>
      <Icon className={styles[status]} />
      <span>{statusText[status]}</span>
    </span>
  );
};
