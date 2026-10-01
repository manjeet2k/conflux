// Dev-only: a simulated backend so the UI can be developed and screenshotted in a plain
// browser (`npm run dev`, then open http://localhost:5173). Never bundled into production:
// main.tsx only imports it when running outside Tauri in dev mode.
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type { AdapterInfo, DownloadTask, Settings } from '../types';
import { NETWORK_ADAPTERS_CHANGED_EVENT, PROGRESS_EVENT } from '../types';

const MB = 1024 * 1024;
const adapters: AdapterInfo[] = [
  { id: 'Ethernet:192.168.1.24', name: 'Ethernet', ip: '192.168.1.24', is_ipv4: true, is_loopback: false, enabled: true, usable: true, kind: 'ethernet' },
  { id: 'Wi-Fi:192.168.0.105', name: 'Wi-Fi', ip: '192.168.0.105', is_ipv4: true, is_loopback: false, enabled: true, usable: true, kind: 'wifi' },
  { id: 'Cellular:10.44.2.9', name: 'Cellular', ip: '10.44.2.9', is_ipv4: true, is_loopback: false, enabled: true, usable: true, kind: 'cellular' },
  { id: 'vEthernet (WSL):172.24.0.1', name: 'vEthernet (WSL)', ip: '172.24.0.1', is_ipv4: true, is_loopback: false, enabled: false, usable: true, kind: 'virtual' },
  { id: 'Ethernet:fe80::4022:7688:97ea:1', name: 'Ethernet', ip: 'fe80::4022:7688:97ea:1', is_ipv4: false, is_loopback: false, enabled: false, usable: false, kind: 'ethernet' },
  { id: 'Loopback:127.0.0.1', name: 'Loopback Pseudo-Interface 1', ip: '127.0.0.1', is_ipv4: true, is_loopback: true, enabled: false, usable: false, kind: 'loopback' },
];
const speeds = [9.5 * MB, 6.2 * MB, 3.1 * MB];
// Mirrors crates/conflux-desktop/src/settings.rs.
const MIN_CONNECTIONS = 1;
const MAX_CONNECTIONS = 16;
const MIN_CHUNK_MB = 1;
const MAX_CHUNK_MB = 64;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, Math.round(v)));
const NO_ADAPTERS_ERROR = 'No network adapters are enabled. Enable one on the Network page.';
// Discovery defaults, before user overrides.
const discoveredEnabled = new Map(adapters.map((a) => [a.id, a.enabled]));
/** Interface name: the adapter id without its `:<ip>` suffix. */
const ifaceName = (a: AdapterInfo) => a.id.slice(0, a.id.length - a.ip.length - 1);

let settings: Settings = {
  theme: 'system',
  default_save_dir: null,
  connections_per_adapter: 4,
  chunk_size_mb: 4,
  notify_on_complete: true,
  close_to_tray: true,
  auto_aggregate_adapters: true,
  adapter_overrides: {},
};

/** Like the backend's apply_overrides: unusable adapters are never enabled. */
function applyOverrides() {
  const overrides = settings.adapter_overrides ?? {};
  for (const a of adapters) {
    a.enabled = a.usable && (overrides[a.id] ?? overrides[ifaceName(a)] ?? discoveredEnabled.get(a.id) ?? false);
  }
}
const adapterCopies = () => adapters.map((a) => ({ ...a }));
const requireEnabledAdapter = () => {
  if (!adapters.some((a) => a.enabled)) throw NO_ADAPTERS_ERROR;
};

const now = Date.now();
const base = (id: string, filename: string, size: number, over: Partial<DownloadTask>): DownloadTask => ({
  id,
  url: `https://mirror.example.com/pub/${filename}`,
  filename,
  save_dir: 'C:\\Users\\pc\\Downloads',
  save_path: `C:\\Users\\pc\\Downloads\\${filename}`,
  adapter_ids: adapters.slice(0, 3).map((a) => a.id),
  total_bytes: Math.floor(size),
  supports_ranges: true,
  downloaded_bytes: 0,
  status: 'paused',
  speed_bytes_sec: 0,
  eta_seconds: 0,
  active_chunks: 0,
  completed_chunks: 0,
  total_chunks: Math.ceil(size / (4 * MB)),
  sha256: null,
  error: null,
  created_at_ms: now,
  completed_at_ms: null,
  adapters: [],
  chunk_map: null,
  ...over,
  ...(over.downloaded_bytes !== undefined && { downloaded_bytes: Math.floor(over.downloaded_bytes) }),
});

