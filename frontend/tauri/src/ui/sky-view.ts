// Sky tab: the evening (or morning) sky of a crescent sighting.
//
// Three views share a time slider spanning sunset → moonset (moonrise →
// sunrise for the morning crescent), with play:
//
// * Horizon: a panorama of the western (eastern) sky with twilight
//   shading, the horizon and compass points, the Sun below the horizon, the
//   Moon phased and oriented in the zenith frame (χ − q), the Sun's and
//   Moon's paths with 10-minute dots, and an ARCV/DAZ annotation. Wheel
//   zooms, drag pans, double-click resets.
// * All-sky: the whole sky, zenith at the centre, north up, east left.
// * Star chart: RA/Dec around the Moon with the ecliptic and the horizon.
//
// All the astronomy comes from the `sky_scene` service (samples every two
// minutes); this module interpolates and draws.

import './sky.css';

import { BackendError, type Backend, type SkySample, type SkyScene, type SkySceneParams } from '../backend';
import { changed, localDate, type Store } from '../state';
import { debounce, h, replaceChildren } from './dom';
import { formatDate, formatValue, valueSymbol } from './format';
import {
  MOON_COLOR,
  SUN_COLOR,
  Series,
  Stage,
  altAzToRaDec,
  compassPoint,
  drawMoon,
  drawStar,
  drawSun,
  hm,
  label,
  norm180,
  norm360,
  polyline,
  rgbCss,
  rotateCcw,
  separation,
  skyColor,
  starAltAz,
  starVisibility,
  utMs,
  TimeSlider,
  zoneLabel,
} from './sky-common';

type ViewKind = 'horizon' | 'allsky' | 'chart';

const VIEW_LABELS: Record<ViewKind, string> = { horizon: 'Horizon', allsky: 'All-sky', chart: 'Star chart' };

function chip(rgb: [number, number, number], code: string): HTMLElement {
  const el = h('span', { class: 'zone-chip' }, code);
  el.style.background = `rgb(${rgb.join(' ')})`;
  const lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
  el.style.color = lum > 140 ? '#111' : '#fff';
  return el;
}

/** Circular mean of two azimuths, degrees. */
function meanAz(a: number, b: number): number {
  return norm360(a + norm180(b - a) / 2);
}

export class SkyView {
  readonly el = h('div', { class: 'sky-view' });
  private readonly stageEl = h('div', { class: 'sky-stage' });
  private readonly stage: Stage;
  private readonly slider: TimeSlider;
  private readonly status = h('span', { class: 'status' });
  private readonly summary = h('div', { class: 'sky-summary' });
  private readonly readout = h('div', { class: 'sky-readout' });
  private readonly hover = h('div', { class: 'sky-hover', hidden: true });
  private readonly viewButtons: Record<ViewKind, HTMLButtonElement>;

  private active = false;
  private stale = true;
  private seq = 0;
  private lastParams = '';
  private scene: SkyScene | null = null;
  private series: Series<SkySample> | null = null;
  private timeMs = 0;

  private view: ViewKind = 'horizon';
  private showStars = true;
  private showPaths = true;
  private showLabels = true;
  private moonScale = 6;
  // Horizon view state.
  private spanDeg = 100;
  private azOffset = 0;
  private altOffset = 0;
  private autoAz = 270;
  private drag: { x: number; y: number } | null = null;
  /** Unproject a screen point to (alt, az) in the current view, if any. */
  private unproject: ((x: number, y: number) => { alt: number; az: number } | null) | null = null;

