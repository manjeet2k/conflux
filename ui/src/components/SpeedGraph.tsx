import React from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';
import { formatSpeed } from '../utils/formatters';

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

/** Smallest 1/2/5 x 10^n MB/s (at least 1 MB/s) >= v, so axis labels are round numbers. */
function niceCeil(bytesPerSec: number): number {
  const mb = Math.max(1, bytesPerSec / MIB);
  const exp = Math.pow(10, Math.floor(Math.log10(mb)));
  const f = mb / exp;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * exp * MIB;
}

/**
 * Rolling throughput chart: `area` is drawn filled (the aggregate), `lines` on top (per adapter).
 * Values are right-aligned to `capacity` samples so the chart scrolls in from the right.
 */
export const SpeedGraph: React.FC<{ area: Series; lines: Series[]; capacity: number; height?: number }> = ({
  area,
  lines,
  capacity,
  height = 120,
}) => {
  const styles = useStyles();
  const W = 600;
  const H = height;
  const max = niceCeil(Math.max(...area.values, ...lines.flatMap((l) => l.values), 1));
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
        {formatSpeed(max)}
      </span>
      <span className={styles.label} style={{ top: H / 2 - 8 }}>
        {formatSpeed(max / 2)}
      </span>
    </div>
  );
};
