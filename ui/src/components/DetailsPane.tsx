import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
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
import type { SpeedUnit } from '../utils/formatters';
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
  speedUnit: SpeedUnit;
  task: DownloadTask;
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
  const pane = useRef<HTMLElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const drag = useRef<{ y: number; height: number } | null>(null);
  const [height, setHeight] = useState(340);
  const [availableHeight, setAvailableHeight] = useState(340);
  const minHeight = Math.min(160, availableHeight);
  const paneHeight = Math.max(minHeight, Math.min(height, availableHeight));

  useLayoutEffect(() => {
    const parent = pane.current?.parentElement;
    if (!parent) return;
    // Leave enough room for the list header and at least two download rows.
    const update = () => setAvailableHeight(Math.max(0, parent.clientHeight - 120));
    update();
    const observer = new ResizeObserver(update);
    observer.observe(parent);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (content.current) content.current.scrollTop = 0;
  }, [p.task.id]);

  const resize = (next: number) => setHeight(Math.max(minHeight, Math.min(next, availableHeight)));
  const close = p.onClose && (
    <Tooltip content="Close details" relationship="label">
      <Button appearance="subtle" icon={<Dismiss16Regular />} onClick={p.onClose} size="small" aria-label="Close details" />
    </Tooltip>
  );

  const t = p.task;
  const fraction = progressOf(t.downloaded_bytes, t.total_bytes);
  const running = t.status === 'downloading';
  const resumable = t.status === 'paused' || t.status === 'error';
  const speeds = p.speedHistory[t.id] ?? [];
  const completed = t.status === 'completed';
  const showChunks = !completed && t.supports_ranges && t.total_chunks > 1;
  // Keep the task's adapter rows when paused, with configured adapters as a fallback
  // for restored tasks that have no runtime snapshot. Only running tasks show metrics.
  const connectionRows = t.adapters.length > 0 ? t.adapters : resumable
    ? p.adapters.filter((a) => a.usable && a.enabled).map((a) => ({
      adapter_id: a.id, name: a.name, ip: a.ip, dropped: false,
      active_connections: 0, speed_bytes_sec: 0, downloaded_bytes: 0,
    })) : [];
  const connections = completed ? [] : running ? connectionRows : connectionRows.map((a) => ({
    ...a, active_connections: 0, speed_bytes_sec: 0, downloaded_bytes: 0,
  }));
  const transferState = running ? 'now' : t.status === 'paused' ? 'paused'
    : t.status === 'stopping' ? 'stopping' : 'stopped';
  const connectionMessage = running ? 'Establishing connections…'
    : t.status === 'paused' ? 'Connections reopen when you resume this download.'
    : t.status === 'stopping' ? 'Connections are closing…' : 'Retry this download to reconnect.';
  const { peak, average } = speedWindowStats(speeds);
  const totalAdapterSpeed = connections.reduce((s, a) => s + a.speed_bytes_sec, 0);
  const received = connections.reduce((s, a) => s + a.downloaded_bytes, 0);
  const streamCount = connections.reduce((s, a) => s + a.active_connections, 0);
  const adapterCount = connections.filter((a) => !a.dropped).length;
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
    <aside ref={pane} className="cfx-details" aria-label="Download details" style={{ height: paneHeight }}>
      <div
        className="cfx-details-resize"
        role="separator"
        tabIndex={0}
        aria-label="Resize download details"
        aria-orientation="horizontal"
        aria-controls="download-details-content"
        aria-valuemin={minHeight}
        aria-valuemax={availableHeight}
        aria-valuenow={paneHeight}
        aria-valuetext={`${Math.round(paneHeight)} pixels high`}
        title="Drag to resize; use Up and Down arrow keys when focused"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.focus();
          event.currentTarget.setPointerCapture(event.pointerId);
          drag.current = { y: event.clientY, height: paneHeight };
        }}
        onPointerMove={(event) => {
          if (drag.current) resize(drag.current.height + drag.current.y - event.clientY);
        }}
        onPointerUp={(event) => {
          drag.current = null;
          if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onLostPointerCapture={() => { drag.current = null; }}
        onPointerCancel={() => { drag.current = null; }}
        onKeyDown={(event) => {
          const next = { ArrowUp: paneHeight + 24, ArrowDown: paneHeight - 24, Home: minHeight, End: availableHeight }[event.key];
          if (next === undefined) return;
          event.preventDefault();
          resize(next);
        }}
      />
      <div ref={content} id="download-details-content" className="cfx-details-content" tabIndex={0} role="region" aria-label={`Details for ${t.filename}`}>
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
              {completed && t.sha256 && <MenuItem icon={<Copy16Regular />} onClick={() => p.onCopy(t.sha256!, 'SHA-256')}>Copy SHA-256</MenuItem>}
            </MenuList></MenuPopover>
          </Menu>
          {p.onClose && <span className="cfx-details-close">{close}</span>}
        </div>
      </div>

      {t.status === 'error' && t.error && <MessageBar intent="error" layout="multiline" className="cfx-details-error"><MessageBarBody>{t.error}</MessageBarBody></MessageBar>}

      {!completed && <section className="cfx-details-card cfx-details-transfer" aria-label="Transfer status">
        <div className="cfx-details-progress">
          <div className="cfx-details-progress-header">
            <div className="cfx-details-completion">
              <span className="cfx-details-percent">{t.total_bytes > 0 ? `${(fraction * 100).toFixed(1)}%` : formatBytes(t.downloaded_bytes)}</span>
              <Caption1>{t.total_bytes > 0 ? `${formatBytes(t.downloaded_bytes)} of ${formatBytes(t.total_bytes)}` : 'Size unknown'}</Caption1>
            </div>
            {(running ? t.eta_seconds > 0 : (t.duration_ms ?? 0) > 0) && <div className="cfx-details-metric">
              <Caption1 className="cfx-details-muted">{running ? 'Time left' : 'Elapsed'}</Caption1>
              <strong>{running ? formatEta(t.eta_seconds) : formatDuration(t.duration_ms)}</strong>
            </div>}
            {showChunks && <div className="cfx-details-metric">
              <Caption1 className="cfx-details-muted">Chunks</Caption1>
              <strong>{t.completed_chunks} / {t.total_chunks}</strong>
            </div>}
          </div>
          {showChunks && t.chunk_map ? <ChunkMap map={t.chunk_map} colors={p.chunkColors} compact totalBytes={t.total_bytes} /> : (
            <div className="cfx-details-no-chunks">
              <DownloadProgress status={t.status} fraction={fraction} indeterminate={running && t.total_bytes === 0} size="large" />
              {running && !t.supports_ranges && <Caption1 className="cfx-details-muted">Single stream · server does not support ranges</Caption1>}
            </div>
          )}
        </div>
        <div className="cfx-details-speed" role="group" aria-label="Download speed history, last 60 seconds of active transfer" style={{ '--cfx-sparkline-muted': p.dark ? '#8A99AD' : '#596779' } as React.CSSProperties}>
          <div className="cfx-details-speed-header">
            <span><Caption1 className="cfx-details-muted">Speed</Caption1> <strong>{running ? formatSpeed(t.speed_bytes_sec, p.speedUnit) : t.status === 'paused' ? 'Paused' : t.status === 'stopping' ? 'Stopping…' : 'Stopped'}</strong></span>
            {speeds.length > 0 && <Caption1 className="cfx-details-speed-stats" title="Peak and average of the last 60 one-second transfer samples">
              Peak: {formatSpeed(peak, p.speedUnit)} • Avg: {formatSpeed(average, p.speedUnit)}
            </Caption1>}
          </div>
          {speeds.length > 0 ? <>
            <SpeedGraph speedUnit={p.speedUnit} area={{ key: t.id, color: p.dark ? '#38BDF8' : '#007EBD', values: speeds }} lines={[]} capacity={HISTORY_SECONDS} height={60} compact active={running} paused={t.status === 'paused'} />
            <div className="cfx-details-speed-axis cfx-details-muted"><span>Last 60 s of transfer</span><span>{transferState}</span></div>
          </> : <Caption1 className="cfx-details-muted">{running ? 'Collecting speed samples…' : 'No speed samples recorded for this download.'}</Caption1>}
        </div>
      </section>}

      <div className={`cfx-details-bottom${completed ? ' cfx-details-single' : ''}`}>
        {!completed && <section className="cfx-details-card cfx-details-connections" aria-label="Connections">
          <div className="cfx-details-card-heading">
            <h3>{t.status === 'error' ? 'Connection issues' : 'Connections'}</h3>
            <Caption1 className="cfx-details-muted">{adapterCount} adapter{adapterCount === 1 ? '' : 's'}{running && adapterCount > 1 ? ' bonded' : ''} · {streamCount} stream{streamCount === 1 ? '' : 's'}</Caption1>
          </div>
          {connections.length === 0 && <Caption1 className="cfx-details-muted">{connectionMessage}</Caption1>}
          {connections.length > 0 && <div className="cfx-details-bond" aria-hidden="true" title={totalAdapterSpeed > 0 ? 'Share of current download speed' : running ? 'Share of received data' : 'Connections inactive'}>
            {connections.map((a) => <span key={a.adapter_id ?? 'default'} style={{ width: `${(totalAdapterSpeed > 0 ? a.speed_bytes_sec / totalAdapterSpeed : received > 0 ? a.downloaded_bytes / received : 0) * 100}%`, background: adapterColor(a.adapter_id, p.adapters, p.dark) }} />)}
          </div>}
          <div className="cfx-details-adapters">
            {connections.map((a) => {
              const problem = adapterProblem(a);
              return (
                <div className="cfx-details-adapter" key={a.adapter_id ?? 'default'}>
                  <div className="cfx-details-adapter-row">
                    <span className="cfx-details-adapter-name" title={a.name}><span className="cfx-details-dot" style={{ background: adapterColor(a.adapter_id, p.adapters, p.dark) }} /><strong>{a.name}</strong></span>
                    <span className="cfx-details-adapter-ip cfx-details-muted" title={a.ip ?? 'Default route'}>{a.ip ?? 'Default route'}</span>
                    <span>{a.active_connections} stream{a.active_connections === 1 ? '' : 's'}</span>
                    <span title="Received bytes, including retries">{formatBytes(a.downloaded_bytes)}</span>
                    <strong>{running && a.dropped ? 'Dropped' : formatSpeed(a.speed_bytes_sec, p.speedUnit)}</strong>
                  </div>
                  {problem && <Caption1 role="status" className="cfx-details-adapter-problem" style={{ color: tokens.colorPaletteRedForeground1 }}>{problem}</Caption1>}
                </div>
              );
            })}
          </div>
        </section>}

        <section className="cfx-details-card cfx-details-properties" aria-label="File details">
          <div className="cfx-details-card-heading"><h3>Details</h3>{completed && t.sha256 && <span className="cfx-details-verified" style={{ color: tokens.colorPaletteGreenForeground1 }}><ShieldCheckmark16Regular />Checksum calculated</span>}</div>
          {prop('Address', t.url, t.url)}
          {prop('Saved to', t.save_path, t.save_path)}
          {prop('Added', formatDate(t.created_at_ms))}
          {prop('Type', `${fileTypeLabels[fileKind(t.filename)]}${t.total_bytes > 0 ? ` · ${formatBytes(t.total_bytes)}` : ''}`)}
          {completed && t.completed_at_ms != null && prop('Completed', formatDate(t.completed_at_ms))}
          {completed && (t.duration_ms ?? 0) > 0 && prop('Time taken', formatDuration(t.duration_ms))}
          {completed && (t.avg_speed_bytes_sec ?? 0) > 0 && prop('Avg. speed', formatSpeed(t.avg_speed_bytes_sec!, p.speedUnit))}
          {completed && t.sha256 && prop('SHA-256', t.sha256, t.sha256)}
        </section>
      </div>
      </div>
    </aside>
  );
};
