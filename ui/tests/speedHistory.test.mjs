import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

function historyFor(initialTasks) {
  let tasks = initialTasks;
  let result;
  let cursor = 0;
  let changed = false;
  let pendingEffects = [];
  let intervalCount = 0;
  const values = [];
  const effects = [];
  const intervals = new Map();
  const module = { exports: {} };
  const source = fs.readFileSync(new URL('../src/hooks/useSpeedHistory.ts', import.meta.url), 'utf8');
  const output = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const react = {
    useRef: (current) => {
      const index = cursor++;
      values[index] ??= { current };
      return values[index];
    },
    useState: (initial) => {
      const index = cursor++;
      if (!Object.hasOwn(values, index)) values[index] = typeof initial === 'function' ? initial() : initial;
      return [values[index], (update) => {
        const next = typeof update === 'function' ? update(values[index]) : update;
        changed ||= !Object.is(next, values[index]);
        values[index] = next;
      }];
    },
    useEffect: (effect, dependencies) => {
      const index = cursor++;
      const previous = effects[index];
      if (previous && dependencies.every((dependency, i) => Object.is(dependency, previous.dependencies[i]))) return;
      pendingEffects.push(() => {
        previous?.cleanup?.();
        effects[index] = { dependencies, cleanup: effect() };
      });
    },
  };
  vm.runInNewContext(output, {
    module,
    exports: module.exports,
    require: (name) => {
      assert.equal(name, 'react');
      return react;
    },
    window: {
      setInterval: (callback, delay) => {
        assert.equal(delay, 1000);
        intervals.set(++intervalCount, callback);
        return intervalCount;
      },
      clearInterval: (id) => intervals.delete(id),
    },
  });
  function render() {
    let renders = 0;
    do {
      assert.ok(++renders < 20, 'hook must settle after effects');
      cursor = 0;
      changed = false;
      pendingEffects = [];
      result = module.exports.useSpeedHistory(tasks);
      pendingEffects.forEach((effect) => effect());
    } while (changed);
  }
  render();
  return {
    tick: () => { intervals.forEach((callback) => callback()); render(); },
    setTasks: (next) => { tasks = next; render(); },
    samples: () => result.history,
    taskSamples: () => result.taskHistory,
    intervalCount: () => intervalCount,
    activeIntervals: () => intervals.size,
    cleanup: () => effects.forEach((effect) => effect?.cleanup?.()),
  };
}

const plain = (value) => JSON.parse(JSON.stringify(value));
const task = (id, speed, status = 'downloading') => ({
  id,
  status,
  speed_bytes_sec: speed,
  adapters: [{ adapter_id: null, speed_bytes_sec: speed }],
});

test('task histories stay separate while aggregate and shared-adapter rates are summed', () => {
  const active = task('first', 100);
  const second = task('second', 200);
  const paused = task('paused', 900, 'paused');
  const history = historyFor([active, second, paused]);
  assert.deepEqual(plain(history.taskSamples()), { first: [], second: [], paused: [] });
  history.tick();
  const sample = history.samples()[0];
  assert.equal(sample.total, 300);
  assert.equal(sample.byAdapter.default, 300);
  assert.equal(sample.byTask.first, 100);
  assert.equal(sample.byTask.second, 200);
  assert.equal(sample.byTask.paused, undefined);
  assert.deepEqual(plain(history.taskSamples()), { first: [100], second: [200], paused: [] });

  history.setTasks([{ ...active, status: 'completed' }, second, paused]);
  history.tick();
  assert.equal(history.samples()[1].byTask.first, undefined);
  assert.equal(history.samples()[1].total, 200);
  assert.equal(history.samples()[0].byTask.first, 100);
  assert.deepEqual(plain(history.taskSamples().first), [100]);
});

test('rolling histories retain only the most recent sixty one-second samples', () => {
  const history = historyFor([task('first', 0)]);
  for (let speed = 1; speed <= 61; speed++) {
    history.setTasks([task('first', speed)]);
    history.tick();
  }
  assert.equal(history.samples().length, 60);
  assert.equal(history.samples()[0].byTask.first, 2);
  assert.equal(history.samples()[59].byTask.first, 61);
  assert.deepEqual(plain(history.taskSamples().first), Array.from({ length: 60 }, (_, i) => i + 2));
  assert.equal(history.intervalCount(), 1);
  assert.equal(history.activeIntervals(), 1);
  history.cleanup();
  assert.equal(history.activeIntervals(), 0);
});

test('pause, error and completion freeze history; resuming appends without inactive zero samples', () => {
  const history = historyFor([task('first', 100)]);
  history.tick();
  const frozen = history.taskSamples().first;
  for (const status of ['paused', 'error', 'completed']) {
    history.setTasks([task('first', 900, status)]);
    history.tick();
    history.tick();
    assert.equal(history.taskSamples().first, frozen);
    assert.equal(history.samples().at(-1).total, 0);
  }
  history.setTasks([task('first', 150)]);
  history.tick();
  assert.deepEqual(plain(history.taskSamples().first), [100, 150]);
  assert.deepEqual(plain(frozen), [100]);
});

test('all tasks collect history before they are selected and retain it across task reordering', () => {
  const history = historyFor([task('first', 100), task('second', 200)]);
  history.tick();
  history.setTasks([task('second', 250), task('first', 150)]);
  history.tick();
  assert.deepEqual(plain(history.taskSamples().second), [200, 250]);
  assert.deepEqual(plain(history.taskSamples().first), [100, 150]);
});

test('the next sampling tick prunes removed tasks and initializes new task measurements', () => {
  const history = historyFor([task('removed', 100), task('kept', 200)]);
  history.tick();
  const previous = history.taskSamples();
  const kept = previous.kept;
  Object.freeze(previous.removed);
  Object.freeze(previous.kept);
  Object.freeze(previous);
  history.setTasks([task('kept', 0, 'paused'), task('new', 300)]);
  assert.equal(history.taskSamples().new, undefined);
  assert.equal(history.taskSamples().kept, kept);
  history.tick();
  assert.deepEqual(plain(history.taskSamples()), { kept: [200], new: [300] });
  assert.deepEqual(plain(previous), { removed: [100], kept: [200] });
  history.setTasks([]);
  history.tick();
  assert.deepEqual(plain(history.taskSamples()), {});
});

test('active zero measurements are preserved while non-finite and negative rates become zero', () => {
  const history = historyFor([task('first', 100)]);
  history.tick();
  for (const speed of [0, Number.NaN, Number.POSITIVE_INFINITY, -40]) {
    history.setTasks([task('first', speed)]);
    history.tick();
    const sample = history.samples().at(-1);
    assert.equal(sample.total, 0);
    assert.equal(sample.byTask.first, 0);
    assert.equal(sample.byAdapter.default, 0);
  }
  assert.deepEqual(plain(history.taskSamples().first), [100, 0, 0, 0, 0]);
});
