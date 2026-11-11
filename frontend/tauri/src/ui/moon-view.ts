// Moon tab: a close-up of the Moon at the app time and location.
//
// * The disc with the correct phase (terminator from the Sun's
//   selenographic longitude and latitude), orientation (axis position angle
//   P; north up, or zenith up with the parallactic angle q) and libration
//   (the disc centre is the sub-observer point), with the maria and named
//   craters of the IAU Gazetteer (labels toggle).
// * The libration in longitude against latitude through the lunation.
// * A data panel: phase, illumination, age, distance, diameter, libration,
//   P, χ, q, colongitude, and sunrise/sunset at a chosen crater.
//
// The projection is the one of `mooncalc_app::moon::disc::View` (Meeus ch.
// 53): a point at selenographic (η, θ) maps to x = cos θ sin(η − l),
// y = sin θ cos b − cos θ sin b cos(η − l), z = … (toward the observer),
// rotated anticlockwise by the screen angle of the lunar north pole.

import './sky.css';

import { BackendError, type Backend, type DiscView, type MoonFeatures, type MoonView, type MoonViewParams, type UtDateTime } from '../backend';
import { changed, type Store } from '../state';
import { debounce, h, replaceChildren } from './dom';
import { DEG, Stage, cssVar, dateHm, label, msToUt, norm360, utMs, zoneLabel } from './sky-common';

type Row = [string, string | Node];

function kv(rows: Row[]): HTMLTableElement {
  return h('table', { class: 'kv' }, h('tbody', {}, ...rows.map(([k, v]) => h('tr', {}, h('th', { scope: 'row' }, k), h('td', {}, v)))));
}

/** The disc geometry, radians (`mooncalc_app::moon::disc::View`). */
class Disc {
  readonly l: number;
  readonly b: number;
  readonly sunLon: number;
  readonly sunLat: number;
  readonly rot: number;

  constructor(v: DiscView, zenithUp: boolean) {
    this.l = v.lDeg * DEG;
    this.b = v.bDeg * DEG;
    this.sunLon = v.sunLonDeg * DEG;
    this.sunLat = v.sunLatDeg * DEG;
    this.rot = ((zenithUp && v.zenithUpRotationDeg !== null ? v.zenithUpRotationDeg : v.northUpRotationDeg) ?? 0) * DEG;
  }

  /** (right, up, toward the observer) on a unit disc. */
  project(eta: number, theta: number): [number, number, number] {
    const st = Math.sin(theta);
    const ct = Math.cos(theta);
    const sd = Math.sin(eta - this.l);
    const cd = Math.cos(eta - this.l);
    const sb = Math.sin(this.b);
    const cb = Math.cos(this.b);
    const x = ct * sd;
    const y = st * cb - ct * sb * cd;
    const z = st * sb + ct * cb * cd;
    const sr = Math.sin(this.rot);
    const cr = Math.cos(this.rot);
    return [x * cr - y * sr, x * sr + y * cr, z];
  }

  unproject(u: number, v: number): [number, number] | null {
    const sr = Math.sin(this.rot);
    const cr = Math.cos(this.rot);
    const x = u * cr + v * sr;
    const y = -u * sr + v * cr;
    const z2 = 1 - x * x - y * y;
    if (z2 < 0) return null;
    const z = Math.sqrt(z2);
    const sb = Math.sin(this.b);
    const cb = Math.cos(this.b);
    return [this.l + Math.atan2(x, z * cb - y * sb), Math.asin(Math.max(-1, Math.min(1, y * cb + z * sb)))];
  }

  sunDot(eta: number, theta: number): number {
    return Math.sin(theta) * Math.sin(this.sunLat) + Math.cos(theta) * Math.cos(this.sunLat) * Math.cos(eta - this.sunLon);
  }
}

/** Points (lat, lon radians) around a small circle of angular radius r. */
function smallCircle(lat: number, lon: number, r: number, n: number): [number, number][] {
  const out: [number, number][] = [];
  for (let i = 0; i <= n; i++) {
    const az = (2 * Math.PI * i) / n;
    const lat2 = Math.asin(Math.sin(lat) * Math.cos(r) + Math.cos(lat) * Math.sin(r) * Math.cos(az));
    const lon2 = lon + Math.atan2(Math.sin(az) * Math.sin(r) * Math.cos(lat), Math.cos(r) - Math.sin(lat) * Math.sin(lat2));
    out.push([lat2, lon2]);
  }
  return out;
}

