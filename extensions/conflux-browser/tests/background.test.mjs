import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

async function harness(state = 'in_progress') {
  const events = {};
  const calls = { cancelled: [], erased: [], native: [] };
  let respond;
  const event = (name) => ({ addListener(callback) { events[name] = callback; } });
  const chrome = {
    runtime: { lastError: null, onInstalled: event('installed'), onMessage: event('message'),
      sendNativeMessage(host, payload, callback) { calls.native.push(payload); respond = callback; } },
    storage: { sync: { get(defaults, callback) { callback({ ...defaults, interceptDownloads: true, fileExtensions: ['.zip'] }); } }, local: {}, onChanged: event('settings') },
    notifications: { create() {} },
    contextMenus: { create() {}, onClicked: event('context') },
    downloads: { onCreated: event('created'), search(filter, callback) { callback([{ id: filter.id, state }]); },
      cancel(id, callback) { calls.cancelled.push(id); callback(); }, erase(filter) { calls.erased.push(filter.id); } },
  };
  vm.runInNewContext(fs.readFileSync(new URL('../background.js', import.meta.url), 'utf8'), {
    chrome, console: { log() {}, warn() {}, error() {} }, URL, navigator: { userAgent: 'TestBrowser' },
  });
  const pending = events.created({ id: 1, url: 'https://example.com/file.zip', filename: 'file.zip', totalBytes: 1024, referrer: 'https://example.com/page' });
  await Promise.resolve();
  await Promise.resolve();
  return { calls, pending, respond(value, error = null) { chrome.runtime.lastError = error; respond(value); } };
}

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
