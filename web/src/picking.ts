// Point-in-polygon hit testing over the patch outline data.

/** Ray-casting test for the polygon whose vertices are xy pairs [start, end). */
export function pointInPolygon(x: number, y: number, xy: Float32Array, start: number, end: number): boolean {
  let inside = false;
  for (let i = start, j = end - 1; i < end; j = i++) {
    const xi = xy[2 * i];
    const yi = xy[2 * i + 1];
    const xj = xy[2 * j];
    const yj = xy[2 * j + 1];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/**
 * Return the cell containing world point (x, y), or -1 if none.
 * O(cells) scan with a per-cell ray cast — fine at 500-5000 irregular tiles.
 */
export function findCellAt(x: number, y: number, polyXy: Float32Array, polyOffsets: Uint32Array): number {
  for (let c = 0; c + 1 < polyOffsets.length; c++) {
    if (pointInPolygon(x, y, polyXy, polyOffsets[c], polyOffsets[c + 1])) return c;
  }
  return -1;
}