/** A 0.5° equirectangular mare mask drawn from the outlines. */
class MareMask {
  private readonly data: Uint8ClampedArray;
  static readonly W = 720;
  static readonly H = 360;

  constructor(f: MoonFeatures) {
    const c = new OffscreenCanvas(MareMask.W, MareMask.H);
    const ctx = c.getContext('2d')!;
    ctx.fillStyle = '#fff';
    for (const m of f.maria) {
      ctx.beginPath();
      for (let i = 0; i < m.outline.length; i += 2) {
        const x = (m.outline[i + 1]! + 180) * 2;
        const y = (90 - m.outline[i]!) * 2;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.closePath();
      ctx.fill();
    }
    this.data = ctx.getImageData(0, 0, MareMask.W, MareMask.H).data;
  }

  at(lat: number, lon: number): boolean {
    const latD = lat / DEG;
    const lonD = norm360(lon / DEG + 180) - 180;
    const row = Math.min(MareMask.H - 1, Math.max(0, Math.floor((90 - latD) * 2)));
    const col = Math.min(MareMask.W - 1, Math.max(0, Math.floor((lonD + 180) * 2)));
    return this.data[(row * MareMask.W + col) * 4 + 3]! > 127;
  }
}

function smoothstep(e0: number, e1: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
}

/** Rasterise the lit disc into an n × n image (transparent outside). */
function rasterize(disc: Disc, n: number, mask: MareMask | null): ImageData {
  const img = new ImageData(n, n);
  const half = n / 2;
  for (let j = 0; j < n; j++) {
    const v = 1 - (j + 0.5) / half;
    for (let i = 0; i < n; i++) {
      const u = (i + 0.5) / half - 1;
      const r = Math.hypot(u, v);
      if (r > 1 + 1 / half) continue;
      const k = r > 1 ? 0.9999 / r : 0.9999;
      const p = disc.unproject(u * k, v * k);
      if (!p) continue;
      const [eta, theta] = p;
      const albedo = mask && mask.at(theta, eta) ? 0.6 : 0.88;
      const dot = disc.sunDot(eta, theta);
      const lit = smoothstep(-0.012, 0.03, dot);
      const day = albedo * (0.8 + 0.2 * Math.sqrt(Math.max(0, dot)));
      const night = albedo * 0.14;
      const value = night + (day - night) * lit;
      const alpha = Math.max(0, Math.min(1, (1 - r) * half + 0.5));
      const o = (j * n + i) * 4;
      img.data[o] = value * 255;
      img.data[o + 1] = value * 0.98 * 255;
      img.data[o + 2] = value * 0.93 * 255;
      img.data[o + 3] = alpha * 255;
    }
  }
  return img;
}

const CRATER_KEY = 'mooncalc.moon.crater';

export class MoonTabView {
  readonly el = h('div', { class: 'sky-view moon-view' });
  private readonly discEl = h('div', { class: 'sky-stage moon-disc' });
  private readonly graphEl = h('div', { class: 'sky-stage moon-graph' });
  private readonly disc: Stage;
  private readonly graph: Stage;
  private readonly status = h('span', { class: 'status' });
  private readonly data = h('div', { class: 'moon-data' });
  private readonly craterSelect = h('select', { 'aria-label': 'Crater for lunar sunrise and sunset' });
  private readonly orientButtons: HTMLButtonElement[];

  private active = false;
  private stale = true;
  private seq = 0;
  private features: MoonFeatures | null = null;
  private mask: MareMask | null = null;
  private view: MoonView | null = null;
  private zenithUp = false;
  private showMaria = true;
  private showCraters = true;
  private showLabels = true;
  private crater = 'Copernicus';
  private texture: { key: string; canvas: OffscreenCanvas } | null = null;

