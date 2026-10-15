import React, { useEffect, useRef, useState } from 'react';
import { makeStyles, mergeClasses, tokens } from '@fluentui/react-components';
import { formatBytes } from '../utils/formatters';

const CELL = 8;
const GAP = 2;
const MAX_ROWS = 10;

export interface ChunkMapColors {
  done: string;
  active: string;
  failed: string;
  pending: string;
}

const useStyles = makeStyles({
  canvas: { display: 'block', width: '100%' },
  legend: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '4px 14px',
    marginTop: '8px',
    fontSize: tokens.fontSizeBase200,
    color: tokens.colorNeutralForeground2,
  },
  swatch: {
    display: 'inline-block',
    width: '10px',
    height: '10px',
    borderRadius: '2px',
    marginRight: '6px',
    verticalAlign: '-1px',
  },
  compactLegend: {
    gap: '4px 12px',
    marginTop: '12px',
    fontSize: '10px',
    lineHeight: '16px',
  },
  compactSwatch: { width: '7px', height: '7px', borderRadius: '1px', marginRight: '6px', verticalAlign: '0' },
  scale: { marginLeft: 'auto' },
});

/** Per cell: which color to paint. Cells cover `bucket` chunks each when there are too many to show 1:1. */
function cellColor(slice: string, colors: ChunkMapColors): { color: string; alpha: number } {
  let done = 0;
  for (const c of slice) {
    if (c === '!') return { color: colors.failed, alpha: 1 };
    if (c === '>') return { color: colors.active, alpha: 1 };
    if (c === '#') done++;
  }
  if (done === 0) return { color: colors.pending, alpha: 1 };
  // Partially completed bucket: fade the "done" color by its completed fraction.
  return { color: colors.done, alpha: 0.35 + 0.65 * (done / slice.length) };
}

/**
 * Canvas chunk map. One cell per chunk when it fits in MAX_ROWS rows; otherwise each cell
 * aggregates a contiguous run of chunks (active/failed win, else shaded by completion).
 */
export const ChunkMap: React.FC<{ map: string; colors: ChunkMapColors; compact?: boolean; totalBytes?: number }> = ({ map, colors, compact = false, totalBytes }) => {
  const styles = useStyles();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => setWidth(Math.floor(entry.contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const cellWidth = compact ? 6 : CELL;
  const cellHeight = compact ? 8 : CELL;
  const gap = compact ? 3 : GAP;
  const maxRows = compact ? 5 : MAX_ROWS;
  const cols = Math.max(1, Math.floor((width + gap) / (cellWidth + gap)));
  const bucket = Math.max(1, Math.ceil(map.length / (cols * maxRows)));
  const cells = Math.ceil(map.length / bucket);
  const rows = Math.max(1, Math.ceil(cells / cols));
  const height = rows * (cellHeight + gap) - gap;
  const active = map.split('>').length - 1;
  const swatch = mergeClasses(styles.swatch, compact && styles.compactSwatch);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    for (let i = 0; i < cells; i++) {
      const { color, alpha } = cellColor(map.slice(i * bucket, (i + 1) * bucket), colors);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = color;
      const x = (i % cols) * (cellWidth + gap);
      const y = Math.floor(i / cols) * (cellHeight + gap);
      ctx.beginPath();
      ctx.roundRect(x, y, cellWidth, cellHeight, compact ? 0 : 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }, [map, colors, width, height, cells, cols, bucket, cellWidth, cellHeight, gap, compact]);

  return (
    <div>
      <canvas
        ref={canvasRef}
        className={styles.canvas}
        style={{ height }}
        role="img"
        aria-label={`Chunk map: ${map.split('#').length - 1} of ${map.length} chunks complete`}
      />
      <div className={mergeClasses(styles.legend, compact && styles.compactLegend)}>
        <span>
          <span className={swatch} style={{ background: colors.done }} />
          Done
        </span>
        <span>
          <span className={swatch} style={{ background: colors.active }} />
          Downloading{compact && ` · ${active} active`}
        </span>
        <span>
          <span className={swatch} style={{ background: colors.pending }} />
          Pending
        </span>
        {map.includes('!') && (
          <span>
            <span className={swatch} style={{ background: colors.failed }} />
            Failed
          </span>
        )}
        {(compact || bucket > 1) && <span className={compact ? styles.scale : undefined}>1 cell = {bucket} chunk{bucket === 1 ? '' : 's'}{compact && totalBytes != null && totalBytes > 0 && map.length > 0 ? ` · ≈${formatBytes(bucket * totalBytes / map.length)}` : ''}</span>}
      </div>
    </div>
  );
};
