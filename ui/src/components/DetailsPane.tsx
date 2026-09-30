import React from 'react';
import {
  Button,
  Caption1,
  MessageBar,
  MessageBarBody,
  ProgressBar,
  Subtitle2,
  Text,
  Tooltip,
  makeStyles,
  tokens,
} from '@fluentui/react-components';
import {
  Copy16Regular,
  FolderOpen20Regular,
  Open20Regular,
  Pause20Regular,
  Play20Regular,
  ShieldCheckmark16Regular,
} from '@fluentui/react-icons';
import type { AdapterInfo, DownloadTask } from '../types';
import { formatBytes, formatDate, formatEta, formatSpeed, progressOf } from '../utils/formatters';
import { FileIcon } from './FileIcon';
import { adapterColor } from '../utils/adapters';
import { StatusLabel } from './StatusLabel';
import { progressColor } from '../utils/status';
import { ChunkMap } from './ChunkMap';
import type { ChunkMapColors } from './ChunkMap';

const useStyles = makeStyles({
  root: {
    width: '340px',
    flexShrink: 0,
    borderLeft: '1px solid var(--cfx-divider)',
    overflowY: 'auto',
    padding: '16px',
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
    '> *': { flexShrink: 0 },
  },
  header: { display: 'flex', gap: '12px', alignItems: 'flex-start' },
  bigIcon: { fontSize: '40px', color: tokens.colorNeutralForeground2, flexShrink: 0 },
  title: { wordBreak: 'break-word', userSelect: 'text' },
  card: {
    backgroundColor: 'var(--cfx-card)',
    border: '1px solid var(--cfx-stroke)',
    borderRadius: tokens.borderRadiusLarge,
    padding: '12px',
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  cardTitle: { fontWeight: tokens.fontWeightSemibold },
  row: { display: 'flex', justifyContent: 'space-between', gap: '8px', alignItems: 'baseline' },
  muted: { color: tokens.colorNeutralForeground3 },
  num: { fontVariantNumeric: 'tabular-nums' },
  big: {
    fontSize: tokens.fontSizeBase500,
    fontWeight: tokens.fontWeightSemibold,
    fontVariantNumeric: 'tabular-nums',
  },
  pausedBar: { backgroundColor: tokens.colorPaletteMarigoldBackground3 },
  actions: { display: 'flex', gap: '8px', flexWrap: 'wrap' },
  adapter: { display: 'flex', flexDirection: 'column', gap: '4px' },
  dot: { width: '8px', height: '8px', borderRadius: '50%', flexShrink: 0 },
  adapterName: { display: 'flex', alignItems: 'center', gap: '8px', minWidth: 0 },
  share: { height: '4px', borderRadius: '2px', backgroundColor: 'var(--cfx-track)', overflow: 'hidden' },
  shareFill: { height: '100%', borderRadius: '2px', transitionProperty: 'width', transitionDuration: '300ms' },
  prop: { display: 'flex', flexDirection: 'column', gap: '2px' },
  propValue: {
    display: 'flex',
    alignItems: 'flex-start',
    gap: '4px',
    wordBreak: 'break-all',
    userSelect: 'text',
  },
  mono: { fontFamily: tokens.fontFamilyMonospace, fontSize: tokens.fontSizeBase200 },
  empty: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    textAlign: 'center',
    gap: '4px',
    color: tokens.colorNeutralForeground3,
  },
});

interface DetailsPaneProps {
  selection: DownloadTask[];
  adapters: AdapterInfo[];
  dark: boolean;
  chunkColors: ChunkMapColors;
  onResume: (ids: string[]) => void;
  onPause: (ids: string[]) => void;
  onOpen: (task: DownloadTask) => void;
  onReveal: (task: DownloadTask) => void;
  onCopy: (text: string, what: string) => void;
}

