import assert from 'node:assert/strict';
import test from 'node:test';
import { sourceLoader, hookHarness, deferred, flushPromises, jsx, fluent, react, elements } from './helpers/loadSource.mjs';

function dialogHarness() {
  const harness = hookHarness();
  const dismissed = [], notices = [];
  let id = 0, focused = 0;
  const load = sourceLoader({
    react: harness.hooks,
    '../api/downloads.api': { downloadsApi: { dismissExternalDownload: async (handoff) => dismissed.push(handoff) } },
  }, {
    crypto: { randomUUID: () => String(++id) },
    document: { getElementById: (id) => { assert.equal(id, 'cfx-download-grid'); return { focus: () => focused++ }; } },
  });
  const { useDownloadDialogs } = load('features/downloads/hooks/useDownloadDialogs.ts');
  const options = {
    tasks: [{ id: 'task-1' }], refreshAdapters() {},
    notify: (intent, message) => notices.push({ intent, message }),
    onError: (message) => assert.fail(message),
  };
  const render = () => {
    harness.render(() => useDownloadDialogs(options));
    return harness.render(() => useDownloadDialogs(options));
  };
  return { harness, render, dismissed, notices, focused: () => focused };
}

test('dialog hook queues consecutive handoffs and ignores completion of an older request', async () => {
  const fixture = dialogHarness();
  let dialogs = fixture.render();
  dialogs.handleExternalDownload({ url: 'https://example.com/a', handoff_id: 'a' });
  dialogs.handleExternalDownload({ url: 'https://example.com/b', handoff_id: 'b' });
  dialogs = fixture.render();
  const firstId = dialogs.current.id;
  assert.equal(dialogs.current.payload.handoff_id, 'a');
  assert.equal(dialogs.queueCount, 1);
  assert.equal(fixture.notices.length, 1);
  dialogs.advanceOrClose(firstId, true);
  dialogs = fixture.render();
  const secondId = dialogs.current.id;
  assert.equal(dialogs.current.payload.handoff_id, 'b');
  dialogs.advanceOrClose(firstId, true);
  assert.equal(fixture.render().current.id, secondId);
  assert.equal(fixture.dismissed.length, 0, 'accepted requests are not dismissed');
  dialogs.advanceOrClose(secondId);
  dialogs = fixture.render();
  assert.equal(dialogs.current, null);
  assert.equal(dialogs.isDialogOpen(), false);
  assert.deepEqual(fixture.dismissed, ['b']);
  assert.equal(fixture.focused(), 1);
  fixture.harness.dispose();
  await flushPromises();
});

test('remove dialog blocks manual adds immediately and presents browser requests after closing', () => {
  const fixture = dialogHarness();
  let dialogs = fixture.render();
  dialogs.openRemove(['task-1']);
  assert.equal(dialogs.isDialogOpen(), true);
  dialogs.openAdd('https://example.com/manual');
  dialogs.handleExternalDownload({ url: 'https://example.com/browser', handoff_id: 'browser' });
  dialogs = fixture.render();
  assert.equal(dialogs.current, null);
  assert.equal(dialogs.removeTasks.length, 1);
  assert.equal(dialogs.queueCount, 1);
  dialogs.closeRemove();
  dialogs = fixture.render();
  assert.equal(dialogs.current.payload.handoff_id, 'browser');
  assert.equal(dialogs.removeTasks.length, 0);
  assert.equal(dialogs.queueCount, 0);
  fixture.harness.dispose();
});

test('cancel all dismisses every queued handoff exactly once', async () => {
  const fixture = dialogHarness();
  let dialogs = fixture.render();
  for (const handoff of ['a', 'b', 'c']) dialogs.handleExternalDownload({ url: 'https://example.com/' + handoff, handoff_id: handoff });
  dialogs = fixture.render();
  dialogs.cancelAll();
  dialogs = fixture.render();
  assert.deepEqual(fixture.dismissed, ['a', 'b', 'c']);
  assert.equal(dialogs.current, null);
  assert.equal(dialogs.queueCount, 0);
  assert.equal(dialogs.isDialogOpen(), false);
  fixture.harness.dispose();
  await flushPromises();
});

