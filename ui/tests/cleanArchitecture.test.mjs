// tests/cleanArchitecture.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import ts from 'typescript';

function loadModule(relativePath, customRequire = () => ({})) {
  const source = fs.readFileSync(new URL(relativePath, import.meta.url), 'utf8');
  const module = { exports: {} };
  const output = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(output, { module, exports: module.exports, require: customRequire });
  return module.exports;
}

test('Result pattern: ok and fail return correct discriminator objects', () => {
  const { ok, fail } = loadModule('../src/types/Result.ts');
  const successRes = ok({ id: '123', value: 42 });
  assert.equal(successRes.success, true);
  assert.equal(successRes.data.id, '123');
  assert.equal(successRes.data.value, 42);

  const errorRes = fail(new Error('something failed'));
  assert.equal(errorRes.success, false);
  assert.equal(errorRes.error.message, 'something failed');
});

test('ManageQueueUseCase: handles pure enqueue, advance, and present operations', () => {
  const { ManageQueueUseCase } = loadModule('../src/features/downloads/utils/downloadQueue.ts');
  const empty = { current: null, queued: [] };
  const reqA = { id: 'A', payload: { url: 'https://example.com/A' } };
  const reqB = { id: 'B', payload: { url: 'https://example.com/B' } };

  // Enqueue when empty and not blocked -> becomes current
  let state = ManageQueueUseCase.enqueue(empty, reqA, false);
  assert.equal(state.current?.id, 'A');
  assert.equal(state.queued.length, 0);

  // Enqueue when current active -> queues up
  state = ManageQueueUseCase.enqueue(state, reqB, false);
  assert.equal(state.current?.id, 'A');
  assert.equal(state.queued.length, 1);
  assert.equal(state.queued[0].id, 'B');

  // Advance current A -> B becomes current
  state = ManageQueueUseCase.advance(state, 'A');
  assert.equal(state.current?.id, 'B');
  assert.equal(state.queued.length, 0);

  // Present queued when current is null and queued has items
  const queuedState = { current: null, queued: [reqA] };
  const presented = ManageQueueUseCase.presentQueued(queuedState);
  assert.equal(presented.current?.id, 'A');
  assert.equal(presented.queued.length, 0);
});

test('StartDownloadUseCase: rejects empty or whitespace-only URLs before repository call', async () => {
  const resultMod = loadModule('../src/types/Result.ts');
  const validationMod = loadModule('../src/features/downloads/utils/downloadValidation.ts', (path) => {
    if (path.includes('Result')) return resultMod;
    return {};
  });

  const { StartDownloadUseCase } = validationMod;
  let repoCalled = false;
  const mockRepo = {
    startDownload: async (params) => {
      repoCalled = true;
      return resultMod.ok({ id: 'task-1', ...params });
    },
  };

  const useCase = new StartDownloadUseCase(mockRepo);

  // Empty string
  const emptyRes = await useCase.execute({ url: '   ', saveDir: '/downloads', filename: null });
  assert.equal(emptyRes.success, false);
  assert.equal(emptyRes.error.kind, 'INVALID_PARAMS');
  assert.equal(repoCalled, false);

  // Valid trimmed URL
  const validRes = await useCase.execute({ url: '  https://example.com/test.iso  ', saveDir: '/downloads', filename: 'test.iso' });
  assert.equal(validRes.success, true);
  assert.equal(validRes.data.url, 'https://example.com/test.iso');
  assert.equal(repoCalled, true);
});

test('UpdateSettingsUseCase: sanitizes adapter_overrides before passing to repository', async () => {
  const resultMod = loadModule('../src/types/Result.ts');
  const settingsMod = loadModule('../src/features/settings/utils/settingsValidation.ts', () => resultMod);

  const { UpdateSettingsUseCase } = settingsMod;
  let passedSettings = null;
  const mockRepo = {
    updateSettings: async (settings) => {
      passedSettings = settings;
      return resultMod.ok(settings);
    },
  };

  const useCase = new UpdateSettingsUseCase(mockRepo);
  const inputSettings = {
    theme: 'dark',
    backdrop: 'mica',
    default_save_dir: '/downloads',
    connections_per_adapter: 4,
    slow_mode: false,
    slow_mode_limit_kib: 1024,
    notify_on_complete: true,
    close_to_tray: true,
    auto_aggregate_adapters: true,
    check_updates_on_start: true,
    adapter_overrides: { eth0: false, wlan0: true },
  };

  const res = await useCase.execute(inputSettings);
  assert.equal(res.success, true);
  assert.equal(passedSettings.theme, 'dark');
  assert.equal('adapter_overrides' in passedSettings, false);
});
