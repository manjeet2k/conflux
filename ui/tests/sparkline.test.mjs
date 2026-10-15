import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

const module = { exports: {} };
const source = fs.readFileSync(new URL('../src/utils/sparkline.ts', import.meta.url), 'utf8');
const output = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
vm.runInNewContext(output, { module, exports: module.exports });
const { SPEED_HISTORY_CAPACITY, normalizeSpeedSamples, speedWindowStats, dampCeiling, buildSmoothPath } = module.exports;

test('visible history has sixty points with zero padding and only the newest samples', () => {
  assert.equal(SPEED_HISTORY_CAPACITY, 60);
  const samples = [10, 20];
  const normalized = normalizeSpeedSamples(samples);
  assert.equal(normalized.length, 60);
  assert.ok(normalized.slice(0, 58).every((value) => value === 0));
  assert.deepEqual(Array.from(normalized.slice(-2)), samples);
  assert.deepEqual(samples, [10, 20]);
  assert.deepEqual(Array.from(normalizeSpeedSamples(Array.from({ length: 75 }, (_, index) => index))),
    Array.from({ length: 60 }, (_, index) => index + 15));
  assert.equal(normalizeSpeedSamples([]).length, 60);
  assert.deepEqual(Array.from(normalizeSpeedSamples([1], 0)), []);
});

test('negative and nonfinite samples become zero in the graph and its statistics', () => {
  const samples = [-20, NaN, Infinity, -Infinity, 50];
  assert.deepEqual(Array.from(normalizeSpeedSamples(samples, 5)), [0, 0, 0, 0, 50]);
  const stats = speedWindowStats(samples);
  assert.equal(stats.peak, 50);
  assert.equal(stats.average, 10);
  assert.equal(stats.ceiling, 1024 * 1024);
});

test('statistics count recorded zeroes without counting synthetic leading padding', () => {
  const stats = speedWindowStats([0, 6 * 1024 * 1024, 0]);
  assert.equal(stats.peak, 6 * 1024 * 1024);
  assert.equal(stats.average, 2 * 1024 * 1024);
  assert.equal(stats.ceiling, stats.peak * 1.18);
  assert.equal(speedWindowStats([8 * 1024 * 1024]).average, 8 * 1024 * 1024);
  assert.equal(speedWindowStats([]).average, 0);
  assert.equal(speedWindowStats([]).ceiling, 1024 * 1024);
  const trimmed = speedWindowStats([100 * 1024 * 1024, ...Array(59).fill(0), 60]);
  assert.equal(trimmed.peak, 60);
  assert.equal(trimmed.average, 1);
});

test('scale damping approaches the target without overshooting in either direction', () => {
  assert.equal(dampCeiling(10, 100, 0), 10);
  assert.equal(dampCeiling(10, 100, -1), 10);
  for (const [start, target] of [[10, 100], [100, 10]]) {
    let current = start;
    for (const elapsed of [1, 16, 33, 140]) {
      const next = dampCeiling(current, target, elapsed);
      assert.ok(next >= Math.min(current, target) && next <= Math.max(current, target));
      assert.ok(Math.abs(next - target) < Math.abs(current - target));
      current = next;
    }
    assert.ok(Math.abs(dampCeiling(start, target, 1000) - target) < 0.0001);
  }
});

test('scale damping depends on elapsed time rather than the number of animation frames', () => {
  const steps = [7, 18, 33, 1, 100, 24];
  const stepped = steps.reduce((current, elapsed) => dampCeiling(current, 100, elapsed), 10);
  const combined = dampCeiling(10, 100, steps.reduce((total, elapsed) => total + elapsed, 0));
  assert.ok(Math.abs(stepped - combined) < 1e-10);
});

function parsePath(path) {
  const tokens = path.match(/[MC]|-?(?:\d*\.\d+|\d+)(?:e[+-]?\d+)?/gi) ?? [];
  assert.equal(tokens.shift(), 'M');
  const first = [Number(tokens.shift()), Number(tokens.shift())];
  const segments = [];
  let start = first;
  while (tokens.length) {
    assert.equal(tokens.shift(), 'C');
    const control1 = [Number(tokens.shift()), Number(tokens.shift())];
    const control2 = [Number(tokens.shift()), Number(tokens.shift())];
    const end = [Number(tokens.shift()), Number(tokens.shift())];
    segments.push({ start, control1, control2, end });
    start = end;
  }
  return { first, segments };
}

const bezier = (start, control1, control2, end, t) => {
  const inverse = 1 - t;
  return inverse ** 3 * start + 3 * inverse ** 2 * t * control1 + 3 * inverse * t ** 2 * control2 + t ** 3 * end;
};

function assertBoundedCurve(points) {
  const { segments } = parsePath(buildSmoothPath(points, 58));
  assert.equal(segments.length, points.length - 1);
  for (const { start, control1, control2, end } of segments) {
    const low = Math.min(start[1], end[1]);
    const high = Math.max(start[1], end[1]);
    for (const control of [control1, control2]) {
      assert.ok(control[0] >= start[0] && control[0] <= end[0]);
      assert.ok(control[1] >= low && control[1] <= high);
      assert.ok(control[1] >= 4 && control[1] <= 58);
    }
    let previousY = start[1];
    let previousX = start[0];
    for (let step = 1; step <= 100; step++) {
      const t = step / 100;
      const x = bezier(start[0], control1[0], control2[0], end[0], t);
      const y = bezier(start[1], control1[1], control2[1], end[1], t);
      assert.ok(x >= previousX - 1e-8 && x <= end[0] + 1e-8);
      assert.ok(y >= low - 1e-8 && y <= high + 1e-8);
      if (end[1] >= start[1]) assert.ok(y >= previousY - 1e-8);
      else assert.ok(y <= previousY + 1e-8);
      previousX = x;
      previousY = y;
    }
  }
}

test('dense spikes and drops to zero never overshoot their neighboring values or the baseline', () => {
  assertBoundedCurve(Array.from({ length: 60 }, (_, index) => [index * 280 / 59, index % 3 === 0 ? 58 : 4]));
  assertBoundedCurve([[0, 58], [0.5, 10], [90, 11], [92, 55], [279, 58], [280, 58]]);
});

test('constant samples stay flat, including an all-zero speed window at the baseline', () => {
  for (const height of [4, 25, 58]) {
    const points = Array.from({ length: 60 }, (_, index) => [index * 4, height]);
    assertBoundedCurve(points);
    const { segments } = parsePath(buildSmoothPath(points, 58));
    assert.ok(segments.every(({ control1, control2, end }) =>
      control1[1] === height && control2[1] === height && end[1] === height));
  }
});

test('path handles empty and single histories and clamps sample positions to the drawable height', () => {
  assert.equal(buildSmoothPath([], 58), '');
  assert.equal(buildSmoothPath([[10, 58]], 58), 'M 10,58');
  const { first, segments } = parsePath(buildSmoothPath([[0, -10], [140, 90], [280, NaN]], 58));
  assert.deepEqual(first, [0, 4]);
  assert.deepEqual(segments.map(({ end }) => end), [[140, 58], [280, 58]]);
  assertBoundedCurve([[0, -10], [140, 90], [280, NaN]]);
});
