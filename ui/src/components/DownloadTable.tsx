import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  Menu,
  MenuDivider,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  ProgressBar,
  Table,
  TableBody,
  TableCell,
  TableCellLayout,
  TableHeader,
  TableHeaderCell,
  TableRow,
  makeStyles,
  mergeClasses,
  tokens,
} from '@fluentui/react-components';
import {
  Add20Regular,
  Delete20Regular,
  FolderOpen20Regular,
  Link20Regular,
  Open20Regular,
  Pause20Regular,
  Play20Regular,
  SelectAllOn20Regular,
} from '@fluentui/react-icons';
import type { DownloadTask } from '../types';
import { formatBytes, formatDate, formatEta, formatSpeed, progressOf } from '../utils/formatters';
import { FileIcon } from './FileIcon';
import { StatusLabel } from './StatusLabel';
import { progressColor, statusText } from '../utils/status';
import type { SortKey, SortState } from '../utils/sort';

interface Column {
  key: SortKey;
  label: string;
  width?: number;
  align?: 'end';
  /** Hidden when the list is narrower than this (px), so the name column keeps its room. */
  minListWidth?: number;
}

const NAME_MIN_WIDTH = 220;
/** DOM id of the focusable grid, so callers can return focus to the list. */
export const GRID_ID = 'download-grid';
const columns: Column[] = [
  { key: 'name', label: 'Name' },
  { key: 'size', label: 'Size', width: 88, align: 'end', minListWidth: 640 },
  { key: 'progress', label: 'Progress', width: 180 },
  { key: 'speed', label: 'Speed', width: 100, align: 'end', minListWidth: 760 },
  { key: 'eta', label: 'Time left', width: 88, align: 'end', minListWidth: 860 },
  { key: 'status', label: 'Status', width: 124 },
  { key: 'added', label: 'Date added', width: 136, minListWidth: 1000 },
];

