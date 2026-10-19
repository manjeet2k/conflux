// src/features/downloads/components/DownloadsView.tsx
import React from 'react';
import { makeStyles, MessageBar, MessageBarBody } from '@fluentui/react-components';
import { Search48Regular } from '@fluentui/react-icons';
import type { AdapterInfo } from '@/features/network/types/network.types';
import type { DownloadTask, ViewId } from '../types/downloads.types';
import type { ChunkMapColors } from './ChunkMap';
import type { RowActions } from './DownloadTable';
import type { SortState } from '../utils/sort';
import type { SpeedUnit } from '@/utils/formatters';
import { CommandBar } from './CommandBar';
import { DownloadTable } from './DownloadTable';
import { DetailsPane } from './DetailsPane';
import { EmptyState } from '@/components/ui/EmptyState';

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
  onToggleDetails: () => void;
  search: string;
  onSearch: (search: string) => void;
  searchRef?: React.RefObject<HTMLInputElement | null>;
  historyError: string | null;
  adapters: AdapterInfo[];
  taskHistory: Record<string, number[]>;
  dark: boolean;
  chunkColors: ChunkMapColors;
  actions: RowActions;
  view: ViewId;
  emptyState: {
    icon: any;
    title: string;
    body: string;
  };
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
  onToggleDetails,
  search,
  onSearch,
  searchRef,
  historyError,
  adapters,
  taskHistory,
  dark,
  chunkColors,
  actions,
  view,
  emptyState,
  onAdd,
  onResume,
  onPause,
  onRemove,
  onClearCompleted,
  copy,
}) => {
  const styles = useStyles();

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
        onToggleDetails={onToggleDetails}
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
            onRowClick={() => onToggleDetails()}
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
            onClose={onToggleDetails}
          />
        )}
      </div>
    </>
  );
};