test('desktop listeners forward payloads, ignore late pending reads and clean up delayed registrations', async () => {
  const harness = hookHarness();
  const pendingDownload = deferred();
  const records = [], received = [], updates = [], adds = [], settings = [];
  let reads = 0;
  const load = sourceLoader({
    react: harness.hooks,
    '@tauri-apps/api/core': { invoke: (command) => {
      assert.equal(command, 'take_pending_download');
      reads++;
      return pendingDownload.promise;
    } },
    '@tauri-apps/api/event': { listen: (name, callback) => {
      const ready = deferred();
      records.push({ name, callback, ready, removed: false });
      return ready.promise;
    } },
  });
  const { useDesktopEvents } = load('hooks/useDesktopEvents.ts');
  const options = {
    onAdd: (...args) => adds.push(args), onSettings: (...args) => settings.push(args),
    onExternalDownload: (payload) => received.push(payload), onUpdate: (info) => updates.push(info),
  };
  harness.render(() => useDesktopEvents(options));
  harness.render(() => useDesktopEvents(options));
  assert.equal(reads, 1);
  assert.equal(records.length, 5);
  const emit = (name, payload) => records.find((record) => record.name === name).callback({ payload });
  const payload = { url: 'https://example.com/a' };
  emit('external-download', payload);
  assert.equal(received[0], payload);
  emit('open-add-dialog', null);
  emit('open-settings', null);
  assert.deepEqual(adds, [[]], 'tray event objects must never become download URLs');
  assert.deepEqual(settings, [[]]);
  emit('history-save-status', 'Disk full');
  assert.equal(harness.render(() => useDesktopEvents(options)), 'Disk full');
  emit('history-save-status', null);
  assert.equal(harness.render(() => useDesktopEvents(options)), null);
  const update = { version: '0.4.9' };
  emit('update-available', update);
  assert.equal(updates[0], update);
  harness.dispose();
  pendingDownload.resolve({ url: 'https://example.com/late' });
  for (const record of records) record.ready.resolve(() => { record.removed = true; });
  await flushPromises();
  assert.equal(received.length, 1);
  assert.ok(records.every((record) => record.removed));
});

test('shortcuts respect modal ownership, editable fields and listener cleanup', () => {
  const harness = hookHarness();
  const windows = new Map(), documentEvents = new Map();
  const surface = (events) => ({
    addEventListener: (name, callback) => events.set(name, callback),
    removeEventListener: (name, callback) => { assert.equal(events.get(name), callback); events.delete(name); },
  });
  class Element { constructor(tagName) { this.tagName = tagName; this.isContentEditable = false; } }
  const load = sourceLoader({ react: harness.hooks }, {
    window: surface(windows), document: surface(documentEvents), HTMLElement: Element,
    navigator: {},
  });
  const { useAppShortcuts } = load('hooks/useAppShortcuts.ts');
  let modal = false, searches = 0, refreshes = 0;
  const added = [], notices = [];
  harness.render(() => useAppShortcuts({
    isDialogOpen: () => modal, onAdd: (url) => added.push(url),
    onSearch: () => searches++, onRefresh: () => refreshes++,
    notify: (intent, message) => notices.push({ intent, message }),
  }));
  const key = (name) => {
    const event = { key: name, ctrlKey: true, prevented: false, preventDefault() { this.prevented = true; } };
    windows.get('keydown')(event);
    assert.equal(event.prevented, true);
  };
  key('n');
  key('f');
  assert.equal(added.length, 1);
  assert.equal(searches, 1);
  modal = true;
  key('n'); key('f'); key('r');
  assert.equal(added.length, 1);
  assert.equal(searches, 1);
  assert.equal(refreshes, 1);
  const paste = (tag, text) => documentEvents.get('paste')({
    target: new Element(tag), clipboardData: { getData: () => text }, preventDefault() {},
  });
  paste('DIV', 'https://example.com/modal');
  modal = false;
  paste('INPUT', 'https://example.com/input');
  paste('DIV', 'https://example.com/download');
  assert.deepEqual(added, [undefined, 'https://example.com/download']);
  paste('DIV', 'plain text');
  assert.equal(notices.length, 1);
  harness.dispose();
  assert.equal(windows.size, 0);
  assert.equal(documentEvents.size, 0);
});

test('download filters count stopping tasks as paused and retain search and sorting across views', () => {
  const harness = hookHarness();
  const load = sourceLoader({ react: harness.hooks }, { localStorage: { getItem: () => null, setItem() {} } });
  const { useDownloadList } = load('features/downloads/hooks/useDownloadList.ts');
  const tasks = [
    { id: 'a', filename: 'B.iso', url: 'https://example.com/b', status: 'downloading', speed_bytes_sec: 10 },
    { id: 'b', filename: 'A.iso', url: 'https://example.com/a', status: 'stopping', speed_bytes_sec: 20 },
    { id: 'c', filename: 'C.zip', url: 'https://example.com/c', status: 'completed', speed_bytes_sec: 30 },
  ];
  const selected = new Set(['a']);
  const selectionChanged = () => {};
  const render = (view) => harness.render(() => useDownloadList(tasks, view, selected, selectionChanged));
  let list = render('all');
  assert.equal(list.counts.paused, 1);
  assert.equal(list.totalSpeed, 10);
  list.viewProps.onSearch(' .ISO ');
  list.viewProps.onSort({ key: 'name', direction: 'ascending' });
  list = render('all');
  assert.equal(list.viewProps.visible.length, 2);
  assert.equal(list.viewProps.visible[0].id, 'b');
  assert.equal(list.viewProps.selected, selected);
  list = render('paused');
  assert.equal(list.viewProps.visible.length, 1);
  assert.equal(list.viewProps.visible[0].status, 'stopping');
  assert.equal(list.viewProps.search, ' .ISO ');
  harness.dispose();
});

