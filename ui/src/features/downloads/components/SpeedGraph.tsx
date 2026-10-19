// src/features/downloads/components/SpeedGraph.tsx
import React from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';
import { formatSpeed } from '@/utils/formatters';
import type { SpeedUnit } from '@/utils/formatters';
import { SpeedSparkline } from './SpeedSparkline';

export interface Series {
  key: string;
  color: string;
  values: number[];
}

const useStyles = makeStyles({
  root: { position: 'relative' },
  svg: { display: 'block', width: '100%', overflow: 'visible' },
  label: {
    position: 'absolute',
    left: '4px',
    fontSize: tokens.fontSizeBase100,
    color: tokens.colorNeutralForeground3,
    fontVariantNumeric: 'tabular-nums',
  },
});

const MIB = 1024 * 1024;

/** Round the axis up in the selected unit; samples remain in bytes per second. */
function niceCeil(bytesPerSec: number, unit: SpeedUnit): number {
  const scale = unit === 'bits' ? 1_000_000 / 8 : MIB;
  const value = Math.max(1, bytesPerSec / scale);
  const exp = Math.pow(10, Math.floor(Math.log10(value)));
  const f = value / exp;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * exp * scale;
}

/**
 * Rolling throughput chart: `area` is drawn filled (the aggregate), `lines` on top (per adapter).
 * Values are right-aligned to `capacity` samples so the chart scrolls in from the right.
 */
export const SpeedGraph: React.FC<{ speedUnit: SpeedUnit; area: Series; lines: Series[]; capacity: number; height?: number; compact?: boolean; paused?: boolean; active?: boolean }> = ({
  speedUnit,
  area,
  lines,
  capacity,
  height,
  compact = false,
  paused = false,
  active = false,
}) => {
  const styles = useStyles();
  if (compact) return <SpeedSparkline key={area.key} samples={area.values} color={area.color} height={height} paused={paused} active={active} />;
  const W = 600;
  const H = height ?? 120;
  const observedMax = Math.max(...area.values, ...lines.flatMap((l) => l.values), 1);
  const max = niceCeil(observedMax, speedUnit);
  const x = (i: number, n: number) => ((capacity - n + i) / Math.max(1, capacity - 1)) * W;
  const y = (v: number) => H - (v / max) * H;

  const path = (values: number[]) =>
    values.map((v, i) => `${i === 0 ? 'M' : 'L'}${x(i, values.length).toFixed(1)},${y(v).toFixed(1)}`).join('');

  const areaPath =
    area.values.length > 1
      ? `${path(area.values)}L${x(area.values.length - 1, area.values.length)},${H}L${x(0, area.values.length)},${H}Z`
      : '';

  return (
    <div className={styles.root}>
      <svg className={styles.svg} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" style={{ height: H }}>
        {[0.25, 0.5, 0.75].map((f) => (
          <line
            key={f}
            x1={0}
            x2={W}
            y1={H * f}
            y2={H * f}
            stroke="var(--cfx-divider)"
            strokeWidth={1}
            vectorEffect="non-scaling-stroke"
          />
        ))}
        <line x1={0} x2={W} y1={H} y2={H} stroke="var(--cfx-stroke)" vectorEffect="non-scaling-stroke" />
        {areaPath && <path d={areaPath} fill={area.color} fillOpacity={0.18} />}
        {area.values.length > 1 && (
          <path d={path(area.values)} fill="none" stroke={area.color} strokeWidth={2} vectorEffect="non-scaling-stroke" />
        )}
        {lines
          .filter((l) => l.values.length > 1)
          .map((l) => (
            <path
              key={l.key}
              d={path(l.values)}
              fill="none"
              stroke={l.color}
              strokeWidth={1.5}
              strokeDasharray="4 3"
              vectorEffect="non-scaling-stroke"
            />
          ))}
      </svg>
      <span className={styles.label} style={{ top: 0 }}>
        {formatSpeed(max, speedUnit)}
      </span>
      <span className={styles.label} style={{ top: H / 2 - 8 }}>
        {formatSpeed(max / 2, speedUnit)}
      </span>
    </div>
  );
};