  constructor(
    private readonly backend: Backend,
    private readonly store: Store,
  ) {
    this.stage = new Stage(this.stageEl, (ctx, w, hh) => this.draw(ctx, w, hh));
    this.stageEl.appendChild(this.hover);
    this.slider = new TimeSlider(
      (minute) => this.setTime(minute),
      (minute) => this.formatSliderTime(minute),
    );
    const btn = (kind: ViewKind) =>
      h('button', { type: 'button', class: 'segment', 'aria-pressed': String(kind === this.view), onclick: () => this.setView(kind) }, VIEW_LABELS[kind]);
    this.viewButtons = { horizon: btn('horizon'), allsky: btn('allsky'), chart: btn('chart') };
    const check = (text: string, value: boolean, set: (v: boolean) => void) => {
      const input = h('input', { type: 'checkbox', checked: value, onchange: (e: Event) => {
        set((e.target as HTMLInputElement).checked);
        this.stage.redraw();
      } });
      return h('label', { class: 'inline' }, input, text);
    };
    const scale = h(
      'select',
      {
        'aria-label': 'Moon and Sun size',
        onchange: (e: Event) => {
          this.moonScale = Number((e.target as HTMLSelectElement).value);
          this.stage.redraw();
        },
      },
      ...[1, 3, 6, 10].map((v) => h('option', { value: String(v), selected: v === this.moonScale }, v === 1 ? 'True size' : `Discs ×${v}`)),
    );
    const toolbar = h(
      'div',
      { class: 'toolbar' },
      h('div', { class: 'segmented', role: 'group', 'aria-label': 'View' }, ...Object.values(this.viewButtons)),
      check('Stars', this.showStars, (v) => (this.showStars = v)),
      check('Paths', this.showPaths, (v) => (this.showPaths = v)),
      check('Labels', this.showLabels, (v) => (this.showLabels = v)),
      scale,
      this.status,
    );
    this.el.append(toolbar, this.summary, this.stageEl, this.slider.el, this.readout);
    this.bindPointer();

    const refresh = debounce(() => void this.refresh(), 150);
    this.store.subscribe((s, prev) => {
      if (changed(s, prev, ['instant', 'tzOffsetHours', 'location', 'crescent', 'criterionId', 'ephemerisId'])) {
        this.stale = true;
        if (this.active) refresh();
      }
    });
  }

  setActive(active: boolean): void {
    this.active = active;
    if (!active) this.slider.stop();
    if (active && this.stale) void this.refresh();
    if (active) this.stage.redraw();
  }

  private params(): SkySceneParams {
    const s = this.store.get();
    return {
      date: localDate(s),
      tzOffsetHours: s.tzOffsetHours,
      location: s.location,
      crescent: s.crescent,
      criterionId: s.criterionId,
      ephemerisId: s.ephemerisId,
    };
  }

  private async refresh(): Promise<void> {
    this.stale = false;
    const p = this.params();
    const key = JSON.stringify(p);
    if (key === this.lastParams && this.scene) return;
    const seq = ++this.seq;
    this.status.textContent = 'Computing…';
    try {
      const scene = await this.backend.sky.skyScene(p);
      if (seq !== this.seq) return;
      this.lastParams = key;
      this.setScene(scene);
      this.status.textContent = scene.ephemeris;
    } catch (err) {
      if (seq !== this.seq) return;
      this.status.textContent = `Error: ${BackendError.from(err).message}`;
    }
  }

  private setScene(scene: SkyScene): void {
    this.scene = scene;
    this.series = new Series(utMs(scene.samplesStart), scene.stepMinutes * 60_000, scene.samples);
    const start = utMs(scene.start);
    this.timeMs = utMs(scene.defaultTime);
    const at = this.series.at(this.timeMs);
    this.autoAz = scene.sunEvent ? meanAz(at.sunAzDeg, at.moonAzDeg) : scene.crescent === 'evening' ? 270 : 90;
    this.azOffset = 0;
    this.altOffset = 0;
    this.slider.setRange(0, (utMs(scene.end) - start) / 60_000, (this.timeMs - start) / 60_000);
    this.renderSummary();
    this.renderReadout();
    this.stage.redraw();
  }

  private setView(kind: ViewKind): void {
    this.view = kind;
    for (const [k, b] of Object.entries(this.viewButtons)) b.setAttribute('aria-pressed', String(k === kind));
    this.hover.hidden = true;
    this.stage.redraw();
  }

  private setTime(minute: number): void {
    if (!this.scene) return;
    this.timeMs = utMs(this.scene.start) + minute * 60_000;
    this.renderReadout();
    this.stage.redraw();
  }

