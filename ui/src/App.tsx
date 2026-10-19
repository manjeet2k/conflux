// src/App.tsx
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Button,
  FluentProvider,
  Toast,
  ToastBody,
  ToastTitle,
  ToastTrigger,
  Toaster,
  makeStyles,
  tokens,
  useId,
  useToastController,
} from '@fluentui/react-components';
import type { ToastIntent, ToastProps } from '@fluentui/react-components';
import {
  ArrowDownload48Regular,
  CheckmarkCircle48Regular,
  Dismiss16Regular,
  ErrorCircle48Regular,
  PauseCircle48Regular,
} from '@fluentui/react-icons';
import { listen } from '@tauri-apps/api/event';

import { darkTheme, lightTheme, surfaceVars, windowsControlStyleHooks } from './theme';
import { TitleBar } from '@/components/ui/TitleBar';
import { NavPane } from '@/components/ui/NavPane';
import { StatusBar } from '@/components/ui/StatusBar';

import {
  AddDownloadDialog,
  DownloadsView,
  RemoveDialog,
  advanceDownload,
  downloadsApi,
  enqueueDownload,
  presentQueuedDownload,
  sortTasks,
  useDownloads,
  useSpeedHistory,
} from '@/features/downloads';
import type {
  DownloadQueue,
  DownloadRequest,
  ExternalDownloadPayload,
  RequestHeaders,
  RowActions,
  SortState,
  TaskStatus,
  ViewId,
} from '@/features/downloads';

import { NetworkView, useNetwork } from '@/features/network';
import { SettingsView, settingsApi, useSettings, useWindowTheme } from '@/features/settings';
import type { UpdateInfo } from '@/features/settings';

import { copyText, extractUrl } from '@/utils/files';
import type { SpeedUnit } from '@/utils/formatters';
import { usePref } from '@/hooks/usePref';

const useStyles = makeStyles({
  darkSurfaces: surfaceVars(true, 'none'),
  lightSurfaces: surfaceVars(false, 'none'),
  root: {
    height: '100vh',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: 'var(--cfx-window)',
    color: tokens.colorNeutralForeground1,
    overflow: 'hidden',
  },
  body: {
    flex: 1,
    display: 'flex',
    minHeight: 0,
  },
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
  viewContainer: {
    flex: 1,
    minWidth: 0,
    minHeight: 0,
    display: 'flex',
    flexDirection: 'column',
    animationName: 'win11PageEntrance',
    animationDuration: '200ms',
    animationTimingFunction: 'cubic-bezier(0.1, 0.9, 0.2, 1.0)',
    animationFillMode: 'both',
    '@media (prefers-reduced-motion: reduce)': {
      animation: 'none',
    },
  },
});

const viewStatuses: Partial<Record<ViewId, TaskStatus[]>> = {
  active: ['downloading'],
  paused: ['paused', 'stopping'],
  completed: ['completed'],
  failed: ['error'],
};

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

const isEditable = (el: EventTarget | null) =>
  el instanceof HTMLElement && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName));

const GRID_ID = 'cfx-download-grid';

interface InteractiveToastProps extends ToastProps {
  children: React.ReactNode;
}

const InteractiveToast: React.FC<InteractiveToastProps> = ({ children, onClick, style, ...props }) => {
  const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    onClick?.(e);
    if (e.defaultPrevented) return;
    const selection = window.getSelection();
    if (selection && selection.toString().trim().length > 0) {
      e.preventDefault();
    }
  };
  return (
    <ToastTrigger disableButtonEnhancement>
      <Toast {...props} onClick={handleClick} style={{ cursor: 'pointer', ...style }}>
        {children}
      </Toast>
    </ToastTrigger>
  );
};

