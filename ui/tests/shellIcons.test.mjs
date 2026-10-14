import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

function load() {
  const calls = [], rendered = [];
  let result = { width: 1, height: 1, rgba: [64, 128, 192, 128] };
  const api = { getShellIcon: (extension, folder) => { calls.push({ extension, folder }); return Promise.resolve(result); } };
  const module = { exports: {} };
  const source = fs.readFileSync(new URL('../src/utils/shellIcons.ts', import.meta.url), 'utf8');
  const output = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  vm.runInNewContext(output, {
    module, exports: module.exports, require: () => ({ api }),
    console: { warn() {} }, Uint8ClampedArray,
    ImageData: class { constructor(data, width, height) { Object.assign(this, { data, width, height }); } },
    document: { createElement: () => ({ getContext: () => ({ putImageData: data => rendered.push(data) }), toDataURL: () => 'data:image/png;native-icon' }) },
  });
  return { ...module.exports, calls, rendered, setResult: value => { result = value; } };
}

test('rows of the same type share one pending Windows lookup', async () => {
  const m = load();
  const first = m.shellIconUrl('pdf', false);
  const second = m.shellIconUrl('pdf', false);
  assert.equal(first, second);
  assert.equal(m.calls.length, 1);
  assert.equal(await first, 'data:image/png;native-icon');
  assert.deepEqual(Array.from(m.rendered[0].data), [64, 128, 192, 128]);
});

test('a .folder file never collides with the Windows folder icon cache', async () => {
  const m = load();
  await m.shellIconUrl(null, true);
  await m.shellIconUrl('folder', false);
  assert.deepEqual(m.calls, [{ extension: null, folder: true }, { extension: 'folder', folder: false }]);
});

test('only the normalized extension reaches the Shell lookup', () => {
  const m = load();
  assert.equal(m.fileExtension('C:\\Downloads\\Report.PDF'), 'pdf');
  assert.equal(m.fileExtension('/tmp/archive.tar.gz'), 'gz');
  assert.equal(m.fileExtension('.env'), null);
  assert.equal(m.fileExtension('README'), null);
  assert.equal(m.fileExtension('file.' + 'x'.repeat(33)), null);
  assert.equal(m.fileExtension('file.pd\0f'), null);
});

test('missing or invalid native pixels retain the neutral placeholder', async () => {
  const m = load();
  m.setResult(null);
  assert.equal(await m.shellIconUrl('missing', false), null);
  m.setResult({ width: 1, height: 1, rgba: [] });
  assert.equal(await m.shellIconUrl('invalid', false), null);
  assert.equal(m.rendered.length, 0);
});
