// Decoding a MapResult and rasterising its zones at screen resolution.
//
// Smooth zones: where the criterion's zones are thresholds of one value
// (Yallop q, Odeh V, a margin), each screen pixel takes the bilinear
// interpolation of the four surrounding cell values and classifies it, so
// zone edges are the true contour lines of the interpolated field (as
// mooncalc_maps::render does). Where a neighbour has no value (the edge of
// the "no crescent" region) or the criterion has no thresholds, the pixel
// takes a bilinear vote of the neighbouring cells' zones instead.

import { base64ToBytes, type GridSpec, type MapResult, type ZoneLegendEntry } from '../backend';

export interface ZoneData {
  grid: GridSpec;
  legend: ZoneLegendEntry[];
  noData: number;
  /** Zone bytes, row-major from the north-west corner. */
  cells: Uint8Array;
  /** Criterion value per cell (NaN = none), or null when not sent. */
  values: Float32Array | null;
  /** Descending zone thresholds for `values`, or null. */
  thresholds: number[] | null;
  /** The longitude span covers the whole globe, so columns wrap. */
  wraps: boolean;
  /** Time spent decoding, milliseconds. */
  decodeMs: number;
}

export function decodeMap(result: MapResult): ZoneData {
  const start = performance.now();
  const { grid, legend, noData } = result;
  const n = grid.rows * grid.cols;
  const cells = base64ToBytes(result.cells);
  if (cells.length !== n) throw new Error(`map payload has ${cells.length} cells, expected ${n}`);

  let values: Float32Array | null = null;
  if (result.values) {
    const raw = base64ToBytes(result.values.data);
    if (raw.length !== 2 * n) throw new Error(`value field has ${raw.length} bytes, expected ${2 * n}`);
    const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    const { scale, offset, noValue } = result.values;
    values = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const q = view.getInt16(2 * i, true);
      values[i] = q === noValue ? NaN : offset + scale * q;
    }
  }
  return {
    grid,
    legend,
    noData,
    cells,
    values,
    thresholds: result.thresholds,
    wraps: Math.abs(grid.lonMaxDeg - grid.lonMinDeg - 360) < 1e-9,
    decodeMs: performance.now() - start,
  };
}

function cellIndex(d: ZoneData, latDeg: number, lonDeg: number): number | null {
  const { grid } = d;
  const row = Math.floor(((grid.latMaxDeg - latDeg) / (grid.latMaxDeg - grid.latMinDeg)) * grid.rows);
  let lon = lonDeg;
  if (d.wraps) lon = ((((lon - grid.lonMinDeg) % 360) + 360) % 360) + grid.lonMinDeg;
  const col = Math.floor(((lon - grid.lonMinDeg) / (grid.lonMaxDeg - grid.lonMinDeg)) * grid.cols);
  if (row < 0 || row >= grid.rows || col < 0 || col >= grid.cols) return null;
  return row * grid.cols + col;
}

/** The zone of the cell under a point, or null outside the grid / no data. */
export function zoneAt(d: ZoneData, latDeg: number, lonDeg: number): number | null {
  const i = cellIndex(d, latDeg, lonDeg);
  if (i === null) return null;
  const z = d.cells[i]!;
  return z === d.noData ? null : z;
}

/** The criterion value of the cell under a point, if any. */
export function valueAt(d: ZoneData, latDeg: number, lonDeg: number): number | null {
  if (!d.values) return null;
  const i = cellIndex(d, latDeg, lonDeg);
  if (i === null) return null;
  const v = d.values[i]!;
  return Number.isFinite(v) ? v : null;
}

/** An affine map from device pixels to geographic degrees. */
export interface PixelGeo {
  /** Longitude of device pixel column x is lon0 + x * dLon (pixel centres at x + 0.5). */
  lon0: number;
  dLon: number;
  /** Latitude of device pixel row y is lat0 - y * dLat. */
  lat0: number;
  dLat: number;
}

/**
 * Rasterise the zones into `image` (device pixels), one colour per pixel,
 * transparent where there is no crescent or outside the grid.
 */
