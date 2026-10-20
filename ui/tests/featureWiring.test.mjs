import assert from 'node:assert/strict';
import test from 'node:test';
import { sourceLoader, hookHarness, deferred, flushPromises } from './helpers/loadSource.mjs';

test('the download hook validates through its use case and preserves browser handoff metadata', async () => {
  const harness = hookHarness();
  const calls = [];
  let failure = null;
  const task = { id: 'task-1' };
  const load = sourceLoader({
    react: harness.hooks,
    '@tauri-apps/api/core': { invoke: async (name, args) => {
      if (name === 'list_tasks') return [];
      assert.equal(name, 'start_download');
      calls.push(args);
      if (failure) throw failure;
      return task;
    } },
    '@tauri-apps/api/event': { listen: async () => () => {} },
  });
  const { useDownloads } = load('features/downloads/hooks/useDownloads.ts');
  const errors = [];
  const onError = (error) => errors.push(error);
  const hook = harness.render(() => useDownloads(onError));
  for (const url of ['', 'ftp://example.com/file.zip']) {
    await assert.rejects(hook.start({ url, saveDir: '/downloads', filename: null }), (error) => error.kind === 'INVALID_PARAMS');
  }
  await assert.rejects(hook.start({ url: 'https://example.com/file.zip', saveDir: ' ', filename: null }),
    (error) => error.kind === 'INVALID_PARAMS');
  assert.equal(calls.length, 0);
  const headers = { cookie: 'session=private', referer: 'https://example.com/' };
  assert.equal(await hook.start({
    url: ' https://example.com/file.zip ', saveDir: ' /downloads ', filename: 'browser.zip',
    headers, handoffId: 'handoff-1',
  }), task);
  assert.equal(calls[0].url, 'https://example.com/file.zip');
  assert.equal(calls[0].saveDir, '/downloads');
  assert.equal(calls[0].filename, 'browser.zip');
  assert.equal(calls[0].headers, headers);
  assert.equal(calls[0].handoffId, 'handoff-1');
  failure = 'HTTP 403';
  await assert.rejects(hook.start({ url: 'https://example.com/file.zip', saveDir: '/downloads', filename: null }),
    (error) => error.message === failure);
  assert.equal(errors.length, 0, 'submission errors remain available to the dialog');
  harness.dispose();
  await flushPromises();
});

test('settings use case preserves serialized saves and rolls back failed patches without stale adapter overrides', async () => {
  const harness = hookHarness();
  const firstSave = deferred();
  const saves = [], errors = [];
  const initial = {
    theme: 'dark', backdrop: 'mica', connections_per_adapter: 4,
    adapter_overrides: { ethernet: false }, schema_version: 3,
  };
  const load = sourceLoader({
    react: harness.hooks,
    '@tauri-apps/api/core': { invoke: async (name, args) => {
      if (name === 'get_settings') return initial;
      assert.equal(name, 'update_settings');
      saves.push(args.settings);
      if (saves.length === 1) return firstSave.promise;
      return args.settings;
    } },
    '@tauri-apps/api/event': {},
  });
  const { useSettings } = load('features/settings/hooks/useSettings.ts');
  const onError = (message) => errors.push(message);
  let hook = harness.render(() => useSettings(onError));
  await flushPromises();
  hook = harness.render(() => useSettings(onError));
  assert.equal(hook.loaded, true);
  hook.update({ theme: 'light' });
  hook.update({ connections_per_adapter: 8 });
  await flushPromises();
  assert.equal(saves.length, 1, 'second save waits for the first');
  assert.equal(saves[0].theme, 'light');
  assert.equal('adapter_overrides' in saves[0], false);
  firstSave.reject('Disk full');
  await flushPromises();
  hook = harness.render(() => useSettings(onError));
  assert.equal(saves.length, 2);
  assert.equal(saves[1].theme, 'dark', 'failed theme is not included in the next save');
  assert.equal(saves[1].connections_per_adapter, 8);
  assert.equal(saves[1].schema_version, 3);
  assert.equal('adapter_overrides' in saves[1], false);
  assert.equal(hook.settings.theme, 'dark');
  assert.equal(hook.settings.connections_per_adapter, 8);
  assert.equal(errors.length, 1);
  assert.match(errors[0], /Disk full/);
  harness.dispose();
});

test('extracted Add Download hook drops cookies for edited origins and ignores stale probes', async () => {
  const harness = hookHarness();
  const timers = new Map(), probes = [], starts = [];
  let timerId = 0;
  const headers = { cookie: 'session=private' };
  const options = {
    initialUrl: 'https://example.com/first', initialHeaders: headers, initialHandoffId: 'handoff-1',
    defaultSaveDir: '/downloads', onStart: async (args) => starts.push(args), onClose: () => {},
  };
  const load = sourceLoader({
    react: harness.hooks,
    '@tauri-apps/plugin-dialog': {}, '@tauri-apps/api/path': {},
    '@tauri-apps/api/event': {},
    '@tauri-apps/api/core': { invoke: (name, args) => {
      assert.equal(name, 'probe_url');
      const pending = deferred();
      probes.push({ ...pending, ...args });
      return pending.promise;
    } },
  }, { window: {
    setTimeout: (callback) => { timers.set(++timerId, callback); return timerId; },
    clearTimeout: (id) => timers.delete(id),
  } });
  const fireTimers = () => { const queue = [...timers.values()]; timers.clear(); queue.forEach((callback) => callback()); };
  const { useAddDownload } = load('features/downloads/hooks/useAddDownload.ts');
  let hook = harness.render(() => useAddDownload(options));
  fireTimers();
  assert.equal(probes[0].headers, headers);
  await hook.submit({ preventDefault() {} });
  assert.equal(starts[0].headers, headers);
  assert.equal(starts[0].handoffId, 'handoff-1');
  hook.setUrl('https://other.example/second');
  hook = harness.render(() => useAddDownload(options));
  fireTimers();
  assert.equal(probes[1].headers, null);
  probes[1].resolve({ filename: 'second.zip' });
  await flushPromises();
  probes[0].resolve({ filename: 'stale.zip' });
  await flushPromises();
  hook = harness.render(() => useAddDownload(options));
  assert.equal(hook.probe.filename, 'second.zip');
  assert.equal(hook.effectiveName, 'second.zip');
  // The earlier successful submission leaves this form submitting; a real dialog remounts.
  // A fresh form demonstrates the same origin boundary on the submission path.
  const fresh = hookHarness();
  const freshLoad = sourceLoader({
    react: fresh.hooks, '@tauri-apps/plugin-dialog': {}, '@tauri-apps/api/path': {},
    '@tauri-apps/api/event': {}, '@tauri-apps/api/core': {},
  }, { window: { setTimeout: () => 1, clearTimeout: () => {} } });
  const { useAddDownload: useFresh } = freshLoad('features/downloads/hooks/useAddDownload.ts');
  let edited = fresh.render(() => useFresh(options));
  edited.setUrl('https://other.example/second');
  edited = fresh.render(() => useFresh(options));
  await edited.submit({ preventDefault() {} });
  assert.equal(starts[1].headers, null);
  assert.equal(starts[1].url, 'https://other.example/second');
  harness.dispose();
  fresh.dispose();
});

test('browser registration invokes the exposed native command', async () => {
  const calls = [];
  const load = sourceLoader({
    '@tauri-apps/api/core': { invoke: async (command, args) => calls.push({ command, args }) },
    '@tauri-apps/api/event': {},
  });
  const { settingsApi } = load('features/settings/api/settings.api.ts');
  await settingsApi.registerBrowserExtension();
  assert.equal(calls[0].command, 'register_browser_extension');
});
