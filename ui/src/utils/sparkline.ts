export const SPEED_HISTORY_CAPACITY = 60;

const MINIMUM_SPEED_CEILING = 1024 * 1024;
const cleanSpeed = (speed: number): number => Number.isFinite(speed) ? Math.max(0, speed) : 0;
const clamp = (value: number, minimum: number, maximum: number): number => Math.min(maximum, Math.max(minimum, value));

/** Pad the visible window without adding synthetic samples to its statistics. */
export function normalizeSpeedSamples(samples: readonly number[], capacity = SPEED_HISTORY_CAPACITY): number[] {
  const size = Number.isFinite(capacity) ? Math.max(0, Math.trunc(capacity)) : SPEED_HISTORY_CAPACITY;
  if (size === 0) return [];
  const recent = samples.slice(-size).map(cleanSpeed);
  return [...Array<number>(size - recent.length).fill(0), ...recent];
}

/** All speeds are bytes per second; recorded zeroes are part of the average. */
export function speedWindowStats(samples: readonly number[]): { peak: number; average: number; ceiling: number } {
  const recent = samples.slice(-SPEED_HISTORY_CAPACITY).map(cleanSpeed);
  const peak = Math.max(0, ...recent);
  const average = recent.reduce((total, speed) => total + speed / recent.length, 0);
  return {
    peak,
    average,
    ceiling: Math.max(MINIMUM_SPEED_CEILING, Math.min(Number.MAX_VALUE, peak * 1.18)),
  };
}

/** Time-based damping gives the same scale transition at different frame rates. */
export function dampCeiling(current: number, target: number, elapsedMs: number): number {
  const fraction = -Math.expm1(-Math.max(0, elapsedMs) / 70);
  return current + (target - current) * fraction;
}

/** Monotone Hermite interpolation keeps each cubic within its adjacent samples. */
export function buildSmoothPath(points: readonly (readonly [number, number])[], baselineY: number, topY = 4): string {
  const minimumY = Math.min(topY, baselineY);
  const coordinates = points
    .filter(([x]) => Number.isFinite(x))
    .map(([x, y]): [number, number] => [x, clamp(Number.isFinite(y) ? y : baselineY, minimumY, baselineY)]);
  if (coordinates.length === 0) return '';

  const format = (value: number): string => String(Number(value.toFixed(3)));
  const pair = (x: number, y: number): string => `${format(x)},${format(y)}`;
  let path = `M ${pair(...coordinates[0])}`;
  if (coordinates.length === 1) return path;

  const spans = coordinates.slice(1).map(([x], index) => x - coordinates[index][0]);
  const secants = spans.map((span, index) => span > 0
    ? (coordinates[index + 1][1] - coordinates[index][1]) / span
    : 0);
  const tangents = coordinates.map((_, index) => {
    if (index === 0) return secants[0];
    if (index === coordinates.length - 1) return secants[index - 1];
    const before = secants[index - 1];
    const after = secants[index];
    if (before === 0 || after === 0 || Math.sign(before) !== Math.sign(after)) return 0;
    const previousWeight = 2 * spans[index] + spans[index - 1];
    const nextWeight = spans[index] + 2 * spans[index - 1];
    return (previousWeight + nextWeight) / (previousWeight / before + nextWeight / after);
  });

  // Limit tangent pairs together so even steep, unevenly spaced samples stay monotone.
  for (let index = 0; index < secants.length; index++) {
    const slope = secants[index];
    if (slope === 0) {
      tangents[index] = 0;
      tangents[index + 1] = 0;
      continue;
    }
    const magnitude = Math.hypot(tangents[index] / slope, tangents[index + 1] / slope);
    if (magnitude > 3) {
      const factor = 3 / magnitude;
      tangents[index] *= factor;
      tangents[index + 1] *= factor;
    }
  }

  for (let index = 0; index < coordinates.length - 1; index++) {
    const [x1, y1] = coordinates[index];
    const [x2, y2] = coordinates[index + 1];
    const thirdSpan = spans[index] / 3;
    const low = Math.min(y1, y2);
    const high = Math.max(y1, y2);
    const control1 = pair(x1 + thirdSpan, clamp(y1 + tangents[index] * thirdSpan, low, high));
    const control2 = pair(x2 - thirdSpan, clamp(y2 - tangents[index + 1] * thirdSpan, low, high));
    path += ` C ${control1} ${control2} ${pair(x2, y2)}`;
  }
  return path;
}