const tasks: DownloadTask[] = [
  base('t1', 'ubuntu-26.04.1-desktop-amd64.iso', 6.2 * 1024 * MB, { status: 'downloading', created_at_ms: now - 60_000 }),
  base('t2', 'Fedora-Workstation-Live-x86_64-43.iso', 2.3 * 1024 * MB, { status: 'paused', downloaded_bytes: 1.1 * 1024 * MB, completed_chunks: 281, created_at_ms: now - 3_600_000 }),
  base('t3', 'blender-4.5.2-windows-x64.msi', 402 * MB, { status: 'completed', downloaded_bytes: 402 * MB, completed_chunks: 101, sha256: '9f2c1d0b7e4a53f8c6de21b04a7f9e3c55d18a2b6f0c4e79d3a1b85f60e2c7d4', created_at_ms: now - 86_400_000, completed_at_ms: now - 86_000_000 }),
  base('t4', 'dataset-2026-q3.tar.zst', 12.8 * 1024 * MB, { status: 'error', downloaded_bytes: 3.4 * 1024 * MB, completed_chunks: 850, error: 'chunk 871 range [3653238784, 3657433087] via 10.44.2.9: error reading chunk body (stall timeout or disconnect) (received 1048576 of 4194304 bytes)', created_at_ms: now - 7_200_000 }),
  base('t5', 'podcast-episode-212.mp3', 88 * MB, { status: 'completed', downloaded_bytes: 88 * MB, supports_ranges: false, total_chunks: 1, completed_chunks: 1, sha256: '1c0e8b2f4d6a9e7b3c5f1a0d2e4b6c8a9f7e5d3c1b0a2e4f6d8c0b2a4e6f8d0c', created_at_ms: now - 172_800_000, completed_at_ms: now - 172_700_000 }),
];
// Seed the running task's chunk state.
const map = (t: DownloadTask, done: number) =>
  Array.from({ length: t.total_chunks }, (_, i) => (i < done ? '#' : '.')).join('');
tasks[1].chunk_map = map(tasks[1], tasks[1].completed_chunks);
tasks[3].chunk_map = map(tasks[3], 850).replace(/^(.{871})./, '$1!');
tasks[2].chunk_map = map(tasks[2], tasks[2].total_chunks);

function tick() {
  for (const t of tasks) {
    if (t.status !== 'downloading') continue;
    const jitter = () => 0.8 + Math.random() * 0.4;
    t.adapters = adapters.slice(0, 3).map((a, i) => {
      const prev = t.adapters[i]?.downloaded_bytes ?? 0;
      const speed = speeds[i] * jitter();
      return {
        adapter_id: a.id,
        name: a.name,
        ip: a.ip,
        downloaded_bytes: Math.floor(prev + speed / 5),
        speed_bytes_sec: speed,
        active_connections: settings.connections_per_adapter,
        dropped: false,
      };
    });
    t.speed_bytes_sec = t.adapters.reduce((s, a) => s + a.speed_bytes_sec, 0);
    t.downloaded_bytes = Math.min(t.total_bytes, Math.floor(t.downloaded_bytes + t.speed_bytes_sec / 5));
    t.eta_seconds = Math.ceil((t.total_bytes - t.downloaded_bytes) / t.speed_bytes_sec);
    t.completed_chunks = Math.floor(t.downloaded_bytes / (4 * MB));
    t.active_chunks = 12;
    t.chunk_map = Array.from({ length: t.total_chunks }, (_, i) =>
      i < t.completed_chunks ? '#' : i < t.completed_chunks + 12 ? '>' : '.'
    ).join('');
    if (t.downloaded_bytes >= t.total_bytes) {
      Object.assign(t, { status: 'completed', speed_bytes_sec: 0, eta_seconds: 0, active_chunks: 0, adapters: [], completed_at_ms: Date.now() });
    }
    emit(PROGRESS_EVENT, t);
  }
}