  constructor(
    private readonly backend: Backend,
    private readonly store: Store,
  ) {
    try {
      this.crater = localStorage.getItem(CRATER_KEY) ?? this.crater;
    } catch {
      // Not remembered.
    }
    this.disc = new Stage(this.discEl, (ctx, w, hh) => this.drawDisc(ctx, w, hh));
    this.graph = new Stage(this.graphEl, (ctx, w, hh) => this.drawGraph(ctx, w, hh));
    const orient = (zen: boolean, text: string) =>
      h('button', { type: 'button', class: 'segment', 'aria-pressed': String(zen === this.zenithUp), onclick: () => this.setOrientation(zen) }, text);
    this.orientButtons = [orient(false, 'North up'), orient(true, 'Zenith up')];
    const check = (text: string, value: boolean, set: (v: boolean) => void) =>
      h('label', { class: 'inline' }, h('input', { type: 'checkbox', checked: value, onchange: (e: Event) => {
        set((e.target as HTMLInputElement).checked);
        this.disc.redraw();
      } }), text);
    const step = (text: string, hours: number) =>
      h('button', { type: 'button', class: 'button small', title: `${hours > 0 ? '+' : '−'}${Math.abs(hours) >= 24 ? `${Math.abs(hours) / 24} day` : `${Math.abs(hours)} h`}`, onclick: () => this.shift(hours) }, text);
    this.craterSelect.addEventListener('change', () => {
      this.crater = this.craterSelect.value;
      try {
        localStorage.setItem(CRATER_KEY, this.crater);
      } catch {
        // Not remembered.
      }
      void this.refresh(true);
    });
    this.el.append(
      h(
        'div',
        { class: 'toolbar' },
        h('div', { class: 'segmented', role: 'group', 'aria-label': 'Orientation' }, ...this.orientButtons),
        check('Maria', this.showMaria, (v) => (this.showMaria = v)),
        check('Craters', this.showCraters, (v) => (this.showCraters = v)),
        check('Labels', this.showLabels, (v) => (this.showLabels = v)),
        h('span', { class: 'inline' }, step('−1 d', -24), step('−1 h', -1), step('+1 h', 1), step('+1 d', 24)),
        this.status,
      ),
      h('div', { class: 'moon-grid' }, this.discEl, h('div', { class: 'moon-side' }, h('h3', {}, 'Libration through the lunation'), this.graphEl, this.data)),
    );

    const refresh = debounce(() => void this.refresh(), 120);
    this.store.subscribe((s, prev) => {
      if (changed(s, prev, ['instant', 'tzOffsetHours', 'location', 'ephemerisId'])) {
        this.stale = true;
        if (this.active) refresh();
      }
    });
  }

  setActive(active: boolean): void {
    this.active = active;
    if (active && this.stale) void this.refresh();
    if (active) {
      this.disc.redraw();
      this.graph.redraw();
    }
  }

  private shift(hours: number): void {
    const s = this.store.get();
    this.store.update({ instant: msToUt(utMs(s.instant) + hours * 3_600_000) });
  }

  private setOrientation(zen: boolean): void {
    this.zenithUp = zen;
    this.orientButtons.forEach((b, i) => b.setAttribute('aria-pressed', String((i === 1) === zen)));
    this.disc.redraw();
  }

  private async refresh(force = false): Promise<void> {
    this.stale = false;
    const s = this.store.get();
    const seq = ++this.seq;
    this.status.textContent = 'Computing…';
    try {
      if (!this.features) {
        this.features = await this.backend.sky.moonFeatures();
        this.mask = new MareMask(this.features);
        const names = this.features.craters.map((c) => c.name).sort((a, b) => a.localeCompare(b));
        if (!names.includes(this.crater)) this.crater = names.includes('Copernicus') ? 'Copernicus' : names[0] ?? '';
        replaceChildren(this.craterSelect, ...names.map((n) => h('option', { value: n, selected: n === this.crater }, n)));
      }
      const p: MoonViewParams = { instant: s.instant, location: s.location, ephemerisId: s.ephemerisId, crater: this.crater || null, includeTrack: true };
      const v = await this.backend.sky.moonView(p);
      if (seq !== this.seq) return;
      this.view = v;
      this.status.textContent = v.ephemeris;
      this.renderData();
      this.disc.redraw();
      this.graph.redraw();
    } catch (err) {
      if (seq !== this.seq) return;
      this.status.textContent = `Error: ${BackendError.from(err).message}`;
    }
    void force;
  }