export function rasterZones(d: ZoneData, image: ImageData, geo: PixelGeo, alpha = 210): void {
  const { width: w, height: h } = image;
  const { grid, cells, values, noData } = d;
  const { rows, cols } = grid;
  const cellLon = (grid.lonMaxDeg - grid.lonMinDeg) / cols;
  const cellLat = (grid.latMaxDeg - grid.latMinDeg) / rows;
  const thresholds = values && d.thresholds ? d.thresholds : null;

  // RGBA per zone, packed for a Uint32Array view (byte order independent).
  const palette = new Uint32Array(256);
  const pbytes = new Uint8Array(palette.buffer);
  d.legend.forEach((z, i) => pbytes.set([z.rgb[0], z.rgb[1], z.rgb[2], alpha], i * 4));
  palette[noData] = 0;

  // Per column: left neighbour column, right column, weight of the right one.
  const c0 = new Int32Array(w);
  const c1 = new Int32Array(w);
  const fx = new Float32Array(w);
  const colIn = new Uint8Array(w);
  for (let x = 0; x < w; x++) {
    let lon = geo.lon0 + (x + 0.5) * geo.dLon;
    if (d.wraps) lon = ((((lon - grid.lonMinDeg) % 360) + 360) % 360) + grid.lonMinDeg;
    const u = (lon - grid.lonMinDeg) / cellLon - 0.5;
    colIn[x] = lon >= grid.lonMinDeg && lon <= grid.lonMaxDeg ? 1 : 0;
    let a = Math.floor(u);
    let f = u - a;
    let b = a + 1;
    if (d.wraps) {
      a = ((a % cols) + cols) % cols;
      b = ((b % cols) + cols) % cols;
    } else {
      if (a < 0) { a = 0; f = 0; }
      if (b >= cols) { b = cols - 1; f = a >= cols - 1 ? 0 : f; }
      if (a >= cols) a = cols - 1;
    }
    c0[x] = a;
    c1[x] = b;
    fx[x] = f;
  }

  const out = new Uint32Array(image.data.buffer);
  for (let y = 0; y < h; y++) {
    const lat = geo.lat0 - (y + 0.5) * geo.dLat;
    const rowBase = y * w;
    if (lat < grid.latMinDeg || lat > grid.latMaxDeg) {
      out.fill(0, rowBase, rowBase + w);
      continue;
    }
    const v = (grid.latMaxDeg - lat) / cellLat - 0.5;
    let r0 = Math.floor(v);
    let fy = v - r0;
    let r1 = r0 + 1;
    if (r0 < 0) { r0 = 0; fy = 0; }
    if (r1 >= rows) { r1 = rows - 1; if (r0 >= rows - 1) { r0 = rows - 1; fy = 0; } }
    const o0 = r0 * cols;
    const o1 = r1 * cols;
    for (let x = 0; x < w; x++) {
      if (!colIn[x]) {
        out[rowBase + x] = 0;
        continue;
      }
      const i00 = o0 + c0[x]!;
      const i01 = o0 + c1[x]!;
      const i10 = o1 + c0[x]!;
      const i11 = o1 + c1[x]!;
      const ax = fx[x]!;
      const w00 = (1 - ax) * (1 - fy);
      const w01 = ax * (1 - fy);
      const w10 = (1 - ax) * fy;
      const w11 = ax * fy;
      if (thresholds) {
        const v00 = values![i00]!;
        const v01 = values![i01]!;
        const v10 = values![i10]!;
        const v11 = values![i11]!;
        // NaN in any neighbour makes the sum NaN: fall through to the vote.
        const val = v00 * w00 + v01 * w01 + v10 * w10 + v11 * w11;
        if (val === val) {
          let z = 0;
          while (z < thresholds.length && val < thresholds[z]!) z++;
          out[rowBase + x] = palette[z]!;
          continue;
        }
      }
      // Bilinear vote between the zones of the four neighbours.
      const z00 = cells[i00]!;
      const z01 = cells[i01]!;
      const z10 = cells[i10]!;
      const z11 = cells[i11]!;
      let best = z00;
      let bestW = w00 + (z01 === z00 ? w01 : 0) + (z10 === z00 ? w10 : 0) + (z11 === z00 ? w11 : 0);
      if (z01 !== z00) {
        const wt = w01 + (z10 === z01 ? w10 : 0) + (z11 === z01 ? w11 : 0);
        if (wt > bestW) { best = z01; bestW = wt; }
      }
      if (z10 !== z00 && z10 !== z01) {
        const wt = w10 + (z11 === z10 ? w11 : 0);
        if (wt > bestW) { best = z10; bestW = wt; }
      }
      if (z11 !== z00 && z11 !== z01 && z11 !== z10 && w11 > bestW) best = z11;
      out[rowBase + x] = palette[best]!;
    }
  }
}