export function installMockBackend() {
  mockWindows('main');
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      const find = () => {
        const t = tasks.find((x) => x.id === args.taskId);
        if (!t) throw `Unknown task: ${String(args.taskId)}`;
        return t;
      };
      switch (cmd) {
        case 'discover_adapters':
          applyOverrides();
          return adapterCopies();
        case 'set_adapter_enabled': {
          const target = adapters.find((a) => a.id === String(args.id));
          if (!target) throw `Unknown adapter: ${String(args.id)}`;
          settings = {
            ...settings,
            adapter_overrides: { ...settings.adapter_overrides, [ifaceName(target)]: Boolean(args.enabled) },
          };
          applyOverrides();
          emit(NETWORK_ADAPTERS_CHANGED_EVENT, adapterCopies());
          return adapterCopies();
        }
        case 'list_tasks':
          return tasks.map((t) => ({ ...t }));
        case 'get_settings':
          return { ...settings, adapter_overrides: { ...settings.adapter_overrides } };
        case 'update_settings': {
          const next = args.settings as Settings;
          const dir = next.default_save_dir?.trim();
          // Adapter overrides are owned by set_adapter_enabled; the UI's copy is ignored.
          settings = {
            ...next,
            connections_per_adapter: clamp(next.connections_per_adapter, MIN_CONNECTIONS, MAX_CONNECTIONS),
            chunk_size_mb: clamp(next.chunk_size_mb, MIN_CHUNK_MB, MAX_CHUNK_MB),
            default_save_dir: dir ? dir : null,
            adapter_overrides: settings.adapter_overrides,
          };
          return { ...settings, adapter_overrides: { ...settings.adapter_overrides } };
        }
        case 'apply_window_theme':
          return { mica: false };
        case 'probe_url': {
          const url = String(args.url);
          const name = decodeURIComponent(url.split('/').pop() || 'download.bin');
          return new Promise((r) => setTimeout(() => r({ url, filename: name, total_bytes: 1.4 * 1024 * MB, supports_ranges: true }), 400));
        }
        case 'start_download': {
          requireEnabledAdapter();
          const name = (args.filename as string) || String(args.url).split('/').pop() || 'download.bin';
          const t = base(`t${Date.now()}`, name, 1.4 * 1024 * MB, { status: 'downloading', url: String(args.url), created_at_ms: Date.now() });
          tasks.unshift(t);
          emit(PROGRESS_EVENT, t);
          return { ...t };
        }
        case 'pause_download': {
          const t = find();
          Object.assign(t, { status: 'paused', speed_bytes_sec: 0, eta_seconds: 0, active_chunks: 0, adapters: [] });
          t.chunk_map = t.chunk_map?.replace(/>/g, '.') ?? null;
          emit(PROGRESS_EVENT, t);
          return { ...t };
        }
        case 'resume_download': {
          const t = find();
          if (t.status === 'completed') throw 'Download is already complete';
          requireEnabledAdapter();
          Object.assign(t, { status: 'downloading', error: null, speed_bytes_sec: 0, eta_seconds: 0, active_chunks: 0, adapters: [] });
          t.chunk_map = t.chunk_map?.replace(/[>!]/g, '.') ?? null;
          emit(PROGRESS_EVENT, t);
          return { ...t };
        }
        case 'remove_download': {
          const idx = tasks.findIndex((t) => t.id === args.taskId);
          // Like the backend, removing an unknown task is a no-op.
          if (idx !== -1) tasks.splice(idx, 1);
          return null;
        }
        case 'open_file':
        case 'reveal_file':
          return null;
        case 'plugin:path|resolve_directory':
          return 'C:\\Users\\pc\\Downloads';
        case 'plugin:app|version':
          return '0.1.0';
        case 'plugin:window|is_maximized':
          return false;
        default:
          console.debug('[mock] unhandled command', cmd, payload);
          return null;
      }
    },
    { shouldMockEvents: true }
  );
  window.setInterval(tick, 200);
}