  // --- Data panel ------------------------------------------------------------------

  private renderData(): void {
    const v = this.view!;
    const tz = this.store.get().tzOffsetHours;
    const t = (x: UtDateTime | null) => (x ? `${dateHm(utMs(x), tz)}` : '—');
    const p = v.topocentricPhysical ?? v.geocentric;
    const f = (x: number, d = 2) => x.toFixed(d);
    const sgn = (x: number, d = 2) => `${x >= 0 ? '+' : '−'}${Math.abs(x).toFixed(d)}`;
    const rows: Row[] = [
      ['Time', `${dateHm(utMs(v.instant), tz)} ${zoneLabel(tz)}`],
      ['Phase', v.phaseName],
      ['Illuminated', `${f(v.illuminatedFraction * 100, 1)} %`],
      ['Age', `${f(v.ageDays)} days (new Moon ${t(v.lastNewMoon)})`],
      ['Next phases', `First quarter ${t(v.nextFirstQuarter)} · Full ${t(v.nextFullMoon)} · Last quarter ${t(v.nextLastQuarter)} · New ${t(v.nextNewMoon)}`],
      ['Elongation, phase angle', `${f(v.elongationDeg)}°, ${f(v.phaseAngleDeg)}°`],
      ['Distance', `${Math.round(v.distanceKm).toLocaleString('en')} km${v.topocentric ? ` (geocentric ${Math.round(v.geocentricDistanceKm).toLocaleString('en')} km)` : ''}`],
      ['Apparent diameter', `${f(v.apparentDiameterArcmin)}′`],
      ['Libration l, b', `${sgn(p.libration.lonDeg)}°, ${sgn(p.libration.latDeg)}° (optical ${sgn(p.libration.opticalLonDeg)}°, ${sgn(p.libration.opticalLatDeg)}°; physical ${sgn(p.libration.physicalLonDeg, 3)}°, ${sgn(p.libration.physicalLatDeg, 3)}°)`],
      ['Axis position angle P', `${f(p.axisPositionAngleDeg)}°`],
      ['Bright limb χ', `${f(v.brightLimbDeg, 1)}°${v.brightLimbZenithDeg === null ? '' : ` (from zenith ${f(v.brightLimbZenithDeg, 1)}°)`}`],
      ['Parallactic angle q', v.parallacticDeg === null ? '—' : `${sgn(v.parallacticDeg, 1)}°`],
      ['Sun selenographic', `colongitude ${f(p.colongitudeDeg)}°, latitude ${sgn(p.sunSelenographicLatDeg)}°`],
      ['Altitude, azimuth', v.altDeg === null || v.azDeg === null ? '—' : `${sgn(v.altDeg, 1)}°, ${f(v.azDeg, 1)}°`],
    ];
    const c = v.crater;
    replaceChildren(
      this.data,
      kv(rows),
      h('p', { class: 'hint' }, v.topocentric ? 'Librations, P and illumination as seen from the app location (topocentric).' : 'Geocentric values.'),
      h('div', { class: 'moon-crater' }, h('label', { class: 'inline' }, 'Crater ', this.craterSelect)),
      c
        ? kv([
            ['Sun altitude', `${sgn(c.sunAltDeg, 1)}° (${c.sunAltDeg > 0 ? 'day' : 'night'})`],
            ['Next sunrise', t(c.nextSunrise)],
            ['Next sunset', t(c.nextSunset)],
          ])
        : null,
    );
  }

  // --- Disc ------------------------------------------------------------------------------

