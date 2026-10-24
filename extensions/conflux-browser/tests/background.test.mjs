import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

async function harness(state = 'in_progress', options = {}) {
  const events = {};
  const calls = { cancelled: [], erased: [], native: [] };
  let respond;
  const event = (name) => ({ addListener(callback) { events[name] = callback; } });
  const chrome = {
    runtime: { lastError: null, onInstalled: event('installed'), onMessage: event('message'),
      sendNativeMessage(host, payload, callback) { calls.native.push(payload); if (calls.native.length === 1) respond = callback; else callback({ status: 'cancelled' }); } },
    storage: { sync: { get(defaults, callback) { callback({ ...defaults, interceptDownloads: true, fileExtensions: ['.zip'] }); } }, local: {
      get(key, callback) { callback(options.stored || {}); },
      set(values, callback) { Object.assign(options.stored || (options.stored = {}), values); callback(); },
      remove() {},
    }, onChanged: event('settings') },
    notifications: { create() {} },
    contextMenus: { create() {}, onClicked: event('context') },
    downloads: { onCreated: event('created'), onErased: event('erased'), search(filter, callback) { callback([{ id: filter.id, state: options.beforeState || 'in_progress', startTime: '2026-10-24T00:00:00Z' }]); },
      cancel(id, callback) { calls.cancelled.push(id); callback(); }, erase(filter) { calls.erased.push(filter.id); } },
  };
  vm.runInNewContext(fs.readFileSync(new URL('../background.js', import.meta.url), 'utf8'), {
    chrome, console: { log() {}, warn() {}, error() {} }, URL, navigator: { userAgent: 'TestBrowser' },
  });
  const item = { id: 1, state: options.createdState || 'in_progress', startTime: '2026-10-24T00:00:00Z', url: 'https://example.com/file.zip', filename: 'file.zip', totalBytes: 1024, referrer: 'https://example.com/page' };
  const pending = events.created(item);
  for (let i = 0; i < 20; i++) await Promise.resolve();
  return { calls, pending, events, item, respond(value, error = null) {
    chrome.downloads.search = (filter, callback) => callback([{ id: filter.id, state }]);
    chrome.runtime.lastError = error; respond(value);
  } };
}

for (const state of ['complete', 'interrupted']) {
  test('restored ' + state + ' download does not open the desktop dialog', async () => {
    const h = await harness(state, { createdState: state });
    assert.equal(h.calls.native.length, 0);
    await h.pending;
  });
}

test('a download that completes while settings load is not forwarded', async () => {
  const h = await harness('complete', { beforeState: 'complete' });
  assert.equal(h.calls.native.length, 0);
  await h.pending;
});

test('concurrent duplicate events open only one dialog', async () => {
  const h = await harness();
  await h.events.created(h.item);
  assert.equal(h.calls.native.length, 1);
  h.respond({ status: 'cancelled' });
  await h.pending;
});

test('a dismissed download is remembered across worker/browser restarts', async () => {
  const stored = {};
  const first = await harness('in_progress', { stored });
  first.respond({ status: 'cancelled' });
  await first.pending;
  const restarted = await harness('in_progress', { stored });
  assert.equal(restarted.calls.native.length, 0);
  await restarted.pending;
});

test('a new download can reuse an id with a different start time', async () => {
  const h = await harness('in_progress', { stored: { 'conflux.capture.1': '2020-01-01T00:00:00Z' } });
  assert.equal(h.calls.native.length, 1);
  h.respond({ status: 'cancelled' });
  await h.pending;
});

test('automatic capture leaves the browser running until task acceptance', async () => {
  const h = await harness();
  assert.equal(h.calls.native[0].wait_for_acceptance, true);
  assert.equal(h.calls.cancelled.length, 0);
  h.respond({ status: 'accepted' });
  await h.pending;
  assert.equal(h.calls.cancelled.length, 1);
  assert.equal(h.calls.erased.length, 1);
});

for (const status of ['queued', 'cancelled']) {
  test('native ' + status + ' is not permission to cancel a browser download', async () => {
    const h = await harness();
    h.respond({ status });
    await h.pending;
    assert.equal(h.calls.cancelled.length, 0);
    assert.equal(h.calls.erased.length, 0);
  });
}

test('failed native launch/communication preserves the original transfer', async () => {
  const h = await harness();
  h.respond(null, { message: 'Host could not launch' });
  await h.pending;
  assert.equal(h.calls.cancelled.length, 0);
});

test('an already completed browser download retains its history', async () => {
  const h = await harness('complete');
  h.respond({ status: 'accepted' });
  await h.pending;
  assert.equal(h.calls.cancelled.length, 0);
  assert.equal(h.calls.erased.length, 0);
});
