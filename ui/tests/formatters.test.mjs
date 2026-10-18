import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

const source = fs.readFileSync(new URL('../src/utils/formatters.ts', import.meta.url), 'utf8');
const module = { exports: {} };
vm.runInNewContext(ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText, { module, exports: module.exports });
const { formatSpeed, formatBytes } = module.exports;

test('Mb/s uses eight bits per byte and decimal megabits, at any magnitude', () => {
  assert.equal(formatSpeed(125_000, 'bits'), '1 Mb/s');
  assert.equal(formatSpeed(1_000_000, 'bits'), '8 Mb/s');
  assert.equal(formatSpeed(1024 * 1024, 'bits'), '8.39 Mb/s');
  assert.equal(formatSpeed(125_000_000, 'bits'), '1000 Mb/s');
  assert.equal(formatSpeed(1024, 'bits'), '0.01 Mb/s');
});

test('byte mode keeps existing speed and file-size formatting', () => {
  assert.equal(formatSpeed(1024), '1 KB/s');
  assert.equal(formatSpeed(1024 * 1024, 'bytes'), '1 MB/s');
  assert.equal(formatSpeed(1024 ** 3, 'bytes'), '1 GB/s');
  assert.equal(formatBytes(1024 * 1024), '1 MB');
});

test('idle and unavailable speeds remain valid in both modes', () => {
  for (const unit of ['bytes', 'bits']) {
    assert.equal(formatSpeed(0, unit), unit === 'bits' ? '0 Mb/s' : '0 KB/s');
    assert.equal(formatSpeed(-1, unit), unit === 'bits' ? '0 Mb/s' : '0 KB/s');
    for (const value of [NaN, Infinity, -Infinity]) {
      assert.equal(formatSpeed(value, unit), '--');
    }
  }
});