  private formatSliderTime(minute: number): string {
    if (!this.scene) return '';
    const tz = this.scene.tzOffsetHours;
    return `${hm(utMs(this.scene.start) + minute * 60_000, tz)} ${zoneLabel(tz)}`;
  }

  // --- Text ---------------------------------------------------------------------

  private renderSummary(): void {
    const sc = this.scene!;
    const tz = sc.tzOffsetHours;
    const evening = sc.crescent === 'evening';
    const t = (x: typeof sc.sunEvent) => (x ? hm(utMs(x), tz) : '—');
    const g = sc.geometry;
    const parts: (string | Node)[] = [
      h('strong', {}, `${formatDate(sc.date)} ${evening ? 'evening' : 'morning'}`),
      ` · ${evening ? 'Sunset' : 'Sunrise'} ${t(sc.sunEvent)} · ${evening ? 'Moonset' : 'Moonrise'} ${t(sc.moonEvent)} · Best ${t(sc.bestTime)} ${zoneLabel(tz)}`,
    ];
    if (g) {
      parts.push(` · Lag ${g.lagMinutes === null ? '—' : `${g.lagMinutes.toFixed(0)} min`} · Age ${g.ageHours.toFixed(1)} h`);
    }
    if (sc.verdict) {
      const v = sc.verdict;
      parts.push(' · ', chip(v.rgb, v.code), ` ${sc.criterionName}: ${v.description}`);
      if (v.value !== null) parts.push(` (${valueSymbol(sc.criterionId)} = ${formatValue(v.value)})`);
    }
    replaceChildren(this.summary, h('div', {}, ...parts), h('div', { class: 'hint' }, sc.statusText));
  }

  private renderReadout(): void {
    const sc = this.scene;
    if (!sc || !this.series) return;
    const s = this.series.at(this.timeMs);
    const tz = sc.tzOffsetHours;
    const f = (v: number, d = 1) => v.toFixed(d);
    replaceChildren(
      this.readout,
      h('span', {}, h('strong', {}, `${hm(this.timeMs, tz)} ${zoneLabel(tz)}`)),
      h('span', {}, `Sun alt ${f(s.sunAltDeg)}° az ${f(s.sunAzDeg)}°`),
      h('span', {}, `Moon alt ${f(s.moonAltDeg)}° az ${f(s.moonAzDeg)}° ${compassPoint(s.moonAzDeg)}`),
      h('span', {}, `ARCL ${f(s.arclDeg)}° · ARCV ${f(s.arcvDeg)}° · DAZ ${s.dazDeg >= 0 ? '+' : ''}${f(s.dazDeg)}°`),
      h('span', {}, `W ${s.widthArcmin === null ? '—' : `${f(s.widthArcmin, 2)}′`} · ${f(s.illuminatedFraction * 100, 1)} % lit`),
      h('span', {}, `Bright limb χ ${f(s.brightLimbDeg, 0)}°, from zenith ${f(s.limbZenithDeg, 0)}°`),
    );
  }

  // --- Pointer ---------------------------------------------------------------------

  private bindPointer(): void {
    const c = this.stage.canvas;
    c.addEventListener('wheel', (e) => {
      if (this.view !== 'horizon') return;
      e.preventDefault();
      this.spanDeg = Math.max(10, Math.min(360, this.spanDeg * Math.exp(e.deltaY / 400)));
      this.stage.redraw();
    }, { passive: false });
    c.addEventListener('pointerdown', (e) => {
      if (this.view !== 'horizon') return;
      this.drag = { x: e.clientX, y: e.clientY };
      c.setPointerCapture(e.pointerId);
    });
    c.addEventListener('pointerup', () => (this.drag = null));
    c.addEventListener('pointermove', (e) => {
      const r = c.getBoundingClientRect();
      if (this.drag && this.view === 'horizon') {
        const ppd = r.width / this.spanDeg;
        this.azOffset -= (e.clientX - this.drag.x) / ppd;
        this.altOffset = Math.max(-80, Math.min(60, this.altOffset + (e.clientY - this.drag.y) / ppd));
        this.drag = { x: e.clientX, y: e.clientY };
        this.stage.redraw();
      }
      const p = this.unproject?.(e.clientX - r.left, e.clientY - r.top) ?? null;
      this.hover.hidden = !p;
      if (p) this.hover.textContent = `alt ${p.alt >= 0 ? '+' : ''}${p.alt.toFixed(1)}°  az ${norm360(p.az).toFixed(1)}° ${compassPoint(p.az)}`;
    });
    c.addEventListener('pointerleave', () => (this.hover.hidden = true));
    c.addEventListener('dblclick', () => {
      this.spanDeg = 100;
      this.azOffset = 0;
      this.altOffset = 0;
      this.stage.redraw();
    });
  }

