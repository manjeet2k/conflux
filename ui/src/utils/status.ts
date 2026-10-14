import type { TaskStatus } from '../types';

export const statusText: Record<TaskStatus, string> = {
  downloading: 'Downloading',
  paused: 'Paused',
  completed: 'Completed',
  error: 'Failed',
};
