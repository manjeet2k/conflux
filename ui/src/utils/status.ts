// src/utils/status.ts
import type { TaskStatus } from '../types';

export const statusText: Record<TaskStatus, string> = {
  downloading: 'Downloading',
  stopping: 'Stopping…',
  paused: 'Paused',
  completed: 'Completed',
  error: 'Failed',
};
