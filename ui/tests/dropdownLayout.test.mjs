import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

const module = { exports: {} };
const source = fs.readFileSync(new URL('../src/utils/dropdownLayout.ts', import.meta.url), 'utf8');
const output = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
vm.runInNewContext(output, { module, exports: module.exports });
const { dropdownLayout } = module.exports;

test('125% scaling reproduces the recorded seven-item popup and selected row', () => {
  const layout = dropdownLayout(1.25, 3, 7);
  // Recording: popup top 200px, selected item top 347px, bottom 534px.
  assert.equal(Math.round(layout.selectedTop * 1.25), 147);
  assert.equal(Math.round(layout.height * 1.25), 334);
  assert.equal(Math.round(layout.minWidth * 1.25), 163);
});

test('each selected choice stays at its trigger height as the popup anchor changes', () => {
  for (const scale of [1, 1.25, 1.5, 1.75, 2]) {
    for (let index = 0; index < 7; index++) {
      const layout = dropdownLayout(scale, index, 7);
      assert.ok(Math.abs(layout.selectedTop + layout.rowHeight + layout.revealBottom - layout.height) < 1e-8);
      assert.ok(Math.abs(layout.selectedTop * scale - Math.round(layout.selectedTop * scale)) < 1e-8);
    }
  }
});

test('100% scaling retains WinUI item margins and four-pixel popup padding', () => {
  const layout = dropdownLayout(1, 0, 3);
  assert.equal(layout.selectedTop, 7);
  assert.equal(layout.height, 118);
});
