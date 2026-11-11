// Coastlines and borders from the backend's `basemap` command, drawn as
// canvas paths over the zone layer. The 1:110m set loads at start; 1:50m
// loads on first zoom-in and replaces it when zoomed.

import type { Backend, Basemap, BasemapScale } from '../backend';

interface Line {
  /** [lon0, lat0, lon1, lat1, ...] */
  pts: number[];
  minLon: number;
  maxLon: number;
  minLat: number;
  maxLat: number;
}

interface Layer {
  coast: Line[];
  borders: Line[];
}

function prepare(lines: number[][]): Line[] {
  return lines.map((pts) => {
    let minLon = Infinity;
    let maxLon = -Infinity;
    let minLat = Infinity;
    let maxLat = -Infinity;
    for (let i = 0; i < pts.length; i += 2) {
      minLon = Math.min(minLon, pts[i]!);
      maxLon = Math.max(maxLon, pts[i]!);
      minLat = Math.min(minLat, pts[i + 1]!);
      maxLat = Math.max(maxLat, pts[i + 1]!);
    }
    return { pts, minLon, maxLon, minLat, maxLat };
  });
}

export interface ViewBox {
  west: number;
  east: number;
  south: number;
  north: number;
}

export class BasemapLayer {
  private readonly layers = new Map<BasemapScale, Layer>();
  private readonly loading = new Set<BasemapScale>();

  constructor(
    private readonly backend: Backend,
    private readonly onLoad: () => void,
  ) {
    this.load('110m');
  }

  private load(scale: BasemapScale): void {
    if (this.layers.has(scale) || this.loading.has(scale)) return;
    this.loading.add(scale);
    this.backend
      .basemap({ scale })
      .then((b: Basemap) => {
        this.layers.set(scale, { coast: prepare(b.coastlines.lines), borders: prepare(b.borders.lines) });
        this.onLoad();
      })
      .catch((err: unknown) => console.warn(`base map ${scale} unavailable`, err))
      .finally(() => this.loading.delete(scale));
  }

  /** Draw for a view; `zoom` is the scale relative to the whole-world fit. */
  draw(
    ctx: CanvasRenderingContext2D,
    toScreen: (lon: number, lat: number) => { x: number; y: number },
    view: ViewBox,
    zoom: number,
    style: { coast: string; border: string },
  ): void {
    const want: BasemapScale = zoom >= 3 ? '50m' : '110m';
    if (want === '50m') this.load('50m');
    const layer = this.layers.get(want) ?? this.layers.get('110m');
    if (!layer) return;
    const path = (lines: Line[]) => {
      ctx.beginPath();
      for (const l of lines) {
        if (l.maxLon < view.west || l.minLon > view.east || l.maxLat < view.south || l.minLat > view.north) continue;
        const p = l.pts;
        let s = toScreen(p[0]!, p[1]!);
        ctx.moveTo(s.x, s.y);
        for (let i = 2; i < p.length; i += 2) {
          s = toScreen(p[i]!, p[i + 1]!);
          ctx.lineTo(s.x, s.y);
        }
      }
      ctx.stroke();
    };
    ctx.lineJoin = 'round';
    ctx.lineWidth = 0.7;
    ctx.strokeStyle = style.border;
    path(layer.borders);
    ctx.lineWidth = 1;
    ctx.strokeStyle = style.coast;
    path(layer.coast);
  }
}
