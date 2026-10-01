import React from 'react';
import {
  Badge,
  Button,
  Caption1,
  Switch,
  Text,
  makeStyles,
  tokens,
} from '@fluentui/react-components';
import { ArrowClockwise20Regular } from '@fluentui/react-icons';
import type { AdapterInfo, DownloadTask } from '../types';
import type { SpeedSample } from '../hooks/useSpeedHistory';
import { HISTORY_SECONDS, adapterKey } from '../hooks/useSpeedHistory';
import { Page, SectionHeader } from '../components/Page';
import { SpeedGraph } from '../components/SpeedGraph';
import { adapterColor, kindIcon, kindLabel, usableAdapters } from '../utils/adapters';
import { formatBytes, formatSpeed } from '../utils/formatters';

const useStyles = makeStyles({
  card: {
    backgroundColor: 'var(--cfx-card)',
    border: '1px solid var(--cfx-stroke)',
    borderRadius: tokens.borderRadiusLarge,
    padding: '16px',
  },
  graphHeader: { display: 'flex', alignItems: 'flex-end', justifyContent: 'space-between', marginBottom: '12px', gap: '16px' },
  total: { fontSize: '28px', lineHeight: '36px', fontWeight: tokens.fontWeightSemibold, fontVariantNumeric: 'tabular-nums' },
  legend: { display: 'flex', flexWrap: 'wrap', gap: '4px 16px', justifyContent: 'flex-end' },
  legendItem: { display: 'inline-flex', alignItems: 'center', gap: '6px', fontSize: tokens.fontSizeBase200 },
  dot: { width: '8px', height: '8px', borderRadius: '50%', flexShrink: 0 },
  grid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))', gap: '8px' },
  adapterHead: { display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '12px' },
  adapterIcon: { fontSize: '24px', display: 'flex' },
  adapterTitle: { flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' },
  adapterActions: { display: 'flex', alignItems: 'center', gap: '8px', flexShrink: 0 },
  statusBadge: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    minHeight: '22px',
    lineHeight: '18px',
    paddingBlock: '1px',
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', sans-serif",
    textRendering: 'geometricPrecision',
  },
  stats: { display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '8px' },
  stat: { display: 'flex', flexDirection: 'column' },
  statValue: { fontVariantNumeric: 'tabular-nums', fontWeight: tokens.fontWeightSemibold },
  muted: { color: tokens.colorNeutralForeground3 },
});

interface LiveStat {
  speed: number;
  connections: number;
  received: number;
  dropped: boolean;
}

function liveStats(tasks: DownloadTask[]): Map<string, LiveStat> {
  const stats = new Map<string, LiveStat>();
  for (const t of tasks) {
    if (t.status !== 'downloading') continue;
    for (const a of t.adapters) {
      const key = adapterKey(a.adapter_id);
      const s = stats.get(key) ?? { speed: 0, connections: 0, received: 0, dropped: false };
      s.speed += a.speed_bytes_sec;
      s.connections += a.active_connections;
      s.received += a.downloaded_bytes;
      s.dropped ||= a.dropped;
      stats.set(key, s);
    }
  }
  return stats;
}

interface NetworkPageProps {
  adapters: AdapterInfo[];
  tasks: DownloadTask[];
  history: SpeedSample[];
  dark: boolean;
  onRefresh: () => void;
  onToggleAdapter: (id: string, enabled: boolean) => void;
}

