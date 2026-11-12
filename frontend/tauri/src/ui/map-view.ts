// Map tab: world visibility map on a canvas with zoom, pan, hover readout,
// click-to-set-location, coastlines, legend and PNG export.
//
// The view is an equirectangular camera: a centre (lon, lat) and a scale in
// CSS pixels per degree. Screen x grows with longitude, y with falling latitude.
// Zones are rasterised at device resolution for the current camera
// (map-image.ts); while panning or zooming, the last raster is transformed
// as a preview and re-rendered once the camera settles.

import { BackendError, type Backend, type MapParams, type MapResult } from '../backend';
import { observerNote } from '../dateline';
import { changed, localDate, type AppState, type Store } from '../state';
import { noonOffset, tzOf } from '../zone';
import { BasemapLayer } from './basemap-layer';
import { debounce, h, replaceChildren } from './dom';
import { downloadBase64, formatDate, formatLat, formatLon, formatUtDateTime, formatValue, valueSymbol } from './format';
import { decodeMap, rasterZones, valueAt, zoneAt, type ZoneData } from './map-image';
import { ViewPrefs } from './view-prefs';

const MAX_ZOOM = 48;
const CLICK_SLOP_PX = 4;
const RESOLUTIONS = [
  { deg: 2, label: '2° (180 × 90)' },
  { deg: 1, label: '1° (360 × 180)' },
  { deg: 0.5, label: '0.5° (720 × 360)' },
  { deg: 0.25, label: '0.25° (1440 × 720)' },
];
/** The view remembered between sessions: centre and zoom (× the fit scale). */
type MapPrefs = { lon: number; lat: number; zoom: number };
const PREFS = new ViewPrefs<MapPrefs>(
  'mooncalc.view.map.v1',
  { lon: 0, lat: 0, zoom: 1 },
  { lon: (v) => Math.abs(v) <= 180, lat: (v) => Math.abs(v) <= 90, zoom: (v) => v >= 1 && v <= MAX_ZOOM },
);

/** Graticule spacings to choose from, degrees. */
const GRID_STEPS = [30, 15, 10, 5, 2, 1, 0.5, 0.25];

interface Camera {
  lon: number;
  lat: number;
  /** CSS pixels per degree. */
  scale: number;
}

interface Palette {
  ocean: string;
  grid: string;
  gridStrong: string;
  label: string;
  marker: string;
  markerHalo: string;
  coast: string;
  border: string;
}

function readPalette(el: Element): Palette {
  const css = getComputedStyle(el);
  const v = (name: string) => css.getPropertyValue(name).trim();
  return {
    ocean: v('--map-ocean'),
    grid: v('--map-grid'),
    gridStrong: v('--map-grid-strong'),
    label: v('--map-label'),
    marker: v('--map-marker'),
    markerHalo: v('--map-marker-halo'),
    coast: v('--map-coast'),
    border: v('--map-border'),
  };
}

export class MapView {
  readonly el: HTMLElement;

  private readonly canvas = h('canvas', { class: 'map-canvas', tabindex: 0, 'aria-label': 'Visibility map' });
  private readonly ctx: CanvasRenderingContext2D;
  private readonly readout = h('span', { class: 'readout', 'aria-live': 'off' });
  /** Shown when the evening under the observer is not their civil one. */
  private readonly observerNote = h('p', { class: 'note map-date-note', role: 'note', hidden: true });
  private readonly status = h('span', { class: 'status' });
  private readonly info = h('div', { class: 'map-info' });
  private readonly overlay = h('div', { class: 'map-overlay', hidden: true });
  private readonly legend = h('ul', { class: 'legend' });
  private readonly resolutionSelect: HTMLSelectElement;
  private readonly exportButton: HTMLButtonElement;
  private readonly basemap: BasemapLayer;

