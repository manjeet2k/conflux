import React, { useEffect, useRef, useState } from 'react';
import { makeStyles, tokens } from '@fluentui/react-components';

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
export const ChunkMap: React.FC<{ map: string; colors: ChunkMapColors }> = ({ map, colors }) => {
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

  const cols = Math.max(1, Math.floor((width + GAP) / (CELL + GAP)));
  const bucket = Math.max(1, Math.ceil(map.length / (cols * MAX_ROWS)));
  const cells = Math.ceil(map.length / bucket);
  const rows = Math.max(1, Math.ceil(cells / cols));
  const height = rows * (CELL + GAP) - GAP;

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
      const x = (i % cols) * (CELL + GAP);
      const y = Math.floor(i / cols) * (CELL + GAP);
      ctx.beginPath();
      ctx.roundRect(x, y, CELL, CELL, 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }, [map, colors, width, height, cells, cols, bucket]);

  return (
    <div>
      <canvas
        ref={canvasRef}
        className={styles.canvas}
        style={{ height }}
        role="img"
        aria-label={`Chunk map: ${map.split('#').length - 1} of ${map.length} chunks complete`}
      />
      <div className={styles.legend}>
        <span>
          <span className={styles.swatch} style={{ background: colors.done }} />
          Done
        </span>
        <span>
          <span className={styles.swatch} style={{ background: colors.active }} />
          Downloading
        </span>
        <span>
          <span className={styles.swatch} style={{ background: colors.pending }} />
          Pending
        </span>
        {map.includes('!') && (
          <span>
            <span className={styles.swatch} style={{ background: colors.failed }} />
            Failed
          </span>
        )}
        {bucket > 1 && <span>1 cell = {bucket} chunks</span>}
      </div>
    </div>
  );
};
