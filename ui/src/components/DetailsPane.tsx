import React from 'react';
import {
  Button, Caption1, Menu, MenuItem, MenuList, MenuPopover, MenuTrigger,
  MessageBar, MessageBarBody, Subtitle2, Text, Tooltip, tokens,
} from '@fluentui/react-components';
import {
  Copy16Regular, Dismiss16Regular, FolderOpen20Regular, MoreHorizontal20Regular,
  Open20Regular, Pause20Regular, Play20Regular, ShieldCheckmark16Regular,
} from '@fluentui/react-icons';
import type { AdapterInfo, DownloadTask } from '../types';
import { HISTORY_SECONDS } from '../hooks/useSpeedHistory';
import { formatBytes, formatDate, formatDuration, formatEta, formatSpeed, progressOf } from '../utils/formatters';
import { fileKind, fileTypeLabels } from '../utils/fileTypes';
import { adapterColor, adapterProblem } from '../utils/adapters';
import { speedWindowStats } from '../utils/sparkline';
import { FileIcon } from './FileIcon';
import { StatusLabel } from './StatusLabel';
import { DownloadProgress } from './DownloadProgress';
import { ChunkMap } from './ChunkMap';
import type { ChunkMapColors } from './ChunkMap';
import { SpeedGraph } from './SpeedGraph';
import './DetailsPane.css';

interface DetailsPaneProps {
  selection: DownloadTask[];
  adapters: AdapterInfo[];
  speedHistory: Record<string, number[]>;
  dark: boolean;
  chunkColors: ChunkMapColors;
  onResume: (ids: string[]) => void;
  onPause: (ids: string[]) => void;
  onOpen: (task: DownloadTask) => void;
  onReveal: (task: DownloadTask) => void;
  onCopy: (text: string, what: string) => void;
  onClose?: () => void;
}