  private result: MapResult | null = null;
  private data: ZoneData | null = null;
  /** Zones rasterised for `zoneCamera` at device resolution. */
  private readonly zoneCanvas = document.createElement('canvas');
  private zoneCamera: Camera | null = null;
  private rasterMs = 0;
  private camera: Camera = { lon: 0, lat: 0, scale: 1 };
  private size = { w: 0, h: 0 };
  /** Read from CSS custom properties once the element is in the document. */
  private palette: Palette | null = null;
  private requestSeq = 0;
  private active = false;
  private stale = true;
  private hover: { x: number; y: number } | null = null;
  private drag: { x: number; y: number; lon: number; lat: number; moved: boolean; id: number } | null = null;
  private frame = 0;
  private readonly scheduleRaster = debounce(() => this.renderZones(), 90);
  private readonly persist = debounce(() => {
    const c = this.camera;
    PREFS.save({ lon: c.lon, lat: c.lat, zoom: c.scale / this.fitScale() });
  }, 400);

  constructor(
    private readonly backend: Backend,
    private readonly store: Store,
  ) {
    const ctx = this.canvas.getContext('2d');
    if (!ctx) throw new Error('2D canvas unavailable');
    this.ctx = ctx;
    this.basemap = new BasemapLayer(backend, () => this.requestDraw());

    this.resolutionSelect = h(
      'select',
      {
        'aria-label': 'Grid resolution',
        onchange: () => this.store.update({ mapResolutionDeg: Number(this.resolutionSelect.value) }),
      },
      ...RESOLUTIONS.map((r) => h('option', { value: r.deg }, r.label)),
    );
    this.exportButton = h(
      'button',
      { type: 'button', class: 'button small', title: 'Render a 2048-pixel PNG with base map and legend', onclick: () => void this.exportPng() },
      'Export PNG',
    );

    const stage = h('div', { class: 'map-stage' }, this.canvas, this.overlay);
    this.el = h(
      'div',
      { class: 'map-view' },
      h(
        'div',
        { class: 'toolbar' },
        h('label', { class: 'inline' }, 'Grid ', this.resolutionSelect),
        h('button', { type: 'button', class: 'button small', onclick: () => this.fit() }, 'Fit'),
        h('button', { type: 'button', class: 'button small', onclick: () => this.zoomBy(1.5) }, '+'),
        h('button', { type: 'button', class: 'button small', onclick: () => this.zoomBy(1 / 1.5) }, '−'),
        this.exportButton,
        this.status,
      ),
      this.info,
      stage,
      h('div', { class: 'map-footer' }, h('div', { class: 'map-readouts' }, this.readout, this.observerNote), this.legend),
    );

    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      this.palette = null;
      this.requestDraw();
    });

    new ResizeObserver(() => this.resize()).observe(stage);
    this.bindPointer();

    const recompute = debounce(() => void this.compute(), 120);
    this.store.subscribe((s, prev) => {
      this.resolutionSelect.value = String(s.mapResolutionDeg);
      const dateChanged = JSON.stringify(localDate(s)) !== JSON.stringify(localDate(prev));
      if (dateChanged || changed(s, prev, ['criterionId', 'crescent', 'ephemerisId', 'mapResolutionDeg'])) {
        this.stale = true;
        if (this.active) recompute();
      }
      if (changed(s, prev, ['location'])) this.requestDraw();
      if (changed(s, prev, ['location', 'tzOffsetHours', 'timeZone'])) this.updateObserverNote();
    });
    this.resolutionSelect.value = String(this.store.get().mapResolutionDeg);
    this.updateReadout();
  }

  /** Called when the tab becomes visible or hidden. */
  setActive(active: boolean): void {
    this.active = active;
    if (active) {
      this.resize();
      if (this.stale) void this.compute();
    }
  }

  /** Centre the view on a point, zooming in to at least 2.5× the fit. */
  centreOn(latDeg: number, lonDeg: number): void {
    this.camera.scale = Math.max(this.camera.scale, this.fitScale() * 2.5);
    this.camera.lon = lonDeg;
    this.camera.lat = latDeg;
    this.clampCamera();
    this.requestDraw();
  }

  // --- Data ---------------------------------------------------------------

  private mapParams(s: AppState): MapParams {
    return {
      date: localDate(s),
      criterionId: s.criterionId,
      crescent: s.crescent,
      resolutionDeg: s.mapResolutionDeg,
      ephemerisId: s.ephemerisId,
    };
  }

  private async compute(): Promise<void> {
    const s: AppState = this.store.get();
    const seq = ++this.requestSeq;
    this.stale = false;
    this.showOverlay('Computing map…');
    const t0 = performance.now();
    try {
      const result = await this.backend.computeVisibilityMap(this.mapParams(s));
      if (seq !== this.requestSeq) return;
      const roundTripMs = performance.now() - t0;
      this.data = decodeMap(result);
      this.result = result;
      this.zoneCamera = null;
      this.renderLegend();
      this.renderInfo();
      this.updateObserverNote();
      this.overlay.hidden = true;
      const t = result.timing;
      replaceChildren(
        this.status,
        `${result.grid.cols} × ${result.grid.rows} · backend ${t.totalMs.toFixed(0)} ms ` +
          `(${t.threads} threads) · round trip ${roundTripMs.toFixed(0)} ms · decode ${this.data.decodeMs.toFixed(0)} ms`,
      );
      this.draw();
    } catch (err) {
      if (seq !== this.requestSeq) return;
      const e = BackendError.from(err);
      if (e.code === 'cancelled') return;
      this.showOverlay(`Could not compute the map: ${e.message}`);
      this.stale = true;
    }
  }

  private async exportPng(): Promise<void> {
    const s = this.store.get();
    this.exportButton.disabled = true;
    try {
      const params = {
        map: { ...this.mapParams(s), includeValues: false },
        widthPx: 2048,
        legend: true,
        marker: { location: s.location, label: s.locationName?.split(',')[0] ?? null },
      };
      if (this.backend.saveMapPng) {
        const saved = await this.backend.saveMapPng(params);
        this.showOverlay(`Saved ${saved.width} × ${saved.height} PNG to ${saved.path}`);
        setTimeout(() => (this.overlay.hidden = true), 6000);
      } else {
        const png = await this.backend.exportMapPng(params);
        downloadBase64(png.png, png.fileName, 'image/png');
      }
    } catch (err) {
      this.showOverlay(`Export failed: ${BackendError.from(err).message}`);
    } finally {
      this.exportButton.disabled = false;
    }
  }

  private showOverlay(text: string): void {
    this.overlay.textContent = text;
    this.overlay.hidden = false;
  }

  private renderLegend(): void {
    const r = this.result;
    if (!r) return;
    const swatch = (rgb: string) => {
      const s = h('span', { class: 'swatch' });
      s.style.background = rgb;
      return s;
    };
    const total = r.grid.rows * r.grid.cols;
    const pct = (n: number) => (n === 0 ? '' : ` · ${((100 * n) / total).toFixed(n * 1000 < total ? 1 : 0)} %`);
    const none = total - r.zoneCounts.reduce((a, b) => a + b, 0);
    replaceChildren(
      this.legend,
      ...r.legend.map((z, i) =>
        h(
          'li',
          { title: `${z.description}${pct(r.zoneCounts[i] ?? 0)}` },
          swatch(`rgb(${z.rgb.join(' ')})`),
          h('strong', {}, z.code),
          h('span', {}, z.description),
        ),
      ),
      h('li', { title: `Moon sets first, before conjunction, or no sunset${pct(none)}` }, h('span', { class: 'swatch swatch-none' }), h('span', {}, 'No crescent')),
    );
  }

  private renderInfo(): void {
    const r = this.result;
    if (!r) return;
    const sym = valueSymbol(r.criterionId);
    const best = r.best;
    const zone = best ? r.legend[best.zone] : null;
    replaceChildren(
      this.info,
      h('span', {}, h('strong', {}, `${r.criterionName}`), ` · ${formatDate(r.date)} ${r.crescent}`),
      h('span', {}, 'Conjunction ', h('strong', {}, formatUtDateTime(r.conjunction))),
      h('span', {}, `Evaluated at: ${r.evaluationTime}`),
      best && zone
        ? h(
            'span',
            {},
            'Best: ',
            h('strong', {}, zone.code),
            ` at ${formatLat(best.latDeg, 1)} ${formatLon(best.lonDeg, 1)}`,
            best.value !== null ? ` (${sym} ${formatValue(best.value)})` : '',
          )
        : h('span', {}, 'No crescent anywhere on this date'),
      h('span', { class: 'hint' }, r.ephemeris),
      h('span', { class: 'hint map-convention' }, r.dateConvention),
    );
  }

  private updateObserverNote(): void {
    const r = this.result;
    const s = this.store.get();
    const note = r ? observerNote(r.date, s.location.lonDeg, noonOffset(tzOf(s), r.date), r.crescent) : null;
    this.observerNote.textContent = note ?? '';
    this.observerNote.hidden = note === null;
  }

  // --- Camera -------------------------------------------------------------

  private fitScale(): number {
    return Math.max(1e-3, Math.min(this.size.w / 360, this.size.h / 180));
  }

  fit(): void {
    this.camera = { lon: 0, lat: 0, scale: this.fitScale() };
    this.requestDraw();
  }

  zoomBy(factor: number, at?: { x: number; y: number }): void {
    const p = at ?? { x: this.size.w / 2, y: this.size.h / 2 };
    const before = this.toGeo(p.x, p.y);
    const min = this.fitScale();
    this.camera.scale = Math.min(min * MAX_ZOOM, Math.max(min, this.camera.scale * factor));
    // Keep the point under the cursor fixed.
    this.camera.lon = before.lon - (p.x - this.size.w / 2) / this.camera.scale;
    this.camera.lat = before.lat + (p.y - this.size.h / 2) / this.camera.scale;
    this.clampCamera();
    this.requestDraw();
  }

  private clampCamera(): void {
    const c = this.camera;
    const halfW = this.size.w / 2 / c.scale;
    const halfH = this.size.h / 2 / c.scale;
    c.lon = halfW >= 180 ? 0 : Math.min(180 - halfW, Math.max(-180 + halfW, c.lon));
    c.lat = halfH >= 90 ? 0 : Math.min(90 - halfH, Math.max(-90 + halfH, c.lat));
  }

  private toScreen(lon: number, lat: number): { x: number; y: number } {
    const c = this.camera;
    return { x: this.size.w / 2 + (lon - c.lon) * c.scale, y: this.size.h / 2 - (lat - c.lat) * c.scale };
  }

  private toGeo(x: number, y: number): { lon: number; lat: number } {
    const c = this.camera;
    return { lon: c.lon + (x - this.size.w / 2) / c.scale, lat: c.lat - (y - this.size.h / 2) / c.scale };
  }

  private resize(): void {
    const rect = this.canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;
    const first = this.size.w === 0;
    const wasFit = first || Math.abs(this.camera.scale - this.fitScale()) < 1e-9;
    this.size = { w: rect.width, h: rect.height };
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.round(rect.width * dpr);
    this.canvas.height = Math.round(rect.height * dpr);
    if (first) {
      // First layout: restore the remembered view.
      const p = PREFS.load();
      this.camera = { lon: p.lon, lat: p.lat, scale: this.fitScale() * p.zoom };
    } else if (wasFit) this.camera = { lon: 0, lat: 0, scale: this.fitScale() };
    else this.camera.scale = Math.max(this.camera.scale, this.fitScale());
    this.clampCamera();
    this.zoneCamera = null;
    this.draw();
  }

  // --- Input --------------------------------------------------------------

  private localPoint(e: MouseEvent): { x: number; y: number } {
    const rect = this.canvas.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  private bindPointer(): void {
    const cv = this.canvas;
    cv.addEventListener(
      'wheel',
      (e) => {
        e.preventDefault();
        this.zoomBy(Math.exp(-e.deltaY * 0.0015), this.localPoint(e));
      },
      { passive: false },
    );
    cv.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      const p = this.localPoint(e);
      this.drag = { ...p, lon: this.camera.lon, lat: this.camera.lat, moved: false, id: e.pointerId };
      cv.setPointerCapture(e.pointerId);
    });
    cv.addEventListener('pointermove', (e) => {
      const p = this.localPoint(e);
      this.hover = p;
      const d = this.drag;
      if (d && d.id === e.pointerId) {
        const dx = p.x - d.x;
        const dy = p.y - d.y;
        if (!d.moved && Math.hypot(dx, dy) > CLICK_SLOP_PX) {
          d.moved = true;
          cv.classList.add('dragging');
        }
        if (d.moved) {
          this.camera.lon = d.lon - dx / this.camera.scale;
          this.camera.lat = d.lat + dy / this.camera.scale;
          this.clampCamera();
          this.requestDraw();
        }
      }
      this.updateReadout();
    });
    const endDrag = (e: PointerEvent, click: boolean) => {
      const d = this.drag;
      if (!d || d.id !== e.pointerId) return;
      this.drag = null;
      cv.classList.remove('dragging');
      if (click && !d.moved) void this.setLocationAt(this.localPoint(e));
    };
    cv.addEventListener('pointerup', (e) => endDrag(e, true));
    cv.addEventListener('pointercancel', (e) => endDrag(e, false));
    cv.addEventListener('pointerleave', () => {
      this.hover = null;
      this.updateReadout();
    });
    cv.addEventListener('keydown', (e) => {
      const pan = 40 / this.camera.scale;
      const moves: Record<string, () => void> = {
        '+': () => this.zoomBy(1.5),
        '=': () => this.zoomBy(1.5),
        '-': () => this.zoomBy(1 / 1.5),
        '0': () => this.fit(),
        ArrowLeft: () => (this.camera.lon -= pan),
        ArrowRight: () => (this.camera.lon += pan),
        ArrowUp: () => (this.camera.lat += pan),
        ArrowDown: () => (this.camera.lat -= pan),
      };
      const action = moves[e.key];
      if (!action) return;
      e.preventDefault();
      action();
      this.clampCamera();
      this.requestDraw();
    });
  }

  private async setLocationAt(p: { x: number; y: number }): Promise<void> {
    const { lon, lat } = this.toGeo(p.x, p.y);
    if (lat < -90 || lat > 90 || lon < -180 || lon > 180) return;
    const location = { latDeg: +lat.toFixed(4), lonDeg: +lon.toFixed(4), elevationM: 0 };
    this.store.update({ location, locationName: null });
    try {
      const near = await this.backend.nearestCity({ latDeg: location.latDeg, lonDeg: location.lonDeg });
      const s = this.store.get();
      if (!near || s.location !== location) return;
      const km = near.distanceKm;
      this.store.update({
        locationName: km < 5 ? `${near.city.name}, ${near.city.country}` : `${km.toFixed(0)} km from ${near.city.name}, ${near.city.country}`,
      });
    } catch {
      // The name is a nicety; the coordinates are set.
    }
  }

  private updateReadout(): void {
    if (!this.hover) {
      this.readout.textContent = 'Hover over the map · click to set the location · drag to pan · wheel to zoom';
      return;
    }
    const { lon, lat } = this.toGeo(this.hover.x, this.hover.y);
    if (lat < -90 || lat > 90 || lon < -180 || lon > 180) {
      this.readout.textContent = 'Outside the map';
      return;
    }
    let zone = '';
    if (this.data && this.result) {
      const z = zoneAt(this.data, lat, lon);
      const v = valueAt(this.data, lat, lon);
      const sym = valueSymbol(this.result.criterionId);
      zone =
        z === null
          ? ' · no crescent'
          : ` · ${this.data.legend[z]!.code}: ${this.data.legend[z]!.description}${v !== null ? ` · ${sym} = ${formatValue(v)}` : ''}`;
    }
    this.readout.textContent = `${formatLat(lat)}  ${formatLon(lon)}${zone}`;
  }

  // --- Drawing ------------------------------------------------------------

  private requestDraw(): void {
    if (this.size.w > 0) this.persist();
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.draw();
    });
  }

  /** Rasterise the zones for the current camera at device resolution. */
  private renderZones(): void {
    if (!this.data || this.size.w === 0) return;
    const t0 = performance.now();
    const W = this.canvas.width;
    const H = this.canvas.height;
    const dpr = W / this.size.w;
    if (this.zoneCanvas.width !== W || this.zoneCanvas.height !== H) {
      this.zoneCanvas.width = W;
      this.zoneCanvas.height = H;
    }
    const zctx = this.zoneCanvas.getContext('2d');
    if (!zctx) return;
    const image = zctx.createImageData(W, H);
    const origin = this.toGeo(0, 0);
    const step = 1 / (this.camera.scale * dpr);
    rasterZones(this.data, image, { lon0: origin.lon, dLon: step, lat0: origin.lat, dLat: step });
    zctx.putImageData(image, 0, 0);
    this.zoneCamera = { ...this.camera };
    this.rasterMs = performance.now() - t0;
    this.draw();
  }

  private drawZones(): void {
    if (!this.data) return;
    const z = this.zoneCamera;
    const c = this.camera;
    if (!z) {
      this.renderZones();
      return;
    }
    const { w, h: ht } = this.size;
    if (z.lon === c.lon && z.lat === c.lat && z.scale === c.scale) {
      this.ctx.drawImage(this.zoneCanvas, 0, 0, w, ht);
      return;
    }
    // Preview: the last raster under the camera change, then re-render.
    const k = c.scale / z.scale;
    const dx = w / 2 + (z.lon - c.lon) * c.scale - (w / 2) * k;
    const dy = ht / 2 - (z.lat - c.lat) * c.scale - (ht / 2) * k;
    this.ctx.imageSmoothingEnabled = true;
    this.ctx.drawImage(this.zoneCanvas, dx, dy, w * k, ht * k);
    this.scheduleRaster();
  }

  private draw(): void {
    const { w, h: ht } = this.size;
    if (w === 0 || !this.el.isConnected) return;
    this.palette ??= readPalette(this.el);
    const { ctx, palette: pal } = this;
    const dpr = this.canvas.width / w;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, ht);

    const nw = this.toScreen(-180, 90);
    const se = this.toScreen(180, -90);
    ctx.fillStyle = pal.ocean;
    ctx.fillRect(nw.x, nw.y, se.x - nw.x, se.y - nw.y);

    this.drawZones();

    const tl = this.toGeo(0, 0);
    const br = this.toGeo(w, ht);
    this.basemap.draw(
      ctx,
      (lon, lat) => this.toScreen(lon, lat),
      { west: tl.lon, east: br.lon, south: br.lat, north: tl.lat },
      this.camera.scale / this.fitScale(),
      { coast: pal.coast, border: pal.border },
    );
    this.drawGraticule(pal, nw, se);
    this.drawBest(pal);
    this.drawMarker(pal);
  }

  private drawGraticule(pal: Palette, nw: { x: number; y: number }, se: { x: number; y: number }): void {
    const { ctx, camera } = this;
    const step = [...GRID_STEPS].reverse().find((s) => s * camera.scale >= 70) ?? GRID_STEPS[0]!;
    const view = { tl: this.toGeo(0, 0), br: this.toGeo(this.size.w, this.size.h) };
    const lonFrom = Math.max(-180, Math.ceil(view.tl.lon / step) * step);
    const lonTo = Math.min(180, view.br.lon);
    const latFrom = Math.max(-90, Math.ceil(view.br.lat / step) * step);
    const latTo = Math.min(90, view.tl.lat);

    ctx.lineWidth = 1;
    ctx.font = '11px system-ui, sans-serif';
    ctx.fillStyle = pal.label;
    for (let lon = lonFrom; lon <= lonTo + 1e-9; lon += step) {
      const { x } = this.toScreen(lon, 0);
      ctx.strokeStyle = Math.abs(lon) < 1e-9 ? pal.gridStrong : pal.grid;
      ctx.beginPath();
      ctx.moveTo(Math.round(x) + 0.5, Math.max(0, nw.y));
      ctx.lineTo(Math.round(x) + 0.5, Math.min(this.size.h, se.y));
      ctx.stroke();
      if (Math.abs(lon) < 180) ctx.fillText(formatLon(lon, step < 1 ? 2 : 0), x + 3, Math.max(0, nw.y) + 12);
    }
    for (let lat = latFrom; lat <= latTo + 1e-9; lat += step) {
      const { y } = this.toScreen(0, lat);
      ctx.strokeStyle = Math.abs(lat) < 1e-9 ? pal.gridStrong : pal.grid;
      ctx.beginPath();
      ctx.moveTo(Math.max(0, nw.x), Math.round(y) + 0.5);
      ctx.lineTo(Math.min(this.size.w, se.x), Math.round(y) + 0.5);
      ctx.stroke();
      if (Math.abs(lat) < 90) ctx.fillText(formatLat(lat, step < 1 ? 2 : 0), Math.max(0, nw.x) + 3, y - 3);
    }
  }

  /** A small star at the best cell. */
  private drawBest(pal: Palette): void {
    const best = this.result?.best;
    if (!best) return;
    const { ctx } = this;
    const { x, y } = this.toScreen(best.lonDeg, best.latDeg);
    ctx.beginPath();
    for (let i = 0; i < 10; i++) {
      const r = i % 2 === 0 ? 7 : 3;
      const a = -Math.PI / 2 + (i * Math.PI) / 5;
      ctx.lineTo(x + r * Math.cos(a), y + r * Math.sin(a));
    }
    ctx.closePath();
    ctx.fillStyle = '#ffffff';
    ctx.strokeStyle = pal.marker === '#ffffff' ? '#000000' : pal.marker;
    ctx.lineWidth = 1.2;
    ctx.fill();
    ctx.stroke();
  }

  private drawMarker(pal: Palette): void {
    const { ctx } = this;
    const loc = this.store.get().location;
    const { x, y } = this.toScreen(loc.lonDeg, loc.latDeg);
    ctx.lineWidth = 4;
    ctx.strokeStyle = pal.markerHalo;
    ctx.beginPath();
    ctx.arc(x, y, 6, 0, Math.PI * 2);
    ctx.stroke();
    ctx.lineWidth = 2;
    ctx.strokeStyle = pal.marker;
    ctx.beginPath();
    ctx.arc(x, y, 6, 0, Math.PI * 2);
    ctx.moveTo(x - 11, y);
    ctx.lineTo(x - 7, y);
    ctx.moveTo(x + 7, y);
    ctx.lineTo(x + 11, y);
    ctx.moveTo(x, y - 11);
    ctx.lineTo(x, y - 7);
    ctx.moveTo(x, y + 7);
    ctx.lineTo(x, y + 11);
    ctx.stroke();
  }

  /** Last zone raster time, for diagnostics. */
  get lastRasterMs(): number {
    return this.rasterMs;
  }
}