test('shell preserves the download view key across categories and keeps dialogs inside the provider', () => {
  const load = sourceLoader({
    react, 'react/jsx-runtime': { jsx, jsxs: jsx }, '@fluentui/react-components': fluent,
    '@/theme': { surfaceVars: () => ({}), windowsControlStyleHooks: {} },
    '@/components/TitleBar': { TitleBar: 'TitleBar' },
    '@/components/NavPane': { NavPane: 'NavPane' },
    '@/components/StatusBar': { StatusBar: 'StatusBar' },
    '@/hooks/usePref': { usePref: () => [false, () => {}] },
  }, { window: { innerWidth: 1200 } });
  const { AppShell } = load('components/AppShell.tsx');
  const content = { type: 'Downloads' }, dialogs = { type: 'Dialogs' };
  for (const view of ['all', 'active', 'paused', 'completed', 'failed']) {
    const tree = AppShell({ navigation: { view }, status: {}, children: content, dialogs });
    assert.equal(tree.type, 'FluentProvider');
    const container = elements(tree).find((node) => node.key === 'downloads');
    assert.equal(container.props.children, content);
    assert.ok(elements(tree).includes(dialogs));
  }
});


test('App composition shares selection with the list and selects new downloads after starting from another view', async () => {
  const harness = hookHarness();
  let tasks = [{ id: 'first', status: 'paused', filename: 'first.zip', url: 'https://example.com/first' }];
  let shortcutOptions;
  const manualAdds = [];
  const timers = [];
  const noop = () => {};
  const load = sourceLoader({
    react: harness.hooks, 'react/jsx-runtime': { jsx, jsxs: jsx },
    '@/components/AppShell': { AppShell: 'AppShell' },
    '@/hooks/useAppNotifications': { useAppNotifications: () => ({ notify: noop, onError: noop, notifyUpdate: noop }) },
    '@/hooks/useDesktopEvents': { useDesktopEvents: () => null },
    '@/hooks/useAppShortcuts': { useAppShortcuts: (options) => { shortcutOptions = options; } },
    '@/features/downloads/components/AddDownloadDialog': { AddDownloadDialog: 'AddDownloadDialog' },
    '@/features/downloads/components/RemoveDialog': { RemoveDialog: 'RemoveDialog' },
    '@/features/downloads/components/DownloadsView': { DownloadsView: 'DownloadsView' },
    '@/features/downloads/api/downloads.api': { downloadsApi: {} },
    '@/features/downloads/hooks/useDownloads': { useDownloads: () => ({
      tasks, pause: noop, resume: noop, remove: noop,
      start: async () => {
        const task = { id: 'new-task', filename: 'new.zip', status: 'downloading', url: 'https://example.com/new' };
        tasks = [...tasks, task];
        return task;
      },
    }) },
    '@/features/downloads/hooks/useDownloadDialogs': { useDownloadDialogs: () => ({ current: null, queueCount: 0, removeTasks: [], openAdd: (...args) => manualAdds.push(args) }) },
    '@/features/downloads/hooks/useSpeedHistory': { useSpeedHistory: () => ({}) },
    '@/features/network/components/NetworkView': { NetworkView: 'NetworkView' },
    '@/features/network/hooks/useNetwork': { useNetwork: () => ({}) },
    '@/features/settings/components/SettingsView': { SettingsView: 'SettingsView' },
    '@/features/settings/hooks/useSettings': { useSettings: () => ({ settings: {} }) },
    '@/features/settings/hooks/useWindowTheme': { useWindowTheme: () => ({}) },
  }, { localStorage: { getItem: () => null, setItem() {} }, setTimeout: (callback) => timers.push(callback) });
  const { App } = load('App.tsx');
  const render = () => harness.render(() => App());
  let tree = render();
  let list = elements(tree).find((node) => node.type === 'DownloadsView');
  list.props.onAdd({ type: 'click' });
  list.props.actions.onAdd({ type: 'click' });
  assert.deepEqual(manualAdds, [[], []], 'click events never become initial download URLs');
  list.props.onSelectionChange(new Set(['first']));
  tree = render();
  list = elements(tree).find((node) => node.type === 'DownloadsView');
  assert.equal(list.props.selected.has('first'), true);
  tree.props.navigation.onSelect('network');
  tree = render();
  assert.equal(tree.props.navigation.view, 'network');
  shortcutOptions.onSearch();
  tree = render();
  list = elements(tree).find((node) => node.type === 'DownloadsView');
  assert.equal(tree.props.navigation.view, 'all');
  assert.equal(list.props.selected.size, 0);
  let focused = false;
  list.props.searchRef.current = { focus: () => { focused = true; } };
  timers.splice(0).forEach((callback) => callback());
  assert.equal(focused, true);
  tree.props.navigation.onSelect('settings');
  tree = render();
  const dialog = elements(tree.props.dialogs).find((node) => node.type === 'AddDownloadDialog');
  await dialog.props.onStart({ url: 'https://example.com/new', saveDir: '/downloads', filename: null });
  tree = render();
  list = elements(tree).find((node) => node.type === 'DownloadsView');
  assert.equal(tree.props.navigation.view, 'all');
  assert.equal(list.props.selected.has('new-task'), true);
  harness.dispose();
});