  // --- Drawing ------------------------------------------------------------------------

  private draw(ctx: CanvasRenderingContext2D, w: number, hh: number): void {
    const sc = this.scene;
    if (!sc || !this.series) {
      ctx.fillStyle = '#0b1020';
      ctx.fillRect(0, 0, w, hh);
      label(ctx, 'Computing the sky…', w / 2, hh / 2, '#ccd', 'center');
      return;
    }
    const s = this.series.at(this.timeMs);
    if (this.view === 'horizon') this.drawHorizon(ctx, w, hh, sc, s);
    else if (this.view === 'allsky') this.drawAllSky(ctx, w, hh, sc, s);
    else this.drawChart(ctx, w, hh, sc, s);
  }

  /** 10-minute marks along the sampled paths: [ms, sample]. */
  private marks(): [number, SkySample][] {
    const series = this.series!;
    const step = 10 * 60_000;
    const out: [number, SkySample][] = [];
    for (let t = Math.ceil(series.startMs / step) * step; t <= series.endMs; t += step) out.push([t, series.at(t)]);
    return out;
  }

  private drawStars(ctx: CanvasRenderingContext2D, sc: SkyScene, s: SkySample, alpha: number, project: (alt: number, az: number, ra: number, dec: number) => [number, number] | null): void {
    if (!this.showStars || alpha < 0.02) return;
    for (const st of sc.stars) {
      const a = starAltAz(st.raDeg, st.decDeg, sc.location.latDeg, s.lstDeg);
      const p = project(a.alt, a.az, st.raDeg, st.decDeg);
      if (!p) continue;
      drawStar(ctx, p[0], p[1], st.vmag, st.bv, alpha);
      if (this.showLabels && st.vmag < 1.6 && st.name) label(ctx, st.name, p[0] + 5, p[1], `rgb(220 225 240 / ${0.7 * alpha})`, 'left', 'middle', 10);
    }
  }