export const App: React.FC = () => {
  const toasterId = useId('toaster');
  const { dispatchToast } = useToastController(toasterId);

  const notify = useCallback(
    (intent: ToastIntent, title: string) =>
      dispatchToast(
        <InteractiveToast>
          <ToastTitle
            action={
              <ToastTrigger>
                <Button
                  appearance="subtle"
                  aria-label="Dismiss notification"
                  icon={<Dismiss16Regular />}
                  size="small"
                />
              </ToastTrigger>
            }
          >
            {title}
          </ToastTitle>
        </InteractiveToast>,
        { intent, timeout: intent === 'error' || intent === 'warning' ? 8000 : 3000 }
      ),
    [dispatchToast]
  );

  const onError = useCallback((message: string) => notify('error', message), [notify]);

  // Settings & Theme
  const { settings, loaded: settingsLoaded, update: updateSettings } = useSettings(onError);
  const { dark, backdrop } = useWindowTheme(settings.theme, settings.backdrop);

  // Downloads
  const { tasks, start, pause, resume, remove } = useDownloads(onError);
  const { history, taskHistory } = useSpeedHistory(tasks);

  // Network
  const { adapters, refreshAdapters, toggleAdapter } = useNetwork(onError);

  // UI state
  const styles = useStyles();
  const [view, setView] = useState<ViewId>('all');
  const [compactNav, setCompactNav] = usePref('cfx.compactNav', window.innerWidth < 1100);
  const [detailsOpen, setDetailsOpen] = usePref('cfx.details', true);
  const [speedUnit, setSpeedUnit] = useState<SpeedUnit>(() =>
    localStorage.getItem('cfx.speedUnit') === 'bits' ? 'bits' : 'bytes'
  );
  const [search, setSearch] = useState('');
  const [sort, setSort] = useState<SortState>({ key: 'added', direction: 'descending' });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [downloadDialogs, setDownloadDialogs] = useState<DownloadQueue>({ current: null, queued: [] });
  const downloadDialogsRef = useRef(downloadDialogs);
  const dialogOpenRef = useRef(false);

  const updateDownloadDialogs = useCallback((update: (state: DownloadQueue) => DownloadQueue) => {
    const next = update(downloadDialogsRef.current);
    downloadDialogsRef.current = next;
    if (next.current) dialogOpenRef.current = true;
    setDownloadDialogs(next);
  }, []);

  const addOpen = downloadDialogs.current !== null;
  const addUrl = downloadDialogs.current?.payload.url ?? '';
  const addFilename = downloadDialogs.current?.payload.filename ?? null;
  const addHeaders = downloadDialogs.current?.payload.headers ?? null;
  const downloadQueue = downloadDialogs.queued;
  const [removeIds, setRemoveIds] = useState<string[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    localStorage.setItem('cfx.speedUnit', speedUnit);
  }, [speedUnit]);

  // One-time startup notices
  useEffect(() => {
    settingsApi
      .takeStartupNotices()
      .then((notices) => notices.forEach((n) => notify('warning', n)))
      .catch((e) => console.error('Failed to read startup notices:', e));
  }, [notify]);

  // One-time announcement for browser integration in 0.2.0
  useEffect(() => {
    const key = 'cfx.announced.browser-0.2.0';
    if (!localStorage.getItem(key)) {
      localStorage.setItem(key, '1');
      const timer = window.setTimeout(() => {
        dispatchToast(
          <InteractiveToast>
            <ToastTitle
              action={
                <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                  <ToastTrigger>
                    <Button appearance="transparent" size="small" onClick={() => setView('settings')}>
                      View
                    </Button>
                  </ToastTrigger>
                  <ToastTrigger>
                    <Button
                      appearance="subtle"
                      aria-label="Dismiss notification"
                      icon={<Dismiss16Regular />}
                      size="small"
                    />
                  </ToastTrigger>
                </div>
              }
            >
              Browser integration available
            </ToastTitle>
            <ToastBody subtitle="Open Settings → Browser Integration to get the extension.">
              Capture downloads directly from Chrome, Edge, and Firefox.
            </ToastBody>
          </InteractiveToast>,
          { intent: 'info', timeout: 12000 }
        );
      }, 1500);
      return () => window.clearTimeout(timer);
    }
  }, [dispatchToast]);

  const removeTasks = tasks.filter((t) => removeIds.includes(t.id));
  const removeOpen = removeTasks.length > 0;

  useEffect(() => {
    dialogOpenRef.current = downloadDialogsRef.current.current !== null || removeOpen;
    if (!removeOpen && !downloadDialogsRef.current.current) {
      updateDownloadDialogs(presentQueuedDownload);
    }
  }, [addOpen, removeOpen, updateDownloadDialogs]);

  const dismissRequest = useCallback(
    (request: DownloadRequest) => {
      if (request.payload.handoff_id) {
        downloadsApi
          .dismissExternalDownload(request.payload.handoff_id)
          .catch((error) => onError(String(error)));
      }
    },
    [onError]
  );

  const openAdd = useCallback(
    (url = '', filename: string | null = null, headers: RequestHeaders | null = null) => {
      if (dialogOpenRef.current) return;
      refreshAdapters();
      const request: DownloadRequest = { id: crypto.randomUUID(), payload: { url, filename, headers } };
      dialogOpenRef.current = true;
      updateDownloadDialogs((state) => enqueueDownload(state, request, false));
    },
    [refreshAdapters, updateDownloadDialogs]
  );

  const handleExternalDownload = useCallback(
    (payload: ExternalDownloadPayload) => {
      refreshAdapters();
      const request: DownloadRequest = { id: crypto.randomUUID(), payload };
      const blocked = dialogOpenRef.current;
      if (blocked) notify('info', 'Queued link: ' + (payload.filename || payload.url));
      updateDownloadDialogs((state) => enqueueDownload(state, request, blocked));
    },
    [refreshAdapters, notify, updateDownloadDialogs]
  );

  const handleAdvanceOrClose = useCallback(
    (requestId: string, accepted = false) => {
      const current = downloadDialogsRef.current.current;
      if (current?.id !== requestId) return;
      if (!accepted) dismissRequest(current);
      updateDownloadDialogs((state) => advanceDownload(state, requestId));
      dialogOpenRef.current = downloadDialogsRef.current.current !== null;
      if (!dialogOpenRef.current) document.getElementById(GRID_ID)?.focus();
    },
    [dismissRequest, updateDownloadDialogs]
  );

  const handleCancelAll = useCallback(() => {
    const state = downloadDialogsRef.current;
    for (const request of [...(state.current ? [state.current] : []), ...state.queued]) {
      dismissRequest(request);
    }
    updateDownloadDialogs(() => ({ current: null, queued: [] }));
    dialogOpenRef.current = false;
    document.getElementById(GRID_ID)?.focus();
  }, [dismissRequest, updateDownloadDialogs]);

  useEffect(() => {
    downloadsApi
      .takePendingDownload()
      .then((payload) => {
        if (payload) handleExternalDownload(payload);
      })
      .catch((e) => console.error('Failed to take pending download:', e));

    const cleanupPromise = downloadsApi.onExternalDownload((payload) => {
      handleExternalDownload(payload);
    });

    return () => {
      cleanupPromise.then((unlisten) => unlisten());
    };
  }, [handleExternalDownload]);

  const openAbout = useCallback(() => {
    setView('settings');
    setSelected(new Set());
    setTimeout(() => document.getElementById('settings-about')?.scrollIntoView({ block: 'start' }), 50);
  }, []);

  useEffect(() => {
    const cleanupPromise = settingsApi.onUpdateAvailable((payload: UpdateInfo) => {
      dispatchToast(
        <InteractiveToast>
          <ToastTitle
            action={
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <ToastTrigger>
                  <Button appearance="transparent" size="small" onClick={openAbout}>
                    View
                  </Button>
                </ToastTrigger>
                <ToastTrigger>
                  <Button
                    appearance="subtle"
                    aria-label="Dismiss notification"
                    icon={<Dismiss16Regular />}
                    size="small"
                  />
                </ToastTrigger>
              </div>
            }
          >
            Update available
          </ToastTitle>
          <ToastBody subtitle="Open Settings → About & Updates to install it.">
            Conflux {payload.version} is ready to download.
          </ToastBody>
        </InteractiveToast>,
        { intent: 'info', timeout: 15000 }
      );
    });

    return () => {
      cleanupPromise.then((unlisten) => unlisten());
    };
  }, [dispatchToast, openAbout]);

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

  const actions: RowActions = {
    onOpen: (t) =>
      downloadsApi.openFile(t.id).catch((err) => onError(`Couldn't open ${t.filename}: ${String(err)}`)),
    onReveal: (t) =>
      downloadsApi.revealFile(t.id).catch((err) => onError(`Couldn't show ${t.filename}: ${String(err)}`)),
    onResume: (ids) => ids.length > 0 && resume(ids),
    onPause: (ids) => ids.length > 0 && pause(ids),
    onRemove: (ids) => {
      if (ids.length > 0) {
        dialogOpenRef.current = true;
        setRemoveIds(ids);
      }
    },
    onCopyLink: (list) =>
      copyText(list.map((t) => t.url).join('\n')).then(() =>
        notify('success', list.length === 1 ? 'Link copied' : `${list.length} links copied`)
      ),
    onAdd: () => openAdd(),
  };

  const copy = (text: string, what: string) => copyText(text).then(() => notify('success', `${what} copied`));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      const dialogOpen = addOpen || removeIds.length > 0;
      if (dialogOpen && e.ctrlKey && (key === 'n' || key === 'f')) {
        e.preventDefault();
      } else if (e.ctrlKey && key === 'n') {
        e.preventDefault();
        openAdd();
      } else if (e.ctrlKey && key === 'f') {
        e.preventDefault();
        setView((v) => (v === 'network' || v === 'settings' ? 'all' : v));
        setTimeout(() => searchRef.current?.focus());
      } else if (e.key === 'F5' || (e.ctrlKey && key === 'r')) {
        e.preventDefault();
        refreshAdapters();
      }
    };
    const onPaste = (e: ClipboardEvent) => {
      if (dialogOpenRef.current || isEditable(e.target)) return;
      const url = extractUrl(e.clipboardData?.getData('text') ?? '');
      if (url) {
        e.preventDefault();
        openAdd(url);
      } else {
        notify('info', 'The clipboard does not contain a link');
      }
    };
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
  }, [addOpen, removeIds, openAdd, refreshAdapters, notify]);

  const [historyError, setHistoryError] = useState<string | null>(null);
  useEffect(() => {
    const cleanupPromise = downloadsApi.onHistoryStatus((status) => {
      setHistoryError(status);
    });
    return () => {
      cleanupPromise.then((unlisten) => unlisten());
    };
  }, []);

  // System tray context menu events
  useEffect(() => {
    const unlistenAdd = listen('open-add-dialog', () => openAdd());
    const unlistenSettings = listen('open-settings', () => setView('settings'));
    return () => {
      unlistenAdd.then((fn) => fn()).catch(() => undefined);
      unlistenSettings.then((fn) => fn()).catch(() => undefined);
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
    <FluentProvider
      theme={dark ? darkTheme : lightTheme}
      customStyleHooks_unstable={windowsControlStyleHooks}
      className={dark ? styles.darkSurfaces : styles.lightSurfaces}
      style={{ ...surfaceVars(dark, backdrop), backgroundColor: 'transparent' }}
    >
      <div className={styles.root}>
        <TitleBar />
        <div className={styles.body}>
          <NavPane
            view={view}
            onSelect={(v: ViewId) => {
              setView(v);
              setSelected(new Set());
            }}
            counts={counts}
            compact={compactNav}
            onToggleCompact={() => setCompactNav(!compactNav)}
          />
          <main className={styles.layer}>
            <div key={isListView ? 'downloads' : view} className={styles.viewContainer}>
              {isListView && (
                <DownloadsView
                  tasks={tasks}
                  visible={visible}
                  counts={counts}
                  selected={selected}
                  onSelectionChange={setSelected}
                  sort={sort}
                  onSort={setSort}
                  speedUnit={speedUnit}
                  detailsOpen={detailsOpen}
                  onToggleDetails={() => setDetailsOpen(!detailsOpen)}
                  search={search}
                  onSearch={setSearch}
                  searchRef={searchRef}
                  historyError={historyError}
                  adapters={adapters}
                  taskHistory={taskHistory}
                  dark={dark}
                  chunkColors={chunkColors}
                  actions={actions}
                  view={view}
                  emptyState={empty}
                  onAdd={() => openAdd()}
                  onResume={(ids) => resume(ids)}
                  onPause={(ids) => pause(ids)}
                  onRemove={(ids) => setRemoveIds(ids)}
                  onClearCompleted={() =>
                    remove(
                      tasks.filter((t) => t.status === 'completed').map((t) => t.id),
                      false
                    )
                  }
                  copy={copy}
                />
              )}
              {view === 'network' && (
                <NetworkView
                  speedUnit={speedUnit}
                  adapters={adapters}
                  tasks={tasks}
                  history={history}
                  dark={dark}
                  onRefresh={refreshAdapters}
                  onToggleAdapter={toggleAdapter}
                />
              )}
              {view === 'settings' && (
                <SettingsView
                  settings={settings}
                  loaded={settingsLoaded}
                  onChange={updateSettings}
                  onError={onError}
                />
              )}
            </div>
            <StatusBar
              speedUnit={speedUnit}
              onSpeedUnitChange={setSpeedUnit}
              active={counts.active}
              queued={downloadQueue.length}
              speed={totalSpeed}
              settings={settings}
              loaded={settingsLoaded}
              onChange={updateSettings}
              dark={dark}
            />
          </main>
        </div>
      </div>

      <AddDownloadDialog
        open={addOpen}
        requestId={downloadDialogs.current?.id ?? 'closed'}
        initialHandoffId={downloadDialogs.current?.payload.handoff_id}
        initialUrl={addUrl}
        initialFilename={addFilename}
        initialHeaders={addHeaders}
        defaultSaveDir={settings.default_save_dir}
        queueCount={downloadQueue.length}
        onStart={handleStart}
        onClose={(accepted) => handleAdvanceOrClose(downloadDialogs.current?.id ?? 'closed', accepted)}
        onCancelAll={handleCancelAll}
      />
      <RemoveDialog
        tasks={removeTasks}
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
