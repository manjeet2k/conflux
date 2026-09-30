import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  FluentProvider,
  Toast,
  ToastTitle,
  Toaster,
  makeStyles,
  tokens,
  useId,
  useToastController,
} from '@fluentui/react-components';
import type { ToastIntent } from '@fluentui/react-components';
import {
  ArrowDownload48Regular,
  CheckmarkCircle48Regular,
  ErrorCircle48Regular,
  PauseCircle48Regular,
  Search48Regular,
} from '@fluentui/react-icons';
import { listen } from '@tauri-apps/api/event';
import { api, errorText } from './api';
import type { AdapterInfo, DownloadTask, TaskStatus, ViewId } from './types';
import { NETWORK_ADAPTERS_CHANGED_EVENT } from './types';
import { darkTheme, lightTheme, surfaceVars } from './theme';
import { useDownloads } from './hooks/useDownloads';
import { useSettings } from './hooks/useSettings';
import { useWindowTheme } from './hooks/useWindowTheme';
import { useSpeedHistory } from './hooks/useSpeedHistory';
import { TitleBar } from './components/TitleBar';
import { NavPane } from './components/NavPane';
import { CommandBar } from './components/CommandBar';
import { DownloadTable, GRID_ID } from './components/DownloadTable';
import type { RowActions } from './components/DownloadTable';
import { sortTasks } from './utils/sort';
import type { SortState } from './utils/sort';
import { DetailsPane } from './components/DetailsPane';
import { AddDownloadDialog } from './components/AddDownloadDialog';
import { RemoveDialog } from './components/RemoveDialog';
import { StatusBar } from './components/StatusBar';
import { EmptyState } from './components/EmptyState';
import { NetworkPage } from './pages/NetworkPage';
import { SettingsPage } from './pages/SettingsPage';
import { copyText, extractUrl } from './utils/files';

const useStyles = makeStyles({
  root: {
    height: '100vh',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: 'var(--cfx-window)',
    color: tokens.colorNeutralForeground1,
    overflow: 'hidden',
  },
  body: { flex: 1, display: 'flex', minHeight: 0 },
  // Win11 layering: the content "layer" sits on the backdrop with a rounded top-left corner.
  layer: {
    flex: 1,
    minWidth: 0,
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: 'var(--cfx-layer)',
    borderTop: '1px solid var(--cfx-stroke)',
    borderLeft: '1px solid var(--cfx-stroke)',
    borderTopLeftRadius: tokens.borderRadiusXLarge,
    overflow: 'hidden',
  },
  content: { flex: 1, display: 'flex', minHeight: 0 },
  list: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' },
});

const viewStatuses: Partial<Record<ViewId, TaskStatus>> = {
  active: 'downloading',
  paused: 'paused',
  completed: 'completed',
  failed: 'error',
};

const emptyStates = {
  all: { icon: ArrowDownload48Regular, title: 'No downloads yet', body: 'Add a link, or copy one and press Ctrl+V anywhere in Conflux.' },
  active: { icon: ArrowDownload48Regular, title: 'Nothing downloading', body: 'Active downloads show up here with live speed per adapter.' },
  paused: { icon: PauseCircle48Regular, title: 'No paused downloads', body: 'Paused downloads keep their finished chunks and resume where they left off.' },
  completed: { icon: CheckmarkCircle48Regular, title: 'No completed downloads', body: 'Finished files are verified with a SHA-256 checksum.' },
  failed: { icon: ErrorCircle48Regular, title: 'No failed downloads', body: 'Downloads that fail after retries on every adapter appear here.' },
} as const;

const isEditable = (el: EventTarget | null) =>
  el instanceof HTMLElement && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName));

const readPref = (key: string, fallback: boolean) => {
  const v = localStorage.getItem(key);
  return v === null ? fallback : v === '1';
};