  private drawHorizon(ctx: CanvasRenderingContext2D, w: number, hh: number, sc: SkyScene, s: SkySample): void {
    const ppd = w / this.spanDeg;
    const centerAz = this.autoAz + this.azOffset;
    const horizonY = hh * 0.74 + this.altOffset * ppd;
    const project = (alt: number, az: number): [number, number] => [w / 2 + norm180(az - centerAz) * ppd, horizonY - alt * ppd];
    this.unproject = (x, y) => ({ alt: (horizonY - y) / ppd, az: centerAz + (x - w / 2) / ppd });

    // Sky: a coarse colour grid, smoothly upscaled.
    const skyH = Math.max(0, Math.min(hh, horizonY));
    if (skyH > 0) {
      const gw = 64;
      const gh = 32;
      const off = new OffscreenCanvas(gw, gh);
      const octx = off.getContext('2d')!;
      const img = octx.createImageData(gw, gh);
      for (let j = 0; j < gh; j++) {
        for (let i = 0; i < gw; i++) {
          const x = ((i + 0.5) / gw) * w;
          const y = ((j + 0.5) / gh) * skyH;
          const alt = (horizonY - y) / ppd;
          const az = centerAz + (x - w / 2) / ppd;
          const c = skyColor(s.sunAltDeg, alt, separation(alt, az, s.sunAltDeg, s.sunAzDeg));
          const o = (j * gw + i) * 4;
          img.data[o] = c[0] * 255;
          img.data[o + 1] = c[1] * 255;
          img.data[o + 2] = c[2] * 255;
          img.data[o + 3] = 255;
        }
      }
      octx.putImageData(img, 0, 0);
      ctx.imageSmoothingEnabled = true;
      ctx.drawImage(off, 0, 0, w, skyH);
    }

    // Altitude grid.
    ctx.strokeStyle = 'rgb(255 255 255 / 0.11)';
    ctx.lineWidth = 0.5;
    for (let alt = -80; alt <= 80; alt += 10) {
      if (alt === 0) continue;
      const y = horizonY - alt * ppd;
      if (y < 0 || y > hh) continue;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
      label(ctx, `${alt}°`, w - 4, y - 6, 'rgb(255 255 255 / 0.5)', 'right', 'middle', 10);
    }

    this.drawStars(ctx, sc, s, starVisibility(s.sunAltDeg), (alt, az) => {
      if (alt < -0.5) return null;
      const p = project(alt, az);
      return p[0] >= 0 && p[0] <= w && p[1] >= 0 && p[1] <= hh ? p : null;
    });

    // Paths with 10-minute dots.
    if (this.showPaths) {
      const series = this.series!;
      for (const [color, alt, az] of [
        [SUN_COLOR, 'sunAltDeg', 'sunAzDeg'],
        [MOON_COLOR, 'moonAltDeg', 'moonAzDeg'],
      ] as const) {
        ctx.strokeStyle = color;
        ctx.globalAlpha = 0.75;
        ctx.lineWidth = 1.3;
        polyline(ctx, series.samples.map((x) => project(x[alt], x[az])), w / 2);
        ctx.globalAlpha = 1;
        for (const [t, x] of this.marks()) {
          const [px, py] = project(x[alt], x[az]);
          if (px < -10 || px > w + 10 || py < -10 || py > hh + 10) continue;
          ctx.beginPath();
          ctx.arc(px, py, 2.2, 0, 2 * Math.PI);
          ctx.fillStyle = color;
          ctx.fill();
          const minute = new Date(t + sc.tzOffsetHours * 3_600_000).getUTCMinutes();
          if (this.showLabels && minute % 30 === 0) label(ctx, hm(t, sc.tzOffsetHours), px + 5, py - 7, color, 'left', 'middle', 10);
        }
      }
    }

    // Ground over what is below the horizon.
    if (horizonY < hh) {
      const gc = skyColor(s.sunAltDeg, 0, 180);
      ctx.fillStyle = rgbCss([gc[0] * 0.25 + 0.05, gc[1] * 0.3 + 0.065, gc[2] * 0.2 + 0.04], 0.84);
      ctx.fillRect(0, Math.max(0, horizonY), w, hh - Math.max(0, horizonY));
      ctx.strokeStyle = 'rgb(120 140 110)';
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(0, horizonY);
      ctx.lineTo(w, horizonY);
      ctx.stroke();
    }
    // Azimuth ticks and compass points.
    const tickStep = this.spanDeg > 200 ? 30 : this.spanDeg > 60 ? 10 : 5;
    for (let az = Math.ceil((centerAz - this.spanDeg / 2) / tickStep) * tickStep; az <= centerAz + this.spanDeg / 2; az += tickStep) {
      const [x] = project(0, az);
      const a = norm360(az);
      const cardinal = a % 45 === 0;
      ctx.strokeStyle = 'rgb(200 210 190 / 0.8)';
      ctx.beginPath();
      ctx.moveTo(x, horizonY);
      ctx.lineTo(x, horizonY + (cardinal ? 9 : 5));
      ctx.stroke();
      if (cardinal) label(ctx, compassPoint(a), x, horizonY + 18, '#e8ecd8', 'center', 'middle', 12);
      else if (tickStep >= 10 || a % 10 === 0) label(ctx, `${a.toFixed(0)}°`, x, horizonY + 16, 'rgb(220 225 210 / 0.7)', 'center', 'middle', 9);
    }

    // Sun and Moon.
    const sunP = project(s.sunAltDeg, s.sunAzDeg);
    const moonP = project(s.moonAltDeg, s.moonAzDeg);
    const sunR = Math.max(5, (s.sunSemiDiameterArcmin / 60) * ppd * this.moonScale);
    const moonR = Math.max(4, (s.moonSemiDiameterArcmin / 60) * ppd * this.moonScale);
    // ARCV / DAZ: Sun → (Moon's azimuth, Sun's altitude) → Moon.
    if (sc.geometry) {
      const corner: [number, number] = [moonP[0], sunP[1]];
      ctx.save();
      ctx.setLineDash([5, 4]);
      ctx.strokeStyle = 'rgb(255 255 255 / 0.6)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(...sunP);
      ctx.lineTo(...corner);
      ctx.lineTo(...moonP);
      ctx.stroke();
      ctx.restore();
      label(ctx, `DAZ ${s.dazDeg >= 0 ? '+' : ''}${s.dazDeg.toFixed(1)}°`, (sunP[0] + corner[0]) / 2, sunP[1] + sunR + 10, '#fff', 'center');
      label(ctx, `ARCV ${s.arcvDeg.toFixed(1)}°`, corner[0] + 7, (corner[1] + moonP[1]) / 2, '#fff', 'left');
    }
    drawSun(ctx, sunP[0], sunP[1], sunR, s.sunAltDeg < 0);
    // The bright limb from the zenith (straight up here), χ − q.
    drawMoon(ctx, moonP[0], moonP[1], moonR, s.illuminatedFraction, rotateCcw([0, -1], s.limbZenithDeg));
    if (this.showLabels) {
      label(ctx, this.moonScale === 1 ? 'Moon' : `Moon (×${this.moonScale})`, moonP[0] + moonR + 6, moonP[1] - moonR - 2, '#e6e6d2');
      label(ctx, 'Sun', sunP[0] + sunR + 6, sunP[1] - sunR, '#ffd59a');
    }
  }

