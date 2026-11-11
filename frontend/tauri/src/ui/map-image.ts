// Turn a MapResult (base64 zone bytes + legend) into a grid-resolution image.

import { base64ToBytes, type GridSpec, type MapResult, type ZoneLegendEntry } from '../backend';

export interface ZoneImage {
  grid: GridSpec;
  legend: ZoneLegendEntry[];
  noData: number;
  /** Decoded zone bytes, row-major from the north-west corner. */
  cells: Uint8Array;
  /** cols × rows bitmap, one pixel per cell; no-data cells are transparent. */
  bitmap: HTMLCanvasElement;
  /** Time spent decoding and painting, milliseconds. */
  paintMs: number;
}

export function paintZones(result: MapResult, alpha = 235): ZoneImage {
  const start = performance.now();
  const { grid, legend, noData } = result;
  const cells = base64ToBytes(result.cells);
  if (cells.length !== grid.rows * grid.cols) {
    throw new Error(`map payload has ${cells.length} cells, expected ${grid.rows * grid.cols}`);
  }

  // 256-entry palette of packed RGBA pixels. Writing bytes through a
  // Uint8Array view keeps this independent of platform endianness.
  const palette = new Uint32Array(256);
  const paletteBytes = new Uint8Array(palette.buffer);
  legend.forEach((z, i) => {
    paletteBytes.set([z.rgb[0], z.rgb[1], z.rgb[2], alpha], i * 4);
  });
  palette[noData] = 0;

  const bitmap = document.createElement('canvas');
  bitmap.width = grid.cols;
  bitmap.height = grid.rows;
  const ctx = bitmap.getContext('2d');
  if (!ctx) throw new Error('2D canvas unavailable');
  const image = ctx.createImageData(grid.cols, grid.rows);
  const pixels = new Uint32Array(image.data.buffer);
  for (let i = 0; i < cells.length; i++) pixels[i] = palette[cells[i]!]!;
  ctx.putImageData(image, 0, 0);

  return { grid, legend, noData, cells, bitmap, paintMs: performance.now() - start };
}

/** The zone index at a latitude/longitude, or null outside the grid / no data. */
export function zoneAt(img: ZoneImage, latDeg: number, lonDeg: number): number | null {
  const { grid } = img;
  const row = Math.floor(((grid.latMaxDeg - latDeg) / (grid.latMaxDeg - grid.latMinDeg)) * grid.rows);
  const col = Math.floor(((lonDeg - grid.lonMinDeg) / (grid.lonMaxDeg - grid.lonMinDeg)) * grid.cols);
  if (row < 0 || row >= grid.rows || col < 0 || col >= grid.cols) return null;
  const z = img.cells[row * grid.cols + col]!;
  return z === img.noData ? null : z;
}