export const DetailsPane: React.FC<DetailsPaneProps> = (p) => {
  const close = p.onClose && (
    <Tooltip content="Close details" relationship="label">
      <Button appearance="subtle" icon={<Dismiss16Regular />} onClick={p.onClose} size="small" aria-label="Close details" />
    </Tooltip>
  );

  if (p.selection.length !== 1) {
    const n = p.selection.length;
    const total = p.selection.reduce((s, t) => s + t.total_bytes, 0);
    const done = p.selection.reduce((s, t) => s + t.downloaded_bytes, 0);
    return (
      <aside className="cfx-details cfx-details-empty" aria-label="Download details">
        <div className="cfx-details-header">
          <div className="cfx-details-identity">
            <FileIcon filename="" className="cfx-details-icon" />
            <div className="cfx-details-file">
              <Subtitle2>{n === 0 ? 'No selection' : `${n} items selected`}</Subtitle2>
              <Caption1 className="cfx-details-muted">
                {n === 0 ? 'Select a download to inspect its progress and connections.' : `${formatBytes(done)} of ${formatBytes(total)} downloaded`}
              </Caption1>
            </div>
          </div>
          {close}
        </div>
        <Caption1 className="cfx-details-empty-message cfx-details-muted">
          {n === 0 ? 'Live transfer rates, chunks, and file details appear here.' : 'Use the toolbar above for actions on the selected downloads.'}
        </Caption1>
      </aside>
    );
  }

  const t = p.selection[0];
  const fraction = progressOf(t.downloaded_bytes, t.total_bytes);
  const running = t.status === 'downloading';
  const resumable = t.status === 'paused' || t.status === 'error';
  const speeds = p.speedHistory[t.id] ?? [];
  const { peak, average } = speedWindowStats(speeds);
  const totalAdapterSpeed = t.adapters.reduce((s, a) => s + a.speed_bytes_sec, 0);
  const received = t.adapters.reduce((s, a) => s + a.downloaded_bytes, 0);
  const streamCount = t.adapters.reduce((s, a) => s + a.active_connections, 0);
  const adapterCount = t.adapters.filter((a) => !a.dropped).length;
  let source = t.url;
  try { source = new URL(t.url).hostname; } catch { /* Keep an invalid stored URL readable. */ }

  const prop = (label: string, value: string, copy?: string) => (
    <div className="cfx-details-property" key={label}>
      <Caption1 className="cfx-details-muted">{label}</Caption1>
      <div className="cfx-details-property-value">
        <Text size={200} truncate wrap={false} title={value}>{value}</Text>
        {copy && (
          <Tooltip content={`Copy ${label.toLowerCase()}`} relationship="label">
            <Button size="small" appearance="subtle" icon={<Copy16Regular />} aria-label={`Copy ${label.toLowerCase()}`} onClick={() => p.onCopy(copy, label)} />
          </Tooltip>
        )}
      </div>
    </div>
  );

  return (
    <aside className="cfx-details" aria-label="Download details">
      <div className="cfx-details-header">
        <div className="cfx-details-identity">
          <FileIcon filename={t.filename} className="cfx-details-icon" />
          <div className="cfx-details-file">
            <div className="cfx-details-title-row">
              <Subtitle2 as="h2" className="cfx-details-title" title={t.filename}>{t.filename}</Subtitle2>
              <span className="cfx-details-status"><StatusLabel status={t.status} /></span>
            </div>
            <Caption1 className="cfx-details-subtitle cfx-details-muted" title={source}>
              {fileTypeLabels[fileKind(t.filename)]}{t.total_bytes > 0 && ` · ${formatBytes(t.total_bytes)}`} · {source}
            </Caption1>
          </div>
        </div>
        <div className="cfx-details-actions">
          {running && <Button icon={<Pause20Regular />} onClick={() => p.onPause([t.id])} size="small">Pause</Button>}
          {resumable && <Button appearance="primary" icon={<Play20Regular />} onClick={() => p.onResume([t.id])} size="small">{t.status === 'error' ? 'Retry' : 'Resume'}</Button>}
          {t.status === 'completed' && <Button appearance="primary" icon={<Open20Regular />} onClick={() => p.onOpen(t)} size="small">Open</Button>}
          <Button icon={<FolderOpen20Regular />} onClick={() => p.onReveal(t)} size="small">Show in folder</Button>
          <Menu>
            <MenuTrigger disableButtonEnhancement><Button appearance="subtle" icon={<MoreHorizontal20Regular />} size="small" aria-label="More download actions" /></MenuTrigger>
            <MenuPopover><MenuList>
              <MenuItem icon={<Copy16Regular />} onClick={() => p.onCopy(t.url, 'Address')}>Copy download address</MenuItem>
              <MenuItem icon={<Copy16Regular />} onClick={() => p.onCopy(t.save_path, 'Saved to')}>Copy file path</MenuItem>
              {t.sha256 && <MenuItem icon={<Copy16Regular />} onClick={() => p.onCopy(t.sha256!, 'SHA-256')}>Copy SHA-256</MenuItem>}
            </MenuList></MenuPopover>
          </Menu>
          {p.onClose && <span className="cfx-details-close">{close}</span>}
        </div>
      </div>

      {t.error && <MessageBar intent="error" layout="multiline" className="cfx-details-error"><MessageBarBody>{t.error}</MessageBarBody></MessageBar>}

      <section className="cfx-details-card cfx-details-transfer" aria-label="Transfer status">
        <div className="cfx-details-progress">
          <div className="cfx-details-progress-header">
            <div className="cfx-details-completion">
              <span className="cfx-details-percent">{t.total_bytes > 0 ? `${(fraction * 100).toFixed(1)}%` : formatBytes(t.downloaded_bytes)}</span>
              <Caption1>{t.total_bytes > 0 ? `${formatBytes(t.downloaded_bytes)} of ${formatBytes(t.total_bytes)}` : 'Size unknown'}</Caption1>
            </div>
            <div className="cfx-details-metric">
              <Caption1 className="cfx-details-muted">{running ? 'Time left' : t.status === 'completed' ? 'Time taken' : 'Elapsed'}</Caption1>
              <strong>{running ? formatEta(t.eta_seconds) : formatDuration(t.duration_ms)}</strong>
            </div>
            <div className="cfx-details-metric">
              <Caption1 className="cfx-details-muted">Chunks</Caption1>
              <strong>{t.total_chunks > 0 ? `${t.completed_chunks} / ${t.total_chunks}` : '—'}</strong>
            </div>
          </div>
          {t.chunk_map ? <ChunkMap map={t.chunk_map} colors={p.chunkColors} compact totalBytes={t.total_bytes} /> : (
            <div className="cfx-details-no-chunks">
              <DownloadProgress status={t.status} fraction={fraction} indeterminate={running && t.total_bytes === 0} size="large" />
              <Caption1 className="cfx-details-muted">{t.supports_ranges ? 'The chunk map appears when transfer data is available.' : 'Single stream · server does not support ranges'}</Caption1>
            </div>
          )}
        </div>
        <div className="cfx-details-speed" role="group" aria-label="Download speed history, last 60 seconds of active transfer" style={{ '--cfx-sparkline-muted': p.dark ? '#8A99AD' : '#596779' } as React.CSSProperties}>
          <div className="cfx-details-speed-header">
            <span><Caption1 className="cfx-details-muted">Speed</Caption1> <strong>{formatSpeed(running ? t.speed_bytes_sec : 0)}</strong></span>
            <Caption1 className="cfx-details-speed-stats" title="Peak and average of the last 60 one-second transfer samples; history is held while paused">
              Peak: {formatSpeed(peak)} • Avg: {formatSpeed(average)}
            </Caption1>
          </div>
          <SpeedGraph area={{ key: t.id, color: p.dark ? '#38BDF8' : '#007EBD', values: speeds }} lines={[]} capacity={HISTORY_SECONDS} height={60} compact paused={t.status === 'paused'} active={running} />
          <div className="cfx-details-speed-axis cfx-details-muted"><span>Last 60 s of transfer</span><span>{running ? 'now' : 'history held'}</span></div>
        </div>
      </section>

      <div className="cfx-details-bottom">
        <section className="cfx-details-card cfx-details-connections" aria-label="Connections">
          <div className="cfx-details-card-heading">
            <h3>Connections</h3>
            <Caption1 className="cfx-details-muted">{adapterCount} adapter{adapterCount === 1 ? '' : 's'}{adapterCount > 1 ? ' bonded' : ''} · {streamCount} stream{streamCount === 1 ? '' : 's'}</Caption1>
          </div>
          <div className="cfx-details-bond" aria-hidden="true" title={totalAdapterSpeed > 0 ? 'Share of current download speed' : 'Share of received data'}>
            {t.adapters.map((a) => <span key={a.adapter_id ?? 'default'} style={{ width: `${(totalAdapterSpeed > 0 ? a.speed_bytes_sec / totalAdapterSpeed : received > 0 ? a.downloaded_bytes / received : 0) * 100}%`, background: adapterColor(a.adapter_id, p.adapters, p.dark) }} />)}
          </div>
          {t.adapters.length === 0 && <Caption1 className="cfx-details-muted cfx-details-connections-empty">{running ? 'Establishing connections…' : 'No active connections for this download.'}</Caption1>}
          <div className="cfx-details-adapters">
            {t.adapters.map((a) => {
              const problem = adapterProblem(a);
              return (
                <div className="cfx-details-adapter" key={a.adapter_id ?? 'default'}>
                  <div className="cfx-details-adapter-row">
                    <span className="cfx-details-adapter-name" title={a.name}><span className="cfx-details-dot" style={{ background: adapterColor(a.adapter_id, p.adapters, p.dark) }} /><strong>{a.name}</strong></span>
                    <span className="cfx-details-adapter-ip cfx-details-muted" title={a.ip ?? 'Default route'}>{a.ip ?? 'Default route'}</span>
                    <span>{a.active_connections} stream{a.active_connections === 1 ? '' : 's'}</span>
                    <span title="Received bytes, including retries">{formatBytes(a.downloaded_bytes)}</span>
                    <strong>{a.dropped ? 'Dropped' : formatSpeed(a.speed_bytes_sec)}</strong>
                  </div>
                  {problem && <Caption1 role="status" className="cfx-details-adapter-problem" style={{ color: tokens.colorPaletteRedForeground1 }}>{problem}</Caption1>}
                </div>
              );
            })}
          </div>
        </section>

        <section className="cfx-details-card cfx-details-properties" aria-label="File details">
          <div className="cfx-details-card-heading"><h3>Details</h3>{t.sha256 && <span className="cfx-details-verified" style={{ color: tokens.colorPaletteGreenForeground1 }}><ShieldCheckmark16Regular />Checksum calculated</span>}</div>
          {prop('Address', t.url, t.url)}
          {prop('Saved to', t.save_path, t.save_path)}
          {prop('Added', formatDate(t.created_at_ms))}
          {prop('Type', `${fileTypeLabels[fileKind(t.filename)]}${t.total_bytes > 0 ? ` · ${formatBytes(t.total_bytes)}` : ''}`)}
          {t.completed_at_ms != null && prop('Completed', formatDate(t.completed_at_ms))}
          {t.sha256 && prop('SHA-256', t.sha256, t.sha256)}
        </section>
      </div>
    </aside>
  );
};