export const DetailsPane: React.FC<DetailsPaneProps> = (p) => {
  const styles = useStyles();

  if (p.selection.length !== 1) {
    const n = p.selection.length;
    const total = p.selection.reduce((s, t) => s + t.total_bytes, 0);
    const done = p.selection.reduce((s, t) => s + t.downloaded_bytes, 0);
    return (
      <aside className={styles.root} aria-label="Details">
        <div className={styles.empty}>
          {n === 0 ? (
            <>
              <Subtitle2>No selection</Subtitle2>
              <Caption1>Select a download to see its connections, chunk map and file details.</Caption1>
            </>
          ) : (
            <>
              <Subtitle2>{n} items selected</Subtitle2>
              <Caption1>
                {formatBytes(done)} of {formatBytes(total)} downloaded
              </Caption1>
            </>
          )}
        </div>
      </aside>
    );
  }

  const t = p.selection[0];
  const fraction = progressOf(t.downloaded_bytes, t.total_bytes);
  const running = t.status === 'downloading';
  const resumable = t.status === 'paused' || t.status === 'error';
  const totalAdapterSpeed = t.adapters.reduce((s, a) => s + a.speed_bytes_sec, 0);

  const prop = (label: string, value: React.ReactNode, copy?: string, mono = false) => (
    <div className={styles.prop}>
      <Caption1 className={styles.muted}>{label}</Caption1>
      <div className={styles.propValue}>
        <Text size={200} className={mono ? styles.mono : undefined} style={{ flex: 1 }}>
          {value}
        </Text>
        {copy && (
          <Tooltip content={`Copy ${label.toLowerCase()}`} relationship="label">
            <Button size="small" appearance="subtle" icon={<Copy16Regular />} onClick={() => p.onCopy(copy, label)} />
          </Tooltip>
        )}
      </div>
    </div>
  );

  return (
    <aside className={styles.root} aria-label="Details">
      <div className={styles.header}>
        <FileIcon filename={t.filename} className={styles.bigIcon} />
        <div>
          <Subtitle2 as="h2" className={styles.title} block>
            {t.filename}
          </Subtitle2>
          <StatusLabel status={t.status} />
        </div>
      </div>

      <div className={styles.actions}>
        {running && (
          <Button icon={<Pause20Regular />} onClick={() => p.onPause([t.id])}>
            Pause
          </Button>
        )}
        {resumable && (
          <Button appearance="primary" icon={<Play20Regular />} onClick={() => p.onResume([t.id])}>
            {t.status === 'error' ? 'Retry' : 'Resume'}
          </Button>
        )}
        {t.status === 'completed' && (
          <Button appearance="primary" icon={<Open20Regular />} onClick={() => p.onOpen(t)}>
            Open
          </Button>
        )}
        <Button icon={<FolderOpen20Regular />} onClick={() => p.onReveal(t)}>
          Show in folder
        </Button>
      </div>

      {t.error && (
        <MessageBar intent="error" layout="multiline">
          <MessageBarBody style={{ userSelect: 'text', wordBreak: 'break-word' }}>{t.error}</MessageBarBody>
        </MessageBar>
      )}

      <section className={styles.card}>
        <div className={styles.row}>
          <span className={styles.big}>{t.total_bytes > 0 ? `${(fraction * 100).toFixed(1)}%` : formatBytes(t.downloaded_bytes)}</span>
          {running && <span className={styles.num}>{formatSpeed(t.speed_bytes_sec)}</span>}
        </div>
        <ProgressBar
          value={running && t.total_bytes === 0 ? undefined : fraction}
          color={progressColor(t.status)}
          bar={{ className: t.status === 'paused' ? styles.pausedBar : undefined }}
          thickness="large"
        />
        <div className={styles.row}>
          <Caption1 className={styles.num}>
            {formatBytes(t.downloaded_bytes)} of {t.total_bytes > 0 ? formatBytes(t.total_bytes) : 'unknown size'}
          </Caption1>
          {running && <Caption1 className={styles.muted}>{formatEta(t.eta_seconds)} left</Caption1>}
        </div>
      </section>

      {running && t.adapters.length > 0 && (
        <section className={styles.card} aria-label="Connections">
          <Text className={styles.cardTitle}>Connections</Text>
          {t.adapters.map((a) => {
            const share = totalAdapterSpeed > 0 ? a.speed_bytes_sec / totalAdapterSpeed : 0;
            const color = adapterColor(a.adapter_id, p.adapters, p.dark);
            return (
              <div key={a.adapter_id ?? 'default'} className={styles.adapter}>
                <div className={styles.row}>
                  <span className={styles.adapterName}>
                    <span className={styles.dot} style={{ background: color }} />
                    <Text truncate wrap={false}>
                      {a.name}
                    </Text>
                    <Caption1 className={styles.muted}>{a.ip ?? 'default route'}</Caption1>
                  </span>
                  <Caption1 className={styles.num}>
                    {a.dropped ? 'Dropped' : formatSpeed(a.speed_bytes_sec)}
                  </Caption1>
                </div>
                <div className={styles.share}>
                  <div className={styles.shareFill} style={{ width: `${share * 100}%`, background: color }} />
                </div>
                <Caption1 className={styles.muted}>
                  {a.active_connections} active connection{a.active_connections === 1 ? '' : 's'} ·{' '}
                  {formatBytes(a.downloaded_bytes)} received
                </Caption1>
              </div>
            );
          })}
        </section>
      )}

      <section className={styles.card} aria-label="Chunks">
        <div className={styles.row}>
          <Text className={styles.cardTitle}>Chunks</Text>
          {t.total_chunks > 0 && (
            <Caption1 className={styles.num}>
              {t.completed_chunks} / {t.total_chunks}
              {running ? ` · ${t.active_chunks} active` : ''}
            </Caption1>
          )}
        </div>
        {t.chunk_map ? (
          <ChunkMap map={t.chunk_map} colors={p.chunkColors} />
        ) : (
          <Caption1 className={styles.muted}>
            {t.supports_ranges
              ? 'The chunk map appears once the download is running.'
              : 'This server does not support byte ranges, so the file is downloaded over a single connection and cannot be resumed.'}
          </Caption1>
        )}
      </section>

      <section className={styles.card} aria-label="Properties">
        <Text className={styles.cardTitle}>Properties</Text>
        {prop('Address', t.url, t.url, true)}
        {prop('Saved to', t.save_path, t.save_path, true)}
        {prop('Size', t.total_bytes > 0 ? `${formatBytes(t.total_bytes, 2)} (${t.total_bytes.toLocaleString()} bytes)` : 'Unknown')}
        {prop('Resumable', t.supports_ranges ? 'Yes, the server supports byte ranges' : 'No')}
        {prop('Added', formatDate(t.created_at_ms))}
        {t.completed_at_ms && prop('Completed', formatDate(t.completed_at_ms))}
        {t.sha256 &&
          prop(
            'SHA-256',
            <span style={{ display: 'inline-flex', gap: 4 }}>
              <ShieldCheckmark16Regular style={{ flexShrink: 0, color: tokens.colorPaletteGreenForeground1 }} />
              {t.sha256}
            </span>,
            t.sha256,
            true
          )}
      </section>
    </aside>
  );
};