  private drawDisc(ctx: CanvasRenderingContext2D, w: number, hh: number): void {
    const v = this.view;
    const text = cssVar('--text', '#ddd');
    const muted = cssVar('--text-muted', '#999');
    ctx.fillStyle = '#05070d';
    ctx.fillRect(0, 0, w, hh);
    if (!v) {
      label(ctx, 'Computing…', w / 2, hh / 2, '#ccd', 'center');
      return;
    }
    const disc = new Disc(v.disc, this.zenithUp);
    const cx = w / 2;
    const cy = hh / 2;
    const R = Math.max(40, Math.min(w, hh) / 2 / 1.5);
    const dpr = window.devicePixelRatio || 1;
    const n = Math.max(64, Math.min(640, Math.round(2 * R * dpr)));
    const key = [n, this.showMaria, disc.l, disc.b, disc.sunLon, disc.sunLat, disc.rot].map((x) => (typeof x === 'number' ? x.toFixed(5) : String(x))).join(',');
    if (this.texture?.key !== key) {
      const c = new OffscreenCanvas(n, n);
      c.getContext('2d')!.putImageData(rasterize(disc, n, this.showMaria ? this.mask : null), 0, 0);
      this.texture = { key, canvas: c };
    }
    ctx.imageSmoothingEnabled = true;
    ctx.drawImage(this.texture.canvas, cx - R, cy - R, 2 * R, 2 * R);

    const toScreen = (u: number, vv: number): [number, number] => [cx + u * R, cy - vv * R];
    const path = (pts: [number, number][], color: string, width: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = width;
      ctx.beginPath();
      let pen = false;
      for (const [lat, lon] of pts) {
        const [u, vv, z] = disc.project(lon, lat);
        if (z <= 0) {
          pen = false;
          continue;
        }
        const p = toScreen(u, vv);
        if (pen) ctx.lineTo(...p);
        else ctx.moveTo(...p);
        pen = true;
      }
      ctx.stroke();
    };
    const f = this.features;
    if (f && this.showMaria) {
      for (const m of f.maria) {
        const pts: [number, number][] = [];
        for (let i = 0; i < m.outline.length; i += 2) pts.push([m.outline[i]! * DEG, m.outline[i + 1]! * DEG]);
        path(pts, 'rgb(110 170 255 / 0.55)', 1);
        if (this.showLabels) {
          const [u, vv, z] = disc.project(m.lonDeg * DEG, m.latDeg * DEG);
          if (z > 0.25) label(ctx, m.name.replace(/^(Mare|Oceanus) /, ''), ...toScreen(u, vv), 'rgb(160 205 255 / 0.9)', 'center', 'middle', 10);
        }
      }
    }
    if (f && this.showCraters) {
      for (const c of f.craters) {
        const sel = c.name === this.crater;
        const color = sel ? 'rgb(255 170 40)' : 'rgb(255 120 90 / 0.8)';
        const r = Math.max(0.012, c.diameterKm / 2 / f.moonRadiusKm);
        path(smallCircle(c.latDeg * DEG, c.lonDeg * DEG, r, 24), color, sel ? 2 : 1);
        const [u, vv, z] = disc.project(c.lonDeg * DEG, c.latDeg * DEG);
        if (this.showLabels && z > 0.08) {
          const [x, y] = toScreen(u, vv);
          label(ctx, c.name, x + 5, y, color, 'left', 'middle', 10);
        }
      }
    }
    // Mean centre (0°, 0°) and the lunar north pole.
    const green = 'rgb(120 230 140)';
    const [u0, v0, z0] = disc.project(0, 0);
    if (z0 > 0) {
      const [x, y] = toScreen(u0, v0);
      ctx.strokeStyle = green;
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.moveTo(x - 5, y);
      ctx.lineTo(x + 5, y);
      ctx.moveTo(x, y - 5);
      ctx.lineTo(x, y + 5);
      ctx.stroke();
    }
    const su = -Math.sin(disc.rot);
    const sv = Math.cos(disc.rot);
    ctx.strokeStyle = green;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(...toScreen(su * 1.02, sv * 1.02));
    ctx.lineTo(...toScreen(su * 1.12, sv * 1.12));
    ctx.stroke();
    label(ctx, 'Np', ...toScreen(su * 1.2, sv * 1.2), green, 'center');
    // Sky directions (screen angles anticlockwise from up).
    const q = v.parallacticDeg ?? 0;
    const offset = this.zenithUp && v.parallacticDeg !== null ? -q : 0;
    const arrow = (angleDeg: number, text: string, color: string) => {
      const dx = -Math.sin(angleDeg * DEG);
      const dy = -Math.cos(angleDeg * DEG);
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(cx + dx * R * 1.25, cy + dy * R * 1.25);
      ctx.lineTo(cx + dx * R * 1.38, cy + dy * R * 1.38);
      ctx.stroke();
      label(ctx, text, cx + dx * R * 1.46, cy + dy * R * 1.46, color, 'center', 'middle', 12);
    };
    if (this.zenithUp && v.parallacticDeg !== null) arrow(0, 'Zenith', muted);
    arrow(offset, 'N', text);
    arrow(offset + 90, 'E', text);
    label(ctx, 'Naked-eye view (east left of north). Green cross: mean centre (0°, 0°); Np: lunar north pole. Mare outlines are schematic.', 8, hh - 10, 'rgb(200 205 220 / 0.75)', 'left', 'middle', 10);
  }

