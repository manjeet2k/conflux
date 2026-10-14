import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

function load(source, require = () => { throw Error('Unexpected dependency'); }) {
  const module = { exports: {} };
  const output = ts.transpileModule(fs.readFileSync(source, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(output, { module, exports: module.exports, require, URL });
  return module.exports;
}
const { enqueueDownload, advanceDownload, presentQueuedDownload } = load(new URL('../src/utils/downloadQueue.ts', import.meta.url));
const request = (id) => ({ id, payload: { url: 'https://example.com/' + id } });
const empty = () => ({ current: null, queued: [] });

test('queue arrivals retain the active request identity and edits', () => {
  let state = enqueueDownload(empty(), request('A'), false);
  const original = state.current;
  state = enqueueDownload(state, request('B'), true);
  assert.equal(state.current, original);
  assert.equal(state.queued[0].id, 'B');
});

test('consecutive events cannot overwrite the current request', () => {
  let state = enqueueDownload(empty(), request('A'), false);
  state = enqueueDownload(state, request('B'), false);
  assert.equal(state.current.id, 'A');
  assert.equal(state.queued[0].id, 'B');
});

test('stale submission completion cannot advance a newer request', () => {
  let state = enqueueDownload(empty(), request('A'), false);
  state = enqueueDownload(state, request('B'), true);
  state = advanceDownload(state, 'A');
  const before = state;
  state = advanceDownload(state, 'A');
  assert.equal(state, before);
  assert.equal(state.current.id, 'B');
});

test('closing another modal presents queued downloads in order', () => {
  let state = enqueueDownload(empty(), request('A'), true);
  state = enqueueDownload(state, request('B'), true);
  assert.equal(state.current, null);
  state = presentQueuedDownload(state);
  assert.equal(state.current.id, 'A');
  assert.equal(state.queued[0].id, 'B');
});

const jsx = (type, props, key) => ({ type, props, key });
const react = { useState: (initial) => [initial, () => {}], useEffect: () => {} };
const fluent = new Proxy({ makeStyles: () => () => ({}), tokens: {} }, { get: (object, key) => object[key] ?? key });
const requireShim = (name) => {
  if (name === 'react') return react;
  if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
  if (name === '@fluentui/react-components') return fluent;
  if (name === '@fluentui/react-icons') return {};
  if (name === '@tauri-apps/plugin-dialog') return { open: async () => null };
  if (name === '@tauri-apps/api/path') return { downloadDir: async () => '/downloads' };
  if (name === '../api') return { api: {}, errorText: String };
  if (name === '../utils/formatters') return { formatBytes: String };
  if (name === './FileIcon') return { FileIcon: 'FileIcon' };
  throw Error('Unexpected dependency: ' + name);
};
const { AddDownloadDialog } = load(new URL('../src/components/AddDownloadDialog.tsx', import.meta.url), requireShim);
const props = { open: true, requestId: 'A', initialUrl: 'https://example.com/A', defaultSaveDir: '/downloads', onStart: async () => {}, onClose: () => {} };

test('actual Add form key is stable while the queue grows', () => {
  const first = AddDownloadDialog({ ...props, queueCount: 0 });
  const second = AddDownloadDialog({ ...props, queueCount: 2 });
  assert.equal(first.props.children.key, second.props.children.key);
  assert.notEqual(first.props.children.key, AddDownloadDialog({ ...props, requestId: 'B' }).props.children.key);
});

test('actual delayed form completion cannot close a replacement request', async () => {
  let resolveStart;
  const pendingStart = new Promise((resolve) => { resolveStart = resolve; });
  let state = enqueueDownload(empty(), request('A'), false);
  state = enqueueDownload(state, request('B'), true);
  const onClose = () => { state = advanceDownload(state, 'A'); };
  const wrapper = AddDownloadDialog({ ...props, onStart: () => pendingStart, onClose });
  const form = wrapper.props.children.type(wrapper.props.children.props).props.children;
  const submission = form.props.onSubmit({ preventDefault() {} });
  onClose();
  assert.equal(state.current.id, 'B');
  resolveStart();
  await submission;
  assert.equal(state.current.id, 'B');
});