test('notification hook preserves startup warnings, action destinations and urgency timeouts', async () => {
  const harness = hookHarness();
  const notifications = [], timers = new Map(), preferences = new Map();
  let settings = 0, about = 0;
  const dispatchToast = (toast, options) => notifications.push({ toast, options });
  const load = sourceLoader({
    react: harness.hooks, 'react/jsx-runtime': { jsx, jsxs: jsx },
    '@fluentui/react-components': { useId: () => 'toaster', useToastController: () => ({ dispatchToast }) },
    '@/features/settings/api/settings.api': { settingsApi: { takeStartupNotices: async () => ['Recovered history'] } },
    '../components/AppToast': { AppToast: 'AppToast' },
  }, {
    localStorage: { getItem: (key) => preferences.get(key), setItem: (key, value) => preferences.set(key, value) },
    window: { setTimeout: (callback) => { timers.set(1, callback); return 1; }, clearTimeout: (id) => timers.delete(id) },
  });
  const { useAppNotifications } = load('hooks/useAppNotifications.tsx');
  const hook = harness.render(() => useAppNotifications(() => settings++, () => about++));
  await flushPromises();
  assert.equal(notifications[0].toast.props.title, 'Recovered history');
  assert.equal(notifications[0].options.timeout, 8000);
  timers.get(1)();
  notifications[1].toast.props.onView();
  assert.equal(settings, 1);
  hook.notifyUpdate({ version: '0.4.9' });
  notifications[2].toast.props.onView();
  assert.equal(about, 1);
  assert.equal(notifications[2].options.timeout, 15000);
  hook.notify('info', 'Copied');
  assert.equal(notifications[3].options.timeout, 3000);
  harness.dispose();
  assert.equal(timers.size, 0);
});

test('selecting toast text prevents dismissal and keeps the View action available', () => {
  let selection = 'Selected warning text';
  const load = sourceLoader({
    react, 'react/jsx-runtime': { jsx, jsxs: jsx },
    '@fluentui/react-components': fluent, '@fluentui/react-icons': {},
  }, { window: { getSelection: () => ({ toString: () => selection }) } });
  const { AppToast } = load('components/AppToast.tsx');
  const onView = () => {};
  const tree = AppToast({ title: 'Warning', onView });
  const toast = elements(tree).find((node) => node.type === 'Toast');
  let prevented = false;
  const event = { preventDefault: () => { prevented = true; } };
  toast.props.onClick(event);
  assert.equal(prevented, true);
  selection = '';
  prevented = false;
  toast.props.onClick(event);
  assert.equal(prevented, false);
  const title = elements(tree).find((node) => node.type === 'ToastTitle');
  assert.equal(elements(title.props.action).find((node) => node.type === 'Button').props.onClick, onView);
});


test('failed desktop registrations are handled before unmount', async () => {
  const harness = hookHarness();
  const errors = [];
  const load = sourceLoader({
    react: harness.hooks,
    '@tauri-apps/api/core': { invoke: async () => null },
    '@tauri-apps/api/event': { listen: async () => { throw Error('Listener unavailable'); } },
  }, { console: { error: (...args) => errors.push(args) } });
  const { useDesktopEvents } = load('hooks/useDesktopEvents.ts');
  harness.render(() => useDesktopEvents({ onAdd() {}, onSettings() {}, onExternalDownload() {}, onUpdate() {} }));
  await flushPromises();
  assert.equal(errors.length, 5);
  harness.dispose();
  await flushPromises();
});
