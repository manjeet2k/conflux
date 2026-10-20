import fs from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';

// Resolve the same relative and @/ imports as Vite, while mocking platform boundaries.
export function sourceLoader(mocks = {}, globals = {}) {
  const cache = new Map();
  const root = new URL('../../src/', import.meta.url);
  function load(url) {
    if (cache.has(url.href)) return cache.get(url.href).exports;
    const module = { exports: {} };
    cache.set(url.href, module);
    const output = ts.transpileModule(fs.readFileSync(url, 'utf8'), {
      fileName: url.pathname,
      compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
    }).outputText;
    const require = (name) => {
      if (Object.hasOwn(mocks, name)) return mocks[name];
      if (!name.startsWith('.') && !name.startsWith('@/')) throw Error('Unexpected dependency: ' + name);
      const target = name.startsWith('@/') ? new URL(name.slice(2), root) : new URL(name, url);
      for (const suffix of ['', '.ts', '.tsx', '/index.ts']) {
        const candidate = new URL(target.href + suffix);
        if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) return load(candidate);
      }
      throw Error('Cannot resolve: ' + name);
    };
    vm.runInNewContext(output, { module, exports: module.exports, require, URL, console, ...globals });
    return module.exports;
  }
  return (path) => load(new URL(path, root));
}

export const jsx = (type, props, key) => ({ type, props, key });
export const react = {
  useState: (initial) => [typeof initial === 'function' ? initial() : initial, () => {}],
  useEffect: () => {},
  useCallback: (callback) => callback,
  useMemo: (factory) => factory(),
  useRef: (current) => ({ current }),
};
export const fluent = new Proxy({ makeStyles: () => () => ({}), tokens: {} }, {
  get: (object, key) => object[key] ?? key,
});
export function elements(node) {
  if (!node || typeof node !== 'object') return [];
  if (Array.isArray(node)) return node.flatMap(elements);
  return [node, ...elements(node.props?.children)];
}

export function hookHarness() {
  const values = [], effects = [];
  let cursor = 0, pending = [];
  const hooks = {
    useState(initial) {
      const index = cursor++;
      if (!Object.hasOwn(values, index)) values[index] = typeof initial === 'function' ? initial() : initial;
      return [values[index], (next) => { values[index] = typeof next === 'function' ? next(values[index]) : next; }];
    },
    useRef(current) {
      const index = cursor++;
      values[index] ??= { current };
      return values[index];
    },
    useMemo(factory, dependencies) {
      const index = cursor++;
      const previous = values[index];
      if (!previous || !dependencies.every((value, i) => Object.is(value, previous.dependencies[i]))) {
        values[index] = { dependencies, value: factory() };
      }
      return values[index].value;
    },
    useCallback: (callback, dependencies) => hooks.useMemo(() => callback, dependencies),
    useEffect(effect, dependencies) {
      const index = cursor++;
      const old = effects[index];
      if (old && dependencies.every((value, i) => Object.is(value, old.dependencies[i]))) return;
      pending.push(() => {
        old?.cleanup?.();
        effects[index] = { dependencies, cleanup: effect() };
      });
    },
  };
  return {
    hooks,
    render(callback) {
      cursor = 0;
      const result = callback();
      const queue = pending;
      pending = [];
      queue.forEach((effect) => effect());
      return result;
    },
    dispose() { effects.forEach((effect) => effect?.cleanup?.()); },
  };
}

export const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
export async function flushPromises() {
  for (let i = 0; i < 12; i++) await Promise.resolve();
}