  private drawAllSky(ctx: CanvasRenderingContext2D, w: number, hh: number, sc: SkyScene, s: SkySample): void {
    const cx = w / 2;
    const cy = hh / 2;
    const R = Math.max(40, Math.min(w, hh) / 2 - 30);
    const project = (alt: number, az: number): [number, number] => {
      const r = (R * (90 - alt)) / 90;
      return [cx - r * Math.sin((az * Math.PI) / 180), cy - r * Math.cos((az * Math.PI) / 180)];
    };
    this.unproject = (x, y) => {
      const dx = cx - x;
      const dy = cy - y;
      const r = Math.hypot(dx, dy);
      if (r > R) return null;
      return { alt: 90 - (90 * r) / R, az: norm360((Math.atan2(dx, dy) * 180) / Math.PI) };
    };
    ctx.fillStyle = '#0a0d16';
    ctx.fillRect(0, 0, w, hh);
    // Sky colour inside the horizon circle.
    const n = 96;
    const off = new OffscreenCanvas(n, n);
    const octx = off.getContext('2d')!;
    const img = octx.createImageData(n, n);
    for (let j = 0; j < n; j++) {
      for (let i = 0; i < n; i++) {
        const x = cx - R + ((i + 0.5) / n) * 2 * R;
        const y = cy - R + ((j + 0.5) / n) * 2 * R;
        const p = this.unproject(x, y);
        const o = (j * n + i) * 4;
        if (!p) continue;
        const c = skyColor(s.sunAltDeg, p.alt, separation(p.alt, p.az, s.sunAltDeg, s.sunAzDeg));
        img.data[o] = c[0] * 255;
        img.data[o + 1] = c[1] * 255;
        img.data[o + 2] = c[2] * 255;
        img.data[o + 3] = 255;
      }
    }
    octx.putImageData(img, 0, 0);
    ctx.save();
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, 2 * Math.PI);
    ctx.clip();
    ctx.drawImage(off, cx - R, cy - R, 2 * R, 2 * R);
    ctx.restore();
    // Altitude rings and meridians.
    ctx.strokeStyle = 'rgb(255 255 255 / 0.14)';
    ctx.lineWidth = 0.6;
    for (const alt of [30, 60]) {
      ctx.beginPath();
      ctx.arc(cx, cy, (R * (90 - alt)) / 90, 0, 2 * Math.PI);
      ctx.stroke();
    }
    for (let az = 0; az < 360; az += 45) {
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(...project(0, az));
      ctx.stroke();
    }
    ctx.strokeStyle = 'rgb(140 160 130)';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, 2 * Math.PI);
    ctx.stroke();
    for (let az = 0; az < 360; az += 45) {
      const p = project(-14, az);
      label(ctx, compassPoint(az), p[0], p[1], '#e8ecd8', 'center', 'middle', az % 90 === 0 ? 13 : 10);
    }
    this.drawStars(ctx, sc, s, starVisibility(s.sunAltDeg), (alt, az) => (alt < 0 ? null : project(alt, az)));
    if (this.showPaths) {
      ctx.save();
      ctx.beginPath();
      ctx.arc(cx, cy, R + 40, 0, 2 * Math.PI);
      ctx.clip();
      for (const [color, alt, az] of [
        [SUN_COLOR, 'sunAltDeg', 'sunAzDeg'],
        [MOON_COLOR, 'moonAltDeg', 'moonAzDeg'],
      ] as const) {
        ctx.strokeStyle = color;
        ctx.globalAlpha = 0.75;
        ctx.lineWidth = 1.2;
        polyline(ctx, this.series!.samples.filter((x) => x[alt] > -12).map((x) => project(x[alt], x[az])), R);
        ctx.globalAlpha = 1;
        for (const [, x] of this.marks()) {
          if (x[alt] < -12) continue;
          const p = project(x[alt], x[az]);
          ctx.beginPath();
          ctx.arc(p[0], p[1], 1.8, 0, 2 * Math.PI);
          ctx.fillStyle = color;
          ctx.fill();
        }
      }
      ctx.restore();
    }
    const scale = Math.max(1, this.moonScale);
    const ppd = R / 90;
    const sunP = project(Math.max(-20, s.sunAltDeg), s.sunAzDeg);
    drawSun(ctx, sunP[0], sunP[1], Math.max(4, (s.sunSemiDiameterArcmin / 60) * ppd * scale), s.sunAltDeg < 0);
    const moonP = project(s.moonAltDeg, s.moonAzDeg);
    const zenith: [number, number] = [cx - moonP[0], cy - moonP[1]];
    drawMoon(ctx, moonP[0], moonP[1], Math.max(4, (s.moonSemiDiameterArcmin / 60) * ppd * scale), s.illuminatedFraction, rotateCcw(Math.hypot(...zenith) > 1 ? zenith : [0, -1], s.limbZenithDeg));
    if (this.showLabels) label(ctx, 'Moon', moonP[0] + 8, moonP[1] - 8, '#e6e6d2');
    label(ctx, 'Zenith at the centre · north up, east left', 10, hh - 12, 'rgb(220 225 240 / 0.7)', 'left', 'middle', 11);
  }

  private drawChart(ctx: CanvasRenderingContext2D, w: number, hh: number, sc: SkyScene, s: SkySample): void {
    const spanDeg = 120;
    const ppd = w / spanDeg;
    const ra0 = s.moonRaDeg;
    const dec0 = Math.max(-60, Math.min(60, s.moonDecDeg));
    const project = (ra: number, dec: number): [number, number] => [w / 2 - norm180(ra - ra0) * ppd, hh / 2 - (dec - dec0) * ppd];
    this.unproject = (x, y) => {
      const ra = ra0 - (x - w / 2) / ppd;
      const dec = dec0 + (hh / 2 - y) / ppd;
      if (Math.abs(dec) > 90) return null;
      const p = starAltAz(ra, dec, sc.location.latDeg, s.lstDeg);
      return { alt: p.alt, az: p.az };
    };
    ctx.fillStyle = '#0b1020';
    ctx.fillRect(0, 0, w, hh);
    // RA hours and declination grid.
    ctx.strokeStyle = 'rgb(140 160 220 / 0.18)';
    ctx.lineWidth = 0.6;
    for (let ra = 0; ra < 360; ra += 15) {
      const [x] = project(ra, 0);
      if (x < 0 || x > w) continue;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, hh);
      ctx.stroke();
      label(ctx, `${ra / 15}h`, x, hh - 10, 'rgb(170 185 230 / 0.7)', 'center', 'middle', 10);
    }
    for (let dec = -80; dec <= 80; dec += 10) {
      const [, y] = project(ra0, dec);
      if (y < 0 || y > hh) continue;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
      label(ctx, `${dec >= 0 ? '+' : ''}${dec}°`, 4, y - 6, 'rgb(170 185 230 / 0.7)', 'left', 'middle', 10);
    }
    // Ecliptic (β = 0) and the horizon.
    const eps = (sc.obliquityDeg * Math.PI) / 180;
    const ecl: [number, number][] = [];
    for (let lon = 0; lon <= 360; lon += 2) {
      const l = (lon * Math.PI) / 180;
      const ra = norm360((Math.atan2(Math.cos(eps) * Math.sin(l), Math.cos(l)) * 180) / Math.PI);
      const dec = (Math.asin(Math.sin(eps) * Math.sin(l)) * 180) / Math.PI;
      ecl.push(project(ra, dec));
    }
    ctx.strokeStyle = 'rgb(230 200 90 / 0.55)';
    ctx.lineWidth = 1;
    polyline(ctx, ecl, w / 3);
    const hor: [number, number][] = [];
    for (let az = 0; az <= 360; az += 2) {
      const p = altAzToRaDec(0, az, sc.location.latDeg, s.lstDeg);
      hor.push(project(p.ra, p.dec));
    }
    ctx.save();
    ctx.setLineDash([6, 4]);
    ctx.strokeStyle = 'rgb(130 200 120 / 0.8)';
    polyline(ctx, hor, w / 3);
    ctx.restore();
    if (this.showLabels) {
      const e = ecl.find((p) => p[0] > 20 && p[0] < w - 20 && p[1] > 20 && p[1] < hh - 20);
      if (e) label(ctx, 'ecliptic', e[0] + 4, e[1] - 8, 'rgb(230 200 90 / 0.85)', 'left', 'middle', 10);
      const hp = hor.find((p) => p[0] > w * 0.6 && p[0] < w - 20 && p[1] > 20 && p[1] < hh - 20);
      if (hp) label(ctx, 'horizon', hp[0], hp[1] - 8, 'rgb(130 200 120 / 0.9)', 'left', 'middle', 10);
    }
    this.drawStars(ctx, sc, s, 1, (_alt, _az, ra, dec) => {
      const p = project(ra, dec);
      return p[0] >= 0 && p[0] <= w && p[1] >= 0 && p[1] <= hh ? p : null;
    });
    const scale = Math.max(1, this.moonScale);
    if (this.showPaths) {
      for (const [color, ra, dec] of [
        [SUN_COLOR, 'sunRaDeg', 'sunDecDeg'],
        [MOON_COLOR, 'moonRaDeg', 'moonDecDeg'],
      ] as const) {
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        polyline(ctx, this.series!.samples.map((x) => project(x[ra], x[dec])), w / 3);
      }
    }
    const sunP = project(s.sunRaDeg, s.sunDecDeg);
    drawSun(ctx, sunP[0], sunP[1], Math.max(4, (s.sunSemiDiameterArcmin / 60) * ppd * scale), false);
    const moonP = project(s.moonRaDeg, s.moonDecDeg);
    // North is up here: the bright limb at position angle χ.
    drawMoon(ctx, moonP[0], moonP[1], Math.max(4, (s.moonSemiDiameterArcmin / 60) * ppd * scale), s.illuminatedFraction, rotateCcw([0, -1], s.brightLimbDeg));
    if (this.showLabels) {
      label(ctx, 'Moon', moonP[0] + 10, moonP[1] - 10, '#e6e6d2');
      label(ctx, 'Sun', sunP[0] + 10, sunP[1] - 10, '#ffd59a');
    }
    label(ctx, 'RA/Dec of date · north up, east left', 10, 12, 'rgb(220 225 240 / 0.7)', 'left', 'middle', 11);
  }
}
