// src/features/downloads/components/DownloadsView.tsx
import React, { useMemo } from 'react';
import { makeStyles, MessageBar, MessageBarBody } from '@fluentui/react-components';
import { Search48Regular, ArrowDownload48Regular, CheckmarkCircle48Regular, ErrorCircle48Regular, PauseCircle48Regular } from '@fluentui/react-icons';
import type { AdapterInfo } from '@/features/network/types/network.types';
import type { DownloadTask, ViewId } from '../types/downloads.types';
import type { RowActions } from './DownloadTable';
import type { SortState } from '@/utils/sort';
import type { SpeedUnit } from '@/utils/formatters';
import { CommandBar } from './CommandBar';
import { DownloadTable } from './DownloadTable';
import { DetailsPane } from './DetailsPane';
import { EmptyState } from '@/components/EmptyState';

const emptyStates = {
  all: {
    icon: ArrowDownload48Regular,
    title: 'No downloads yet',
    body: 'Add a link, or copy one and press Ctrl+V anywhere in Conflux.',
  },
  active: {
    icon: ArrowDownload48Regular,
    title: 'Nothing downloading',
    body: 'Active downloads show up here with live speed per adapter.',
  },
  paused: {
    icon: PauseCircle48Regular,
    title: 'No paused downloads',
    body: 'Paused downloads keep their finished chunks and resume where they left off.',
  },
  completed: {
    icon: CheckmarkCircle48Regular,
    title: 'No completed downloads',
    body: 'Finished files are verified with a SHA-256 checksum.',
  },
  failed: {
    icon: ErrorCircle48Regular,
    title: 'No failed downloads',
    body: 'Downloads that fail after retries on every adapter appear here.',
  },
} as const;

const useStyles = makeStyles({
  content: {
    flex: 1,
    minHeight: 0,
    display: 'flex',
    flexDirection: 'row',
    overflow: 'hidden',
  },
  list: {
    flex: 1,
    minWidth: 0,
    minHeight: 0,
    display: 'flex',
    flexDirection: 'column',
    overflow: 'hidden',
  },
});

export interface DownloadsViewProps {
  tasks: DownloadTask[];
  visible: DownloadTask[];
  counts: {
    all: number;
    active: number;
    paused: number;
    completed: number;
    failed: number;
  };
  selected: Set<string>;
  onSelectionChange: (selected: Set<string>) => void;
  sort: SortState;
  onSort: (sort: SortState) => void;
  speedUnit: SpeedUnit;
  detailsOpen: boolean;
  onDetailsOpenChange: (open: boolean) => void;
  search: string;
  onSearch: (search: string) => void;
  searchRef?: React.RefObject<HTMLInputElement | null>;
  historyError: string | null;
  adapters: AdapterInfo[];
  taskHistory: Record<string, number[]>;
  dark: boolean;
  actions: RowActions;
  view: ViewId;
  onAdd: () => void;
  onResume: (ids: string[]) => void;
  onPause: (ids: string[]) => void;
  onRemove: (ids: string[]) => void;
  onClearCompleted: () => void;
  copy: (text: string, label: string) => void;
}

const isResumable = (t: DownloadTask) => t.status === 'paused' || t.status === 'error';
const isRunning = (t: DownloadTask) => t.status === 'downloading';
const idsWhere = (list: DownloadTask[], pred: (t: DownloadTask) => boolean) =>
  list.filter(pred).map((t) => t.id);

export const DownloadsView: React.FC<DownloadsViewProps> = ({
  tasks,
  visible,
  counts,
  selected,
  onSelectionChange,
  sort,
  onSort,
  speedUnit,
  detailsOpen,
  onDetailsOpenChange,
  search,
  onSearch,
  searchRef,
  historyError,
  adapters,
  taskHistory,
  dark,
  actions,
  view,
  onAdd,
  onResume,
  onPause,
  onRemove,
  onClearCompleted,
  copy,
}) => {
  const styles = useStyles();
  const emptyState = view === 'network' || view === 'settings' ? emptyStates.all : emptyStates[view];
  const chunkColors = useMemo(
    () =>
      dark
        ? { done: '#60CDFF', active: '#C5A8F0', failed: '#FF99A4', pending: 'rgba(255,255,255,0.10)' }
        : { done: '#005FB8', active: '#8764B8', failed: '#C42B1C', pending: 'rgba(0,0,0,0.08)' },
    [dark]
  );

  const selectedTasks = tasks.filter((t) => selected.has(t.id));
  const single = selectedTasks.length === 1 ? selectedTasks[0] : null;

  return (
    <>
      <CommandBar
        canResume={selectedTasks.some(isResumable)}
        canPause={selectedTasks.some(isRunning)}
        canRemove={selectedTasks.length > 0}
        canOpen={single?.status === 'completed'}
        canReveal={!!single}
        canCopyLink={selectedTasks.length > 0}
        anyPaused={tasks.some(isResumable)}
        anyActive={tasks.some(isRunning)}
        anyCompleted={counts.completed > 0}
        onAdd={onAdd}
        onResume={() => onResume(idsWhere(selectedTasks, isResumable))}
        onPause={() => onPause(idsWhere(selectedTasks, isRunning))}
        onRemove={() => onRemove(selectedTasks.map((t) => t.id))}
        onOpen={() => single && actions.onOpen(single)}
        onReveal={() => single && actions.onReveal(single)}
        onCopyLink={() => actions.onCopyLink(selectedTasks)}
        onResumeAll={() => onResume(idsWhere(tasks, isResumable))}
        onPauseAll={() => onPause(idsWhere(tasks, isRunning))}
        onClearCompleted={onClearCompleted}
        search={search}
        onSearch={onSearch}
        searchRef={searchRef as React.RefObject<HTMLInputElement>}
        detailsOpen={detailsOpen}
        onToggleDetails={() => onDetailsOpenChange(!detailsOpen)}
      />

      {historyError && (
        <MessageBar intent="warning" layout="multiline">
          <MessageBarBody>{historyError}</MessageBarBody>
        </MessageBar>
      )}

      <div className={styles.content}>
        <div className={styles.list}>
          <DownloadTable
            speedUnit={speedUnit}
            tasks={visible}
            sort={sort}
            onSort={onSort}
            selected={selected}
            onSelectionChange={onSelectionChange}
            actions={actions}
            onRowClick={() => onDetailsOpenChange(true)}
            emptyState={
              search.trim() ? (
                <EmptyState
                  icon={Search48Regular}
                  title="No matches"
                  body={`Nothing matches "${search.trim()}".`}
                />
              ) : (
                <EmptyState
                  {...emptyState}
                  onAdd={view === 'all' || view === 'active' ? onAdd : undefined}
                />
              )
            }
          />
        </div>

        {detailsOpen && single && (
          <DetailsPane
            speedUnit={speedUnit}
            task={single}
            adapters={adapters}
            speedHistory={taskHistory}
            dark={dark}
            chunkColors={chunkColors}
            onResume={(ids) => onResume(ids)}
            onPause={(ids) => onPause(ids)}
            onOpen={actions.onOpen}
            onReveal={actions.onReveal}
            onCopy={copy}
            onClose={() => onDetailsOpenChange(false)}
          />
        )}
      </div>
    </>
  );
};
