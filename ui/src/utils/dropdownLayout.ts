/** WinUI rounds each layout measurement to device pixels, including item margins. */
export function dropdownLayout(scale: number, index: number, count: number) {
  const snap = (dip: number) => Math.round(dip * scale) / scale;
  const border = snap(1);
  const padding = snap(4);
  const marginY = snap(2);
  const rowHeight = snap(32);
  const pitch = rowHeight + marginY * 2;
  const selectedTop = border + padding + marginY + index * pitch;
  const height = 2 * (border + padding) + count * pitch;
  return {
    snap,
    border,
    padding,
    marginY,
    marginX: snap(5),
    rowHeight,
    minWidth: snap(130),
    selectedTop,
    revealBottom: height - selectedTop - rowHeight,
    height,
  };
}
