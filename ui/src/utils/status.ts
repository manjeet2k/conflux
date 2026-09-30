import type { TaskStatus } from '../types';

export const statusText: Record<TaskStatus, string> = {
  downloading: 'Downloading',
  paused: 'Paused',
  completed: 'Completed',
  error: 'Failed',
};

/** Fluent ProgressBar color per status (Win11: paused = yellow, error = red). */
export const progressColor = (status: TaskStatus) =>
  ({ downloading: 'brand', paused: 'warning', completed: 'success', error: 'error' } as const)[status];
