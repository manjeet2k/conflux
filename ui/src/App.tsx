import React, { useCallback, useEffect, useState } from 'react';
import { AppShell } from '@/components/AppShell';
import { useAppNotifications } from '@/hooks/useAppNotifications';
import { useAppShortcuts } from '@/hooks/useAppShortcuts';
import { useDesktopEvents } from '@/hooks/useDesktopEvents';
import { AddDownloadDialog } from '@/features/downloads/components/AddDownloadDialog';
import { DownloadsView } from '@/features/downloads/components/DownloadsView';
import { RemoveDialog } from '@/features/downloads/components/RemoveDialog';
import type { RowActions } from '@/features/downloads/components/DownloadTable';
import { downloadsApi } from '@/features/downloads/api/downloads.api';
import { useDownloads } from '@/features/downloads/hooks/useDownloads';
import { useDownloadDialogs } from '@/features/downloads/hooks/useDownloadDialogs';
import { useDownloadList } from '@/features/downloads/hooks/useDownloadList';
import { useSpeedHistory } from '@/features/downloads/hooks/useSpeedHistory';
import type { ViewId } from '@/features/downloads/types/downloads.types';
import { NetworkView } from '@/features/network/components/NetworkView';
import { useNetwork } from '@/features/network/hooks/useNetwork';
import { SettingsView } from '@/features/settings/components/SettingsView';
import { useSettings } from '@/features/settings/hooks/useSettings';
import { useWindowTheme } from '@/features/settings/hooks/useWindowTheme';
import { copyText } from '@/utils/files';
import type { SpeedUnit } from '@/utils/formatters';

export const App: React.FC = () => {
  const [view, setView] = useState<ViewId>('all');
  // Navigation callbacks stay stable as progress events update task snapshots.
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const selectView = useCallback((next: ViewId) => {
    setView(next);
    setSelected(new Set());
  }, []);
  const openSettings = useCallback(() => setView('settings'), []);
  const openAbout = useCallback(() => {
    openSettings();
    setSelected(new Set());
    setTimeout(() => document.getElementById('settings-about')?.scrollIntoView({ block: 'start' }), 50);
  }, [openSettings]);
  const { toasterId, notify, onError, notifyUpdate } = useAppNotifications(openSettings, openAbout);
  const { settings, loaded: settingsLoaded, update: updateSettings } = useSettings(onError);
  const { dark, backdrop } = useWindowTheme(settings.theme, settings.backdrop);
  const { tasks, start, pause, resume, remove } = useDownloads(onError);
  const { history, taskHistory } = useSpeedHistory(tasks);
  const { adapters, refreshAdapters, toggleAdapter } = useNetwork(onError);
  const list = useDownloadList(tasks, view, selected, setSelected);
  const dialogs = useDownloadDialogs({ tasks, refreshAdapters, notify, onError });
  const { openAdd } = dialogs;
  const openAddDialog = useCallback(() => openAdd(), [openAdd]);
  const [speedUnit, setSpeedUnit] = useState<SpeedUnit>(() =>
    localStorage.getItem('cfx.speedUnit') === 'bits' ? 'bits' : 'bytes'
  );
  useEffect(() => localStorage.setItem('cfx.speedUnit', speedUnit), [speedUnit]);

  const focusSearch = useCallback(() => {
    setView((current) => current === 'network' || current === 'settings' ? 'all' : current);
    setTimeout(() => list.viewProps.searchRef.current?.focus());
  }, [list.viewProps.searchRef]);
  const historyError = useDesktopEvents({
    onAdd: openAddDialog, onSettings: openSettings,
    onExternalDownload: dialogs.handleExternalDownload, onUpdate: notifyUpdate,
  });
  useAppShortcuts({
    isDialogOpen: dialogs.isDialogOpen, onAdd: dialogs.openAdd,
    onSearch: focusSearch, onRefresh: refreshAdapters, notify,
  });

  const actions: RowActions = {
    onOpen: (task) => downloadsApi.openFile(task.id)
      .catch((error) => onError(`Couldn't open ${task.filename}: ${String(error)}`)),
    onReveal: (task) => downloadsApi.revealFile(task.id)
      .catch((error) => onError(`Couldn't show ${task.filename}: ${String(error)}`)),
    onResume: (ids) => ids.length > 0 && resume(ids),
    onPause: (ids) => ids.length > 0 && pause(ids),
    onRemove: dialogs.openRemove,
    onCopyLink: (items) => copyText(items.map((task) => task.url).join('\n'))
      .then(() => notify('success', items.length === 1 ? 'Link copied' : `${items.length} links copied`)),
    onAdd: openAddDialog,
  };
  const handleStart = async (args: Parameters<typeof start>[0]) => {
    const task = await start(args);
    if (view !== 'all' && view !== 'active') setView('all');
    setSelected(new Set([task.id]));
    notify('success', `Downloading ${task.filename}`);
  };
  const copy = (text: string, label: string) => copyText(text).then(() => notify('success', `${label} copied`));
  const isListView = view !== 'network' && view !== 'settings';

  return (
    <AppShell
      dark={dark} backdrop={backdrop} toasterId={toasterId}
      navigation={{ view, onSelect: selectView, counts: list.counts }}
      status={{
        speedUnit, onSpeedUnitChange: setSpeedUnit, active: list.counts.active,
        queued: dialogs.queueCount, speed: list.totalSpeed, settings,
        loaded: settingsLoaded, onChange: updateSettings, dark,
      }}
      dialogs={
        <>
          <AddDownloadDialog
            open={dialogs.current !== null}
            requestId={dialogs.current?.id ?? 'closed'}
            initialHandoffId={dialogs.current?.payload.handoff_id}
            initialUrl={dialogs.current?.payload.url ?? ''}
            initialFilename={dialogs.current?.payload.filename}
            initialHeaders={dialogs.current?.payload.headers}
            defaultSaveDir={settings.default_save_dir}
            queueCount={dialogs.queueCount}
            onStart={handleStart}
            onClose={(accepted) => dialogs.advanceOrClose(dialogs.current?.id ?? 'closed', accepted)}
            onCancelAll={dialogs.cancelAll}
          />
          <RemoveDialog
            tasks={dialogs.removeTasks}
            onConfirm={(ids, deleteFiles) => { remove(ids, deleteFiles); setSelected(new Set()); }}
            onClose={dialogs.closeRemove}
          />
        </>
      }
    >
      {isListView && (
        <DownloadsView
          {...list.viewProps}
          tasks={tasks} counts={list.counts} speedUnit={speedUnit}
          historyError={historyError} adapters={adapters} taskHistory={taskHistory}
          dark={dark} actions={actions} view={view}
          onAdd={openAddDialog} onResume={resume} onPause={pause}
          onRemove={dialogs.openRemove}
          onClearCompleted={() => remove(tasks.filter((task) => task.status === 'completed').map((task) => task.id), false)}
          copy={copy}
        />
      )}
      {view === 'network' && (
        <NetworkView
          speedUnit={speedUnit} adapters={adapters} tasks={tasks} history={history}
          dark={dark} onRefresh={refreshAdapters} onToggleAdapter={toggleAdapter}
        />
      )}
      {view === 'settings' && (
        <SettingsView settings={settings} loaded={settingsLoaded} onChange={updateSettings} onError={onError} />
      )}
    </AppShell>
  );
};

export default App;