export const NetworkPage: React.FC<NetworkPageProps> = ({
  adapters,
  tasks,
  history,
  dark,
  onRefresh,
  onToggleAdapter,
}) => {
  const styles = useStyles();
  // Display only real, ready physical adapters (hide virtual/loopback/link-local)
  const usable = usableAdapters(adapters).filter((a) => a.kind !== 'virtual' && a.kind !== 'loopback');
  const stats = liveStats(tasks);
  const totalNow = history.length > 0 ? history[history.length - 1].total : 0;
  const accent = dark ? '#60CDFF' : '#005FB8';

  const seriesKeys = [
    ...usable.filter((a) => a.enabled).map((a) => a.id),
    ...(stats.has('default') ? ['default'] : []),
  ];
  const lines = seriesKeys.map((key) => ({
    key,
    color: adapterColor(key === 'default' ? null : key, adapters, dark),
    values: history.map((h) => h.byAdapter[key] ?? 0),
  }));
  const showLines = lines.filter((l) => l.values.some((v) => v > 0));

  return (
    <Page
      title="Network"
      actions={
        <Button icon={<ArrowClockwise20Regular />} onClick={onRefresh}>
          Refresh
        </Button>
      }
    >
      <div className={styles.card}>
        <div className={styles.graphHeader}>
          <div>
            <Caption1 className={styles.muted}>Total throughput</Caption1>
            <div className={styles.total}>{formatSpeed(totalNow)}</div>
          </div>
          <div className={styles.legend}>
            <span className={styles.legendItem}>
              <span className={styles.dot} style={{ background: accent }} />
              All active adapters
            </span>
            {showLines.map((l) => (
              <span key={l.key} className={styles.legendItem}>
                <span className={styles.dot} style={{ background: l.color }} />
                {l.key === 'default' ? 'Default route' : (usable.find((a) => a.id === l.key)?.name ?? l.key)}
              </span>
            ))}
          </div>
        </div>
        <SpeedGraph
          area={{ key: 'total', color: accent, values: history.map((h) => h.total) }}
          lines={showLines}
          capacity={HISTORY_SECONDS}
          height={140}
        />
        <Caption1 className={styles.muted}>Last {HISTORY_SECONDS} seconds</Caption1>
      </div>

      <SectionHeader>Adapters</SectionHeader>
      {usable.length === 0 && (
        <Text className={styles.muted}>
          No bindable IPv4 adapters were found. Downloads will use the system's default route.
        </Text>
      )}
      <div className={styles.grid}>
        {usable.map((a) => {
          const Icon = kindIcon[a.kind];
          const s = stats.get(a.id);
          const inUse = !!s && s.connections > 0;
          return (
            <div key={a.id} className={styles.card}>
              <div className={styles.adapterHead}>
                <span className={styles.adapterIcon} style={{ color: adapterColor(a.id, adapters, dark) }}>
                  <Icon />
                </span>
                <div className={styles.adapterTitle}>
                  <Text weight="semibold" truncate wrap={false}>
                    {a.name}
                  </Text>
                  <Caption1 className={styles.muted}>
                    {kindLabel[a.kind]} · {a.ip}
                  </Caption1>
                </div>
                <div className={styles.adapterActions}>
                  {s?.dropped ? (
                    <Badge className={styles.statusBadge} appearance="tint" color="danger">
                      Dropped
                    </Badge>
                  ) : inUse ? (
                    <Badge className={styles.statusBadge} appearance="filled" color="brand">
                      In use
                    </Badge>
                  ) : null}
                  <Switch
                    checked={a.enabled}
                    onChange={(_, d) => onToggleAdapter(a.id, d.checked)}
                    label={a.enabled ? 'Active' : 'Disabled'}
                    labelPosition="before"
                  />
                </div>
              </div>
              <div className={styles.stats}>
                <div className={styles.stat}>
                  <Caption1 className={styles.muted}>Speed</Caption1>
                  <span className={styles.statValue}>{formatSpeed(s?.speed ?? 0)}</span>
                </div>
                <div className={styles.stat}>
                  <Caption1 className={styles.muted}>Connections</Caption1>
                  <span className={styles.statValue}>{s?.connections ?? 0}</span>
                </div>
                <div className={styles.stat}>
                  <Caption1 className={styles.muted}>Received</Caption1>
                  <span className={styles.statValue}>{formatBytes(s?.received ?? 0)}</span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </Page>
  );
};