  // --- Libration graph --------------------------------------------------------------

  private drawGraph(ctx: CanvasRenderingContext2D, w: number, hh: number): void {
    const tr = this.view?.librationTrack;
    const text = cssVar('--text', '#ddd');
    const muted = cssVar('--text-muted', '#999');
    if (!tr) return;
    const pad = 30;
    const size = Math.min(w - 2 * pad, hh - 2 * pad);
    const cx = w / 2;
    const cy = hh / 2;
    const k = size / 18; // px per degree, ±9°
    const P = (l: number, b: number): [number, number] => [cx + l * k, cy - b * k];
    ctx.strokeStyle = 'rgb(128 128 128 / 0.3)';
    ctx.lineWidth = 0.6;
    for (let d = -8; d <= 8; d += 2) {
      ctx.beginPath();
      ctx.moveTo(...P(d, -9));
      ctx.lineTo(...P(d, 9));
      ctx.moveTo(...P(-9, d));
      ctx.lineTo(...P(9, d));
      ctx.stroke();
    }
    ctx.strokeStyle = 'rgb(128 128 128 / 0.7)';
    ctx.beginPath();
    ctx.moveTo(...P(-9, 0));
    ctx.lineTo(...P(9, 0));
    ctx.moveTo(...P(0, -9));
    ctx.lineTo(...P(0, 9));
    ctx.stroke();
    for (const d of [-8, -4, 4, 8]) {
      label(ctx, `${d > 0 ? '+' : ''}${d}°`, ...P(d, -9.6), muted, 'center', 'middle', 9);
      label(ctx, `${d > 0 ? '+' : ''}${d}°`, P(-9, d)[0] - 4, P(-9, d)[1], muted, 'right', 'middle', 9);
    }
    label(ctx, 'l (longitude) →', P(9, 0)[0], P(9, 0)[1] - 9, muted, 'right', 'middle', 10);
    label(ctx, 'b ↑', P(0, 9)[0] + 4, P(0, 9)[1] + 4, muted, 'left', 'middle', 10);
    ctx.strokeStyle = cssVar('--accent', '#4ade80');
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    tr.points.forEach(([l, b], i) => (i === 0 ? ctx.moveTo(...P(l, b)) : ctx.lineTo(...P(l, b))));
    ctx.stroke();
    const dot = (p: [number, number], color: string, text2: string) => {
      const [x, y] = P(...p);
      ctx.beginPath();
      ctx.arc(x, y, 4.5, 0, 2 * Math.PI);
      ctx.fillStyle = color;
      ctx.fill();
      ctx.strokeStyle = '#000';
      ctx.lineWidth = 0.8;
      ctx.stroke();
      label(ctx, text2, x + 7, y, text, 'left', 'middle', 10);
    };
    dot(tr.points[0]!, 'rgb(90 90 90)', 'New');
    dot(tr.full, 'rgb(235 235 235)', 'Full');
    dot(tr.now, 'rgb(230 160 40)', 'now');
    const tz = this.store.get().tzOffsetHours;
    label(ctx, `New Moon ${dateHm(utMs(tr.newMoon), tz)} → ${dateHm(utMs(tr.nextNewMoon), tz)} (geocentric)`, 4, hh - 8, muted, 'left', 'middle', 10);
  }
}