const useStyles = makeStyles({
  scroller: {
    flex: 1,
    overflow: 'auto',
    outline: 'none',
    padding: '0 4px 8px',
  },
  table: { tableLayout: 'fixed' },
  header: {
    position: 'sticky',
    top: 0,
    zIndex: 1,
    backgroundColor: 'var(--cfx-header)',
    backdropFilter: 'blur(20px)',
  },
  headerCell: { fontWeight: tokens.fontWeightRegular, color: tokens.colorNeutralForeground2 },
  row: {
    cursor: 'default',
    borderBottom: 'none',
    ':hover': { backgroundColor: 'var(--cfx-subtle-hover)' },
    ':active': { backgroundColor: 'var(--cfx-subtle-pressed)' },
  },
  selected: {
    backgroundColor: 'var(--cfx-selected)',
    ':hover': { backgroundColor: 'var(--cfx-selected-hover)' },
  },
  focused: {
    outline: `1px solid ${tokens.colorStrokeFocus2}`,
    outlineOffset: '-1px',
  },
  firstCell: {
    position: 'relative',
  },
  pill: {
    position: 'absolute',
    left: '0',
    top: '50%',
    transform: 'translateY(-50%)',
    width: '3px',
    height: '16px',
    borderRadius: '2px',
    backgroundColor: tokens.colorBrandBackground,
  },
  endHeader: { justifyContent: 'flex-end' },
  end: { textAlign: 'right', fontVariantNumeric: 'tabular-nums' },
  num: { fontVariantNumeric: 'tabular-nums', color: tokens.colorNeutralForeground2 },
  progressCell: { display: 'flex', alignItems: 'center', gap: '10px' },
  progressBar: { flex: 1 },
  pausedBar: { backgroundColor: tokens.colorPaletteMarigoldBackground3 },
  percent: {
    width: '40px',
    textAlign: 'right',
    fontVariantNumeric: 'tabular-nums',
    color: tokens.colorNeutralForeground2,
  },
  name: { overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' },
  icon: { color: tokens.colorNeutralForeground2 },
});

export interface RowActions {
  onOpen: (task: DownloadTask) => void;
  onReveal: (task: DownloadTask) => void;
  onResume: (ids: string[]) => void;
  onPause: (ids: string[]) => void;
  onRemove: (ids: string[]) => void;
  onCopyLink: (tasks: DownloadTask[]) => void;
  onAdd: () => void;
}

interface DownloadTableProps {
  tasks: DownloadTask[]; // already filtered and sorted
  sort: SortState;
  onSort: (sort: SortState) => void;
  selected: Set<string>;
  onSelectionChange: (ids: Set<string>) => void;
  actions: RowActions;
}

export const DownloadTable: React.FC<DownloadTableProps> = ({
  tasks,
  sort,
  onSort,
  selected,
  onSelectionChange,
  actions,
}) => {
  const styles = useStyles();
  const [focusId, setFocusId] = useState<string | null>(null);
  const anchorId = useRef<string | null>(null);
  const [showFocus, setShowFocus] = useState(false);

  const [listWidth, setListWidth] = useState(1200);
  const measureRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = measureRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => setListWidth(entry.contentRect.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const shown = columns.filter((c) => !c.minListWidth || listWidth >= c.minListWidth);
  const has = (key: SortKey) => shown.some((c) => c.key === key);
  const tableMinWidth = NAME_MIN_WIDTH + shown.reduce((sum, c) => sum + (c.width ?? 0), 0);

  const ids = useMemo(() => tasks.map((t) => t.id), [tasks]);
  const selectedTasks = tasks.filter((t) => selected.has(t.id));

  const selectOnly = (id: string) => {
    anchorId.current = id;
    setFocusId(id);
    onSelectionChange(new Set([id]));
  };

  const selectRange = (toId: string, additive: boolean) => {
    const from = ids.indexOf(anchorId.current ?? toId);
    const to = ids.indexOf(toId);
    const [lo, hi] = from < 0 ? [to, to] : [Math.min(from, to), Math.max(from, to)];
    const next = new Set(additive ? selected : []);
    ids.slice(lo, hi + 1).forEach((id) => next.add(id));
    setFocusId(toId);
    onSelectionChange(next);
  };

  const onRowClick = (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    setShowFocus(false);
    if (e.shiftKey) {
      selectRange(id, e.ctrlKey || e.metaKey);
    } else if (e.ctrlKey || e.metaKey) {
      const next = new Set(selected);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      anchorId.current = id;
      setFocusId(id);
      onSelectionChange(next);
    } else {
      selectOnly(id);
    }
  };

  const onRowContextMenu = (id: string) => {
    if (!selected.has(id)) selectOnly(id);
  };

  /** Default action, as in Explorer: open finished files, otherwise resume stopped ones. */
  const activate = (task: DownloadTask) => {
    if (task.status === 'completed') actions.onOpen(task);
    else if (task.status === 'paused' || task.status === 'error') actions.onResume([task.id]);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (ids.length === 0) return;
    const current = focusId ? ids.indexOf(focusId) : -1;
    const moveTo = (idx: number) => {
      const id = ids[Math.max(0, Math.min(ids.length - 1, idx))];
      setShowFocus(true);
      if (e.shiftKey) selectRange(id, false);
      else if (e.ctrlKey) setFocusId(id);
      else selectOnly(id);
      document.getElementById(`row-${id}`)?.scrollIntoView({ block: 'nearest' });
    };
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        moveTo(current + 1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        moveTo(current < 0 ? 0 : current - 1);
        break;
      case 'Home':
        e.preventDefault();
        moveTo(0);
        break;
      case 'End':
        e.preventDefault();
        moveTo(ids.length - 1);
        break;
      case ' ':
        e.preventDefault();
        if (e.ctrlKey && focusId) {
          const next = new Set(selected);
          if (next.has(focusId)) next.delete(focusId);
          else next.add(focusId);
          onSelectionChange(next);
        } else if (selectedTasks.some((t) => t.status === 'downloading')) {
          actions.onPause(selectedTasks.filter((t) => t.status === 'downloading').map((t) => t.id));
        } else {
          actions.onResume(selectedTasks.filter((t) => t.status !== 'completed').map((t) => t.id));
        }
        break;
      case 'Enter':
        if (selectedTasks.length === 1) activate(selectedTasks[0]);
        break;
      case 'Delete':
        if (selectedTasks.length > 0) actions.onRemove(selectedTasks.map((t) => t.id));
        break;
      case 'a':
      case 'A':
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          onSelectionChange(new Set(ids));
        }
        break;
      case 'Escape':
        onSelectionChange(new Set());
        break;
    }
  };

  const headerCell = (c: Column) => (
    <TableHeaderCell
      key={c.key}
      className={styles.headerCell}
      button={{ className: c.align === 'end' ? styles.endHeader : undefined }}
      style={c.width ? { width: `${c.width}px` } : undefined}
      sortDirection={sort.key === c.key ? sort.direction : undefined}
      onClick={(e) => {
        // Sorting must not reach the grid's click-to-clear-selection handler.
        e.stopPropagation();
        onSort({
          key: c.key,
          direction:
            sort.key === c.key && sort.direction === 'ascending'
              ? 'descending'
              : sort.key === c.key
                ? 'ascending'
                : c.key === 'name' || c.key === 'status'
                  ? 'ascending'
                  : 'descending',
        });
      }}
    >
      {c.label}
    </TableHeaderCell>
  );

  const single = selectedTasks.length === 1 ? selectedTasks[0] : null;
  const pausable = selectedTasks.filter((t) => t.status === 'downloading').map((t) => t.id);
  const resumable = selectedTasks.filter((t) => t.status === 'paused' || t.status === 'error').map((t) => t.id);

  return (
    <Menu openOnContext>
      <MenuTrigger disableButtonEnhancement>
        <div
          ref={measureRef}
          id={GRID_ID}
          className={styles.scroller}
          tabIndex={0}
          role="grid"
          aria-multiselectable
          aria-label="Downloads"
          onKeyDown={onKeyDown}
          onFocus={() => {
            if (!focusId && ids.length > 0) setFocusId(ids[0]);
          }}
          onClick={() => onSelectionChange(new Set())}
        >
          <Table
            className={styles.table}
            style={{ minWidth: `${tableMinWidth}px` }}
            size="small"
            sortable
            aria-label="Downloads"
          >
            <TableHeader className={styles.header}>
              <TableRow>{shown.map(headerCell)}</TableRow>
            </TableHeader>
            <TableBody>
              {tasks.map((t) => {
                const isSelected = selected.has(t.id);
                const fraction = progressOf(t.downloaded_bytes, t.total_bytes);
                const indeterminate = t.status === 'downloading' && t.total_bytes === 0;
                return (
                  <TableRow
                    key={t.id}
                    id={`row-${t.id}`}
                    aria-selected={isSelected}
                    className={mergeClasses(
                      styles.row,
                      isSelected && styles.selected,
                      showFocus && focusId === t.id && styles.focused
                    )}
                    onClick={(e) => onRowClick(e, t.id)}
                    onDoubleClick={() => activate(t)}
                    onContextMenu={() => onRowContextMenu(t.id)}
                  >
                    <TableCell className={styles.firstCell}>
                      {isSelected && <span className={styles.pill} />}
                      <TableCellLayout media={<FileIcon filename={t.filename} className={styles.icon} />} truncate>
                        <span className={styles.name} title={t.filename}>
                          {t.filename}
                        </span>
                      </TableCellLayout>
                    </TableCell>
                    {has('size') && (
                      <TableCell className={mergeClasses(styles.end, styles.num)}>
                        {t.total_bytes > 0 ? formatBytes(t.total_bytes) : '--'}
                      </TableCell>
                    )}
                    <TableCell>
                      <div className={styles.progressCell}>
                        <ProgressBar
                          className={styles.progressBar}
                          bar={{ className: t.status === 'paused' ? styles.pausedBar : undefined }}
                          value={indeterminate ? undefined : fraction}
                          color={progressColor(t.status)}
                          thickness="medium"
                          aria-label={`${statusText[t.status]} ${Math.floor(fraction * 100)}%`}
                        />
                        <span className={styles.percent}>
                          {t.total_bytes > 0 ? `${Math.floor(fraction * 100)}%` : ''}
                        </span>
                      </div>
                    </TableCell>
                    {has('speed') && (
                      <TableCell className={mergeClasses(styles.end, styles.num)}>
                        {t.status === 'downloading' ? formatSpeed(t.speed_bytes_sec) : ''}
                      </TableCell>
                    )}
                    {has('eta') && (
                      <TableCell className={mergeClasses(styles.end, styles.num)}>
                        {t.status === 'downloading' ? formatEta(t.eta_seconds) : ''}
                      </TableCell>
                    )}
                    <TableCell title={t.error ?? undefined}>
                      <StatusLabel status={t.status} />
                    </TableCell>
                    {has('added') && <TableCell className={styles.num}>{formatDate(t.created_at_ms)}</TableCell>}
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </div>
      </MenuTrigger>
      <MenuPopover>
        {selectedTasks.length === 0 ? (
          <MenuList>
            <MenuItem icon={<Add20Regular />} secondaryContent="Ctrl+N" onClick={actions.onAdd}>
              Add download
            </MenuItem>
            <MenuItem
              icon={<SelectAllOn20Regular />}
              secondaryContent="Ctrl+A"
              disabled={ids.length === 0}
              onClick={() => onSelectionChange(new Set(ids))}
            >
              Select all
            </MenuItem>
          </MenuList>
        ) : (
          <MenuList>
            <MenuItem
              icon={<Open20Regular />}
              secondaryContent="Enter"
              disabled={!single || single.status !== 'completed'}
              onClick={() => single && actions.onOpen(single)}
            >
              Open
            </MenuItem>
            <MenuItem
              icon={<FolderOpen20Regular />}
              disabled={!single}
              onClick={() => single && actions.onReveal(single)}
            >
              Show in folder
            </MenuItem>
            <MenuDivider />
            <MenuItem icon={<Play20Regular />} disabled={resumable.length === 0} onClick={() => actions.onResume(resumable)}>
              Resume
            </MenuItem>
            <MenuItem icon={<Pause20Regular />} disabled={pausable.length === 0} onClick={() => actions.onPause(pausable)}>
              Pause
            </MenuItem>
            <MenuItem icon={<Link20Regular />} onClick={() => actions.onCopyLink(selectedTasks)}>
              Copy download link
            </MenuItem>
            <MenuDivider />
            <MenuItem
              icon={<Delete20Regular />}
              secondaryContent="Del"
              onClick={() => actions.onRemove(selectedTasks.map((t) => t.id))}
            >
              Remove
            </MenuItem>
          </MenuList>
        )}
      </MenuPopover>
    </Menu>
  );
};
