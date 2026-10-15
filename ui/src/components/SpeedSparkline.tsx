import React, { useEffect, useId, useMemo, useRef, useState } from 'react';
import { buildSmoothPath, dampCeiling, normalizeSpeedSamples, speedWindowStats } from '../utils/sparkline';
import './SpeedSparkline.css';

/** Animate only a changed scale; paused graphs do no frame-by-frame work. */
function useDampedCeiling(target: number): number {
  const [ceiling, setCeiling] = useState(target);
  const current = useRef(target);

  useEffect(() => {
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    let frame = 0;
    let previous = performance.now();
    const finish = () => {
      current.current = target;
      setCeiling(target);
    };
    const step = (now: number) => {
      const next = dampCeiling(current.current, target, now - previous);
      previous = now;
      if (Math.abs(next - target) <= target * 0.001) {
        finish();
        return;
      }
      current.current = next;
      setCeiling(next);
      frame = window.requestAnimationFrame(step);
    };
    const onMotionChange = () => {
      if (reducedMotion.matches) {
        window.cancelAnimationFrame(frame);
        finish();
      }
    };
    if (current.current !== target) {
      frame = window.requestAnimationFrame(reducedMotion.matches ? finish : step);
    }
    reducedMotion.addEventListener('change', onMotionChange);
    return () => {
      window.cancelAnimationFrame(frame);
      reducedMotion.removeEventListener('change', onMotionChange);
    };
  }, [target]);

  return ceiling;
}

export const SpeedSparkline: React.FC<{
  samples: number[];
  color: string;
  height?: number;
  paused?: boolean;
  active?: boolean;
}> = ({ samples, color, height = 58, paused = false, active = false }) => {
  const gradientId = useId();
  const stats = useMemo(() => speedWindowStats(samples), [samples]);
  const ceiling = useDampedCeiling(stats.ceiling);
  const values = useMemo(() => normalizeSpeedSamples(samples), [samples]);
  const width = 280;
  const top = 4;
  const baseline = height - 2;
  const y = (speed: number) => baseline - Math.min(1, Math.max(0, speed / ceiling)) * (baseline - top);
  const points: [number, number][] = values.map((value, i) => [i / (values.length - 1) * width, y(value)]);
  const line = buildSmoothPath(points, baseline, top);
  const area = `${line} L ${width},${baseline} L 0,${baseline} Z`;
  const accent = paused ? '#F59E0B' : color;

  return (
    <div className="cfx-speed-sparkline" style={{ height, color: accent }}>
      <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden="true">
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="currentColor" stopOpacity={0.28} />
            <stop offset="100%" stopColor="currentColor" stopOpacity={0.02} />
          </linearGradient>
        </defs>
        {stats.average > 0 && <line className="cfx-speed-sparkline-average" x1={0} x2={width} y1={y(stats.average)} y2={y(stats.average)} vectorEffect="non-scaling-stroke" />}
        <path d={area} fill={`url(#${gradientId})`} />
        <path className="cfx-speed-sparkline-curve" d={line} vectorEffect="non-scaling-stroke" />
      </svg>
      {active && !paused && values[values.length - 1] > 0 && <span className="cfx-speed-sparkline-live" style={{ top: y(values[values.length - 1]) }} aria-hidden="true" />}
    </div>
  );
};