export const App: React.FC = () => {
  const toasterId = useId('toaster');
  const { dispatchToast } = useToastController(toasterId);
  const notify = useCallback(
    (intent: ToastIntent, title: string) =>
      dispatchToast(
        <Toast>
          <ToastTitle>{title}</ToastTitle>
        </Toast>,
        { intent, timeout: intent === 'error' ? 8000 : 3000 }
      ),
    [dispatchToast]
  );
  const onError = useCallback((message: string) => notify('error', message), [notify]);

  const { settings, update: updateSettings } = useSettings(onError);
  const { dark, mica } = useWindowTheme(settings.theme);
  const { tasks, start, pause, resume, remove } = useDownloads(onError);
  const history = useSpeedHistory(tasks);
  const styles = useStyles();

  const [adapters, setAdapters] = useState<AdapterInfo[]>([]);
  const [view, setView] = useState<ViewId>('all');
  const [compactNav, setCompactNav] = useState(() => readPref('cfx.compactNav', window.innerWidth < 1100));
  const [detailsOpen, setDetailsOpen] = useState(() => readPref('cfx.details', true));
  const [search, setSearch] = useState('');
  const [sort, setSort] = useState<SortState>({ key: 'added', direction: 'descending' });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [addOpen, setAddOpen] = useState(false);
  const [addUrl, setAddUrl] = useState('');
  const [removeIds, setRemoveIds] = useState<string[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => localStorage.setItem('cfx.compactNav', compactNav ? '1' : '0'), [compactNav]);
  useEffect(() => localStorage.setItem('cfx.details', detailsOpen ? '1' : '0'), [detailsOpen]);

  const refreshAdapters = useCallback(() => {
    api
      .discoverAdapters()
      .then(setAdapters)
      .catch((e) => onError(`Failed to discover network adapters: ${errorText(e)}`));
  }, [onError]);
  useEffect(refreshAdapters, [refreshAdapters]);

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;
    listen<AdapterInfo[]>(NETWORK_ADAPTERS_CHANGED_EVENT, (event) => {
      setAdapters(event.payload);
    })
      .then((fn) => {
        unlistenFn = fn;
      })
      .catch((e) => console.error('Failed to listen for network changes:', e));

    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, []);

  const openAdd = useCallback(
    (url = '') => {
      refreshAdapters();
      setAddUrl(url);
      setAddOpen(true);
    },
    [refreshAdapters]
  );

  // ─── Derived lists ───
  const counts = useMemo(
    () => ({
      all: tasks.length,
      active: tasks.filter((t) => t.status === 'downloading').length,
      paused: tasks.filter((t) => t.status === 'paused').length,
      completed: tasks.filter((t) => t.status === 'completed').length,
      failed: tasks.filter((t) => t.status === 'error').length,
    }),
    [tasks]
  );

  const visible = useMemo(() => {
    const status = viewStatuses[view];
    const q = search.trim().toLowerCase();
    const filtered = tasks.filter(
      (t) =>
        (!status || t.status === status) &&
        (!q || t.filename.toLowerCase().includes(q) || t.url.toLowerCase().includes(q))
    );
    return sortTasks(filtered, sort);
  }, [tasks, view, search, sort]);

  // Selection is always a subset of what's visible.
  const selectedTasks = visible.filter((t) => selected.has(t.id));
  const single = selectedTasks.length === 1 ? selectedTasks[0] : null;
  const totalSpeed = tasks.reduce((s, t) => s + (t.status === 'downloading' ? t.speed_bytes_sec : 0), 0);

  const idsWhere = (list: DownloadTask[], pred: (t: DownloadTask) => boolean) => list.filter(pred).map((t) => t.id);
  const isResumable = (t: DownloadTask) => t.status === 'paused' || t.status === 'error';
  const isRunning = (t: DownloadTask) => t.status === 'downloading';

  // ─── Actions ───
  const actions: RowActions = {
    onOpen: (t) => api.openFile(t.id).catch((e) => onError(`Couldn't open ${t.filename}: ${errorText(e)}`)),
    onReveal: (t) => api.revealFile(t.id).catch((e) => onError(`Couldn't show ${t.filename}: ${errorText(e)}`)),
    onResume: (ids) => ids.length > 0 && resume(ids),
    onPause: (ids) => ids.length > 0 && pause(ids),
    onRemove: (ids) => ids.length > 0 && setRemoveIds(ids),
    onCopyLink: (list) =>
      copyText(list.map((t) => t.url).join('\n')).then(() =>
        notify('success', list.length === 1 ? 'Link copied' : `${list.length} links copied`)
      ),
    onAdd: () => openAdd(),
  };

  const copy = (text: string, what: string) => copyText(text).then(() => notify('success', `${what} copied`));

  // ─── Global keyboard shortcuts & paste-to-add ───
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (e.ctrlKey && key === 'n') {
        e.preventDefault();
        openAdd();
      } else if (e.ctrlKey && key === 'f') {
        e.preventDefault();
        setView((v) => (v === 'network' || v === 'settings' ? 'all' : v));
        setTimeout(() => searchRef.current?.focus());
      } else if (e.key === 'F5' || (e.ctrlKey && key === 'r')) {
        // Never reload the webview; refresh adapters instead.
        e.preventDefault();
        refreshAdapters();
      }
    };
    const onPaste = (e: ClipboardEvent) => {
      if (addOpen || isEditable(e.target)) return;
      const url = extractUrl(e.clipboardData?.getData('text') ?? '');
      if (url) {
        e.preventDefault();
        openAdd(url);
      } else {
        notify('info', 'The clipboard does not contain a link');
      }
    };
    // Suppress the browser's context menu outside text fields; our own menus handle right-click.
    const onContext = (e: MouseEvent) => {
      if (!isEditable(e.target)) e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('paste', onPaste);
    document.addEventListener('contextmenu', onContext);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('paste', onPaste);
      document.removeEventListener('contextmenu', onContext);
    };
  }, [addOpen, openAdd, refreshAdapters, notify]);

  // ─── System tray context menu events ───
  useEffect(() => {
    const unlistenAdd = listen('open-add-dialog', () => openAdd());
    const unlistenSettings = listen('open-settings', () => setView('settings'));
    return () => {
      unlistenAdd.then((fn) => fn());
      unlistenSettings.then((fn) => fn());
    };
  }, [openAdd]);

  const handleStart = async (args: Parameters<typeof start>[0]) => {
    const task = await start(args);
    if (view !== 'all' && view !== 'active') setView('all');
    setSelected(new Set([task.id]));
    notify('success', `Downloading ${task.filename}`);
  };

  const chunkColors = useMemo(
    () =>
      dark
        ? { done: '#60CDFF', active: '#C5A8F0', failed: '#FF99A4', pending: 'rgba(255,255,255,0.10)' }
        : { done: '#005FB8', active: '#8764B8', failed: '#C42B1C', pending: 'rgba(0,0,0,0.08)' },
    [dark]
  );

  const isListView = view !== 'network' && view !== 'settings';
  const empty = emptyStates[view as keyof typeof emptyStates];

  return (
    // Layout lives on an inner div: FluentProvider's className is also applied to portal
    // mount nodes (menus, dialogs, toasts), which must not become full-screen boxes.
    <FluentProvider theme={dark ? darkTheme : lightTheme} style={surfaceVars(dark, mica)}>
      <div className={styles.root}>
        <TitleBar />
        <div className={styles.body}>
          <NavPane
            view={view}
            onSelect={(v) => {
              setView(v);
              setSelected(new Set());
            }}
            counts={counts}
            compact={compactNav}
            onToggleCompact={() => setCompactNav((c) => !c)}
          />
          <main className={styles.layer}>
            {isListView && (
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
                  onAdd={() => openAdd()}
                  onResume={() => resume(idsWhere(selectedTasks, isResumable))}
                  onPause={() => pause(idsWhere(selectedTasks, isRunning))}
                  onRemove={() => setRemoveIds(selectedTasks.map((t) => t.id))}
                  onOpen={() => single && actions.onOpen(single)}
                  onReveal={() => single && actions.onReveal(single)}
                  onCopyLink={() => actions.onCopyLink(selectedTasks)}
                  onResumeAll={() => resume(idsWhere(tasks, isResumable))}
                  onPauseAll={() => pause(idsWhere(tasks, isRunning))}
                  onClearCompleted={() =>
                    remove(
                      idsWhere(tasks, (t) => t.status === 'completed'),
                      false
                    )
                  }
                  search={search}
                  onSearch={setSearch}
                  searchRef={searchRef}
                  detailsOpen={detailsOpen}
                  onToggleDetails={() => setDetailsOpen((d) => !d)}
                />
                <div className={styles.content}>
                  <div className={styles.list}>
                    {visible.length === 0 ? (
                      search.trim() ? (
                        <EmptyState icon={Search48Regular} title="No matches" body={`Nothing matches "${search.trim()}".`} />
                      ) : (
                        <EmptyState
                          {...empty}
                          onAdd={view === 'all' || view === 'active' ? () => openAdd() : undefined}
                        />
                      )
                    ) : (
                      <DownloadTable
                        tasks={visible}
                        sort={sort}
                        onSort={setSort}
                        selected={selected}
                        onSelectionChange={setSelected}
                        actions={actions}
                      />
                    )}
                  </div>
                  {detailsOpen && (
                    <DetailsPane
                      selection={selectedTasks}
                      adapters={adapters}
                      dark={dark}
                      chunkColors={chunkColors}
                      onResume={(ids) => resume(ids)}
                      onPause={(ids) => pause(ids)}
                      onOpen={actions.onOpen}
                      onReveal={actions.onReveal}
                      onCopy={copy}
                    />
                  )}
                </div>
                <StatusBar items={visible.length} selected={selectedTasks.length} active={counts.active} speed={totalSpeed} />
              </>
            )}
            {view === 'network' && (
              <NetworkPage adapters={adapters} tasks={tasks} history={history} dark={dark} onRefresh={refreshAdapters} />
            )}
            {view === 'settings' && <SettingsPage settings={settings} onChange={updateSettings} />}
          </main>
        </div>
      </div>

      <AddDownloadDialog
        open={addOpen}
        initialUrl={addUrl}
        adapters={adapters}
        defaultSaveDir={settings.default_save_dir}
        onRefreshAdapters={refreshAdapters}
        onStart={handleStart}
        onClose={() => {
          setAddOpen(false);
          document.getElementById(GRID_ID)?.focus();
        }}
      />
      <RemoveDialog
        tasks={tasks.filter((t) => removeIds.includes(t.id))}
        onConfirm={(ids, deleteFiles) => {
          remove(ids, deleteFiles);
          setSelected(new Set());
        }}
        onClose={() => setRemoveIds([])}
      />
      <Toaster toasterId={toasterId} position="bottom-end" offset={{ vertical: 40 }} />
    </FluentProvider>
  );
};

export default App;
