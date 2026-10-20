import { useMemo, useRef, useState } from 'react';
import { usePref } from '@/hooks/usePref';
import { sortTasks } from '@/utils/sort';

import type { SortState } from '@/utils/sort';
import type { DownloadTask, TaskStatus, ViewId } from '../types/downloads.types';

const viewStatuses: Partial<Record<ViewId, TaskStatus[]>> = {
  active: ['downloading'],
  paused: ['paused', 'stopping'],
  completed: ['completed'],
  failed: ['error'],
};

export function useDownloadList(
  tasks: DownloadTask[], view: ViewId,
  selected: Set<string>, onSelectionChange: (selected: Set<string>) => void,
) {
  const [detailsOpen, setDetailsOpen] = usePref('cfx.details', true);
  const [search, setSearch] = useState('');
  const [sort, setSort] = useState<SortState>({ key: 'added', direction: 'descending' });
  const searchRef = useRef<HTMLInputElement>(null);

  const counts = useMemo(
    () => ({
      all: tasks.length,
      active: tasks.filter((t) => t.status === 'downloading').length,
      paused: tasks.filter((t) => t.status === 'paused' || t.status === 'stopping').length,
      completed: tasks.filter((t) => t.status === 'completed').length,
      failed: tasks.filter((t) => t.status === 'error').length,
    }),
    [tasks]
  );

  const visible = useMemo(() => {
    const statuses = viewStatuses[view];
    const q = search.trim().toLowerCase();
    const filtered = tasks.filter(
      (t) =>
        (!statuses || statuses.includes(t.status)) &&
        (!q || t.filename.toLowerCase().includes(q) || t.url.toLowerCase().includes(q))
    );
    return sortTasks(filtered, sort);
  }, [tasks, view, search, sort]);

  const totalSpeed = tasks.reduce((s, t) => s + (t.status === 'downloading' ? t.speed_bytes_sec : 0), 0);

  return {
    counts, totalSpeed,
    viewProps: {
      visible, selected, onSelectionChange, sort, onSort: setSort,
      detailsOpen, onDetailsOpenChange: setDetailsOpen,
      search, onSearch: setSearch, searchRef,
    },
  };
}
