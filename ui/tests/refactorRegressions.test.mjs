import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { sourceLoader, jsx, react, fluent, elements } from './helpers/loadSource.mjs';

const uiMocks = {
  react,
  'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
  '@fluentui/react-components': fluent,
  '@fluentui/react-icons': {},
};

test('browser handoffs and history persistence warnings use the Rust event contract', async () => {
  const listeners = new Map();
  const load = sourceLoader({
    '@tauri-apps/api/core': {},
    '@tauri-apps/api/event': { listen: async (name, callback) => {
      listeners.set(name, callback);
      return () => listeners.delete(name);
    } },
  });
  const { downloadsApi } = load('features/downloads/api/downloads.api.ts');
  const lib = fs.readFileSync(new URL('../../crates/conflux-desktop/src/lib.rs', import.meta.url), 'utf8');
  const commands = fs.readFileSync(new URL('../../crates/conflux-desktop/src/commands.rs', import.meta.url), 'utf8');
  const externalEvent = lib.match(/app\.emit\("(external-download)"/)[1];
  const historyEvent = commands.match(/HISTORY_STATUS_EVENT: &str = "([^"]+)"/)[1];
  const handoff = { url: 'https://example.com/file.zip', handoff_id: 'handoff-1' };
  let received, warning;
  const offExternal = await downloadsApi.onExternalDownload((payload) => { received = payload; });
  const offHistory = await downloadsApi.onHistoryStatus((payload) => { warning = payload; });
  listeners.get(externalEvent)?.({ payload: handoff });
  listeners.get(historyEvent)?.({ payload: 'Disk full' });
  assert.equal(received, handoff);
  assert.equal(warning, 'Disk full');
  listeners.get(historyEvent)({ payload: null });
  assert.equal(warning, null);
  offExternal();
  offHistory();
  assert.equal(listeners.size, 0);
});

test('selecting downloads opens details without toggling an already open pane', () => {
  const load = sourceLoader({ ...uiMocks,
    './CommandBar': { CommandBar: 'CommandBar' },
    './DownloadTable': { DownloadTable: 'DownloadTable' },
    './DetailsPane': { DetailsPane: 'DetailsPane' },
    '@/components/ui/EmptyState': { EmptyState: 'EmptyState' },
  });
  const { DownloadsView } = load('features/downloads/components/DownloadsView.tsx');
  const task = { id: 'task-1', status: 'paused' };
  for (const initiallyOpen of [false, true]) {
    let open = initiallyOpen;
    const tree = DownloadsView({
      tasks: [task], visible: [task], selected: new Set([task.id]), counts: {},
      actions: {}, detailsOpen: open,
      onDetailsOpenChange: (value) => { open = value; },
      onToggleDetails: () => { open = !open; },
      search: '', emptyState: {},
    });
    const table = elements(tree).find((node) => node.type === 'DownloadTable');
    table.props.onRowClick();
    assert.equal(open, true);
    table.props.onRowClick();
    assert.equal(open, true);
    const pane = elements(tree).find((node) => node.type === 'DetailsPane');
    if (pane) {
      pane.props.onClose();
      assert.equal(open, false);
    }
  }
});

test('settings expose the startup update preference and name each editable control', () => {
  const load = sourceLoader({ ...uiMocks,
    '@/components/ui/Page': { Page: 'Page', SettingsCard: 'SettingsCard', SectionHeader: 'SectionHeader' },
    '@/components/ui/WindowsDropdown': { WindowsDropdown: 'WindowsDropdown' },
    '@/components/ui/TurtleIcon': { TurtleIcon: 'TurtleIcon' },
    '@tauri-apps/plugin-dialog': {}, '@tauri-apps/api/path': {}, '@tauri-apps/api/app': {},
    '@/api': {},
    '../api/settings.api': {}, '@/features/network/api/network.api': {}, '@/utils/errors': {},
    '@/assets/browsers/chrome.svg': '', '@/assets/browsers/firefox.svg': '',
  });
  const { SettingsView } = load('features/settings/components/SettingsView.tsx');
  for (const enabled of [true, false]) {
    const patches = [];
    const tree = SettingsView({
      loaded: true,
      settings: { slow_mode_limit_kib: 1024, default_save_dir: null, check_updates_on_start: enabled },
      onChange: (patch) => patches.push(patch),
    });
    const nodes = elements(tree);
    const updateCard = nodes.find((node) => node.type === 'SettingsCard' && node.props.title === 'Check for updates on start');
    assert.ok(updateCard, 'startup update setting remains available');
    const toggle = elements(updateCard).find((node) => node.type === 'Switch');
    assert.equal(toggle.props.checked, enabled);
    toggle.props.onChange(null, { checked: !enabled });
    assert.equal(patches[0].check_updates_on_start, !enabled);
    for (const control of nodes.filter((node) => ['Switch', 'Input', 'SpinButton'].includes(node.type))) {
      assert.ok(control.props['aria-label'] || control.props['aria-labelledby'] || control.props.label,
        control.type + ' requires an accessible name');
    }
  }
});
