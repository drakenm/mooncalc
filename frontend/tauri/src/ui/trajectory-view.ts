// Trajectory tab: one local day of the Sun and Moon, as on mooncalc.org.
//
// * A 24-hour altitude plot with the twilight bands, the Sun's and Moon's
//   altitude curves, rise/transit/set marked and the slider time.
// * A compass seen from above (north up, east right) with the rise and set
//   azimuths, the day arcs above the horizon and the current positions.
// * Readouts at the slider time: positions, distance, illumination, phase
//   name and age, next new and full Moon, shadow of a 1 m object, events.
//
// The data come from the `trajectory_day` service (samples every five
// minutes); the slider interpolates between them.

import './sky.css';

import { BackendError, type Backend, type BodyDayEvents, type TrackSample, type TrajectoryDay, type TrajectoryParams, type UtDateTime } from '../backend';
import { changed, localDate, type Store } from '../state';
import { debounce, h, replaceChildren } from './dom';
import { formatDate } from './format';
import { DEG, MOON_COLOR, SUN_COLOR, Series, Stage, TimeSlider, compassPoint, cssVar, dateHm, drawMoon, hm, label, phaseName, rotateCcw, utMs, zoneLabel } from './sky-common';

type Row = [string, string | Node];

function kv(rows: Row[]): HTMLTableElement {
  return h('table', { class: 'kv' }, h('tbody', {}, ...rows.map(([k, v]) => h('tr', {}, h('th', { scope: 'row' }, k), h('td', {}, v)))));
}

/** Background of the altitude plot by the Sun's altitude: day, civil, nautical, astronomical twilight, night. */
function bandColor(sunAltDeg: number): string {
  if (sunAltDeg > -0.833) return 'rgb(120 170 230 / 0.30)';
  if (sunAltDeg > -6) return 'rgb(90 120 190 / 0.38)';
  if (sunAltDeg > -12) return 'rgb(55 75 140 / 0.48)';
  if (sunAltDeg > -18) return 'rgb(35 45 95 / 0.58)';
  return 'rgb(15 20 45 / 0.68)';
}

export class TrajectoryView {
  readonly el = h('div', { class: 'sky-view trajectory-view' });
  private readonly plotEl = h('div', { class: 'sky-stage traj-plot' });
  private readonly compassEl = h('div', { class: 'sky-stage traj-compass' });
  private readonly plot: Stage;
  private readonly compass: Stage;
  private readonly slider: TimeSlider;
  private readonly status = h('span', { class: 'status' });
  private readonly title = h('div', { class: 'sky-summary' });
  private readonly readouts = h('div', { class: 'traj-readouts' });

  private active = false;
  private stale = true;
  private seq = 0;
  private lastParams = '';
  private day: TrajectoryDay | null = null;
  private series: Series<TrackSample> | null = null;
  private minute = 720;

  constructor(
    private readonly backend: Backend,
    private readonly store: Store,
  ) {
    this.plot = new Stage(this.plotEl, (ctx, w, hh) => this.drawPlot(ctx, w, hh));
    this.compass = new Stage(this.compassEl, (ctx, w, hh) => this.drawCompass(ctx, w, hh));
    this.slider = new TimeSlider(
      (m) => this.setMinute(m),
      (m) => (this.day ? `${hm(utMs(this.day.dayStart) + m * 60_000, this.day.tzOffsetHours)} ${zoneLabel(this.day.tzOffsetHours)}` : ''),
      90,
    );
    this.el.append(
      h('div', { class: 'toolbar' }, this.title, this.status),
      h('div', { class: 'traj-grid' }, this.plotEl, h('div', { class: 'traj-side' }, this.compassEl, this.readouts)),
      this.slider.el,
    );
    this.plot.canvas.addEventListener('pointerdown', (e) => this.seek(e));
    this.plot.canvas.addEventListener('pointermove', (e) => {
      if (e.buttons & 1) this.seek(e);
    });

    const refresh = debounce(() => void this.refresh(), 150);
    this.store.subscribe((s, prev) => {
      if (changed(s, prev, ['instant', 'tzOffsetHours', 'location', 'ephemerisId'])) {
        this.stale = true;
        if (this.active) refresh();
      }
    });
  }

  setActive(active: boolean): void {
    this.active = active;
    if (!active) this.slider.stop();
    if (active && this.stale) void this.refresh();
    if (active) {
      this.plot.redraw();
      this.compass.redraw();
    }
  }

  private plotBox(w: number, hh: number) {
    return { x0: 44, x1: w - 12, y0: 14, y1: hh - 28 };
  }

  private seek(e: PointerEvent): void {
    const r = this.plot.canvas.getBoundingClientRect();
    const b = this.plotBox(r.width, r.height);
    const m = ((e.clientX - r.left - b.x0) / (b.x1 - b.x0)) * 1440;
    if (m < 0 || m > 1440) return;
    this.slider.setRange(0, 1440, m);
    this.setMinute(Math.round(m));
  }

  private params(): TrajectoryParams {
    const s = this.store.get();
    return { date: localDate(s), tzOffsetHours: s.tzOffsetHours, location: s.location, ephemerisId: s.ephemerisId };
  }

  private async refresh(): Promise<void> {
    this.stale = false;
    const p = this.params();
    const key = JSON.stringify(p);
    const s = this.store.get();
    if (key === this.lastParams && this.day) {
      // Same day: follow the app time.
      this.minute = Math.max(0, Math.min(1440, (utMs(s.instant) - utMs(this.day.dayStart)) / 60_000));
      this.slider.setRange(0, 1440, this.minute);
      this.update();
      return;
    }
    const seq = ++this.seq;
    this.status.textContent = 'Computing…';
    try {
      const day = await this.backend.sky.trajectoryDay(p);
      if (seq !== this.seq) return;
      this.lastParams = key;
      this.day = day;
      this.series = new Series(utMs(day.dayStart), day.stepMinutes * 60_000, day.samples);
      this.minute = Math.max(0, Math.min(1440, (utMs(this.store.get().instant) - utMs(day.dayStart)) / 60_000));
      this.slider.setRange(0, 1440, this.minute);
      this.status.textContent = day.ephemeris;
      this.update();
    } catch (err) {
      if (seq !== this.seq) return;
      this.status.textContent = `Error: ${BackendError.from(err).message}`;
    }
  }

  private setMinute(m: number): void {
    this.minute = m;
    this.update();
  }

  private update(): void {
    this.renderReadouts();
    this.plot.redraw();
    this.compass.redraw();
  }

  private nowMs(): number {
    return utMs(this.day!.dayStart) + this.minute * 60_000;
  }

  // --- Readouts -------------------------------------------------------------------

  private renderReadouts(): void {
    const d = this.day;
    if (!d || !this.series) return;
    const tz = d.tzOffsetHours;
    const now = this.nowMs();
    const s = this.series.at(now);
    replaceChildren(this.title, h('strong', {}, `${formatDate(d.date)}`), ` · local day (${zoneLabel(tz)})`);
    const t = (x: UtDateTime | null) => (x ? hm(utMs(x), tz) : '—');
    const ev = (e: BodyDayEvents) => {
      if (e.circumpolar) return e.circumpolar === 'always-up' ? 'Up all day' : 'Down all day';
      const az = (v: number | null) => (v === null ? '' : ` (${v.toFixed(0)}° ${compassPoint(v)})`);
      return `rise ${t(e.rise)}${az(e.riseAzDeg)} · transit ${t(e.transit)}${e.transitAltDeg === null ? '' : ` (${e.transitAltDeg.toFixed(1)}°)`} · set ${t(e.set)}${az(e.setAzDeg)}`;
    };
    const lastNew = [...d.phases].reverse().find((p) => p.phase === 'new' && utMs(p.time) <= now);
    const next = (code: string) => d.phases.find((p) => p.phase === code && utMs(p.time) > now);
    const shadow = (alt: number) => (alt > 0 ? `${(1 / Math.tan(alt * DEG)).toFixed(2)} m` : 'below the horizon');
    const pos = (alt: number, az: number) => `alt ${alt >= 0 ? '+' : ''}${alt.toFixed(1)}° · az ${az.toFixed(1)}° ${compassPoint(az)}`;
    const tw = (a: UtDateTime | null, b: UtDateTime | null) => `${t(a)} – ${t(b)}`;
    const nextNew = next('new');
    const nextFull = next('full');
    replaceChildren(
      this.readouts,
      kv([
        ['Time', `${dateHm(now, tz)} ${zoneLabel(tz)}`],
        ['Sun', pos(s.sunAltDeg, s.sunAzDeg)],
        ['Moon', pos(s.moonAltDeg, s.moonAzDeg)],
        ['Sun events', ev(d.sun)],
        ['Moon events', ev(d.moon)],
        ['Distance', `${Math.round(s.moonDistanceKm).toLocaleString('en')} km (geocentric ${Math.round(s.moonGeoDistanceKm).toLocaleString('en')} km)`],
        ['Illumination', `${(s.illuminatedFraction * 100).toFixed(1)} %`],
        ['Phase', `${phaseName(s.elongationLonDeg)}${lastNew ? `, ${((now - utMs(lastNew.time)) / 86_400_000).toFixed(2)} days old` : ''}`],
        ['Next new Moon', nextNew ? `${dateHm(utMs(nextNew.time), tz)}` : '—'],
        ['Next full Moon', nextFull ? `${dateHm(utMs(nextFull.time), tz)}` : '—'],
        ['Shadow of 1 m', `Sun: ${shadow(s.sunAltDeg)} · Moon: ${shadow(s.moonAltDeg)}`],
        ['Civil twilight', tw(d.civil.dawn, d.civil.dusk)],
        ['Nautical', tw(d.nautical.dawn, d.nautical.dusk)],
        ['Astronomical', tw(d.astronomical.dawn, d.astronomical.dusk)],
      ]),
    );
  }

  // --- Altitude plot ----------------------------------------------------------------

  private drawPlot(ctx: CanvasRenderingContext2D, w: number, hh: number): void {
    const d = this.day;
    const series = this.series;
    const b = this.plotBox(w, hh);
    const text = cssVar('--text', '#ccc');
    const muted = cssVar('--text-muted', '#999');
    if (!d || !series) {
      label(ctx, 'Computing the day…', w / 2, hh / 2, text, 'center');
      return;
    }
    const x = (m: number) => b.x0 + (m / 1440) * (b.x1 - b.x0);
    const y = (alt: number) => b.y0 + ((90 - alt) / 180) * (b.y1 - b.y0);
    const start = utMs(d.dayStart);
    const tz = d.tzOffsetHours;
    // Twilight bands.
    const cols = Math.max(48, Math.round((b.x1 - b.x0) / 3));
    for (let i = 0; i < cols; i++) {
      const m = ((i + 0.5) / cols) * 1440;
      ctx.fillStyle = bandColor(series.at(start + m * 60_000).sunAltDeg);
      // Whole-pixel columns, so neighbouring bands do not leave seams.
      const xa = Math.round(x((i / cols) * 1440));
      const xb = Math.round(x(((i + 1) / cols) * 1440));
      ctx.fillRect(xa, b.y0, xb - xa, b.y1 - b.y0);
    }
    // Grid.
    ctx.strokeStyle = 'rgb(128 128 128 / 0.3)';
    ctx.lineWidth = 0.6;
    for (let alt = -90; alt <= 90; alt += 30) {
      ctx.beginPath();
      ctx.moveTo(b.x0, y(alt));
      ctx.lineTo(b.x1, y(alt));
      ctx.stroke();
      label(ctx, `${alt}°`, b.x0 - 6, y(alt), muted, 'right', 'middle', 10);
    }
    for (let hr = 0; hr <= 24; hr += 2) {
      ctx.beginPath();
      ctx.moveTo(x(hr * 60), b.y0);
      ctx.lineTo(x(hr * 60), b.y1);
      ctx.stroke();
      label(ctx, `${String(hr % 24).padStart(2, '0')}:00`, x(hr * 60), b.y1 + 14, muted, 'center', 'middle', 10);
    }
    ctx.strokeStyle = 'rgb(140 170 120)';
    ctx.lineWidth = 1.4;
    ctx.beginPath();
    ctx.moveTo(b.x0, y(0));
    ctx.lineTo(b.x1, y(0));
    ctx.stroke();
    // Curves.
    for (const [color, key] of [
      [SUN_COLOR, 'sunAltDeg'],
      [MOON_COLOR, 'moonAltDeg'],
    ] as const) {
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.beginPath();
      series.samples.forEach((s, i) => {
        const px = x(i * d.stepMinutes);
        const py = y(s[key]);
        if (i === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      });
      ctx.stroke();
    }
    // Events.
    const mark = (t: UtDateTime | null, color: string, text: string, above: boolean) => {
      if (!t) return;
      const m = (utMs(t) - start) / 60_000;
      const s = series.at(utMs(t));
      const alt = color === SUN_COLOR ? s.sunAltDeg : s.moonAltDeg;
      ctx.beginPath();
      ctx.arc(x(m), y(alt), 3.5, 0, 2 * Math.PI);
      ctx.fillStyle = color;
      ctx.fill();
      label(ctx, `${text} ${hm(utMs(t), tz)}`, x(m), y(alt) + (above ? -11 : 12), color, 'center', 'middle', 10);
    };
    mark(d.sun.rise, SUN_COLOR, '☀↑', true);
    mark(d.sun.set, SUN_COLOR, '☀↓', true);
    mark(d.sun.transit, SUN_COLOR, '☀', true);
    mark(d.moon.rise, MOON_COLOR, '☾↑', false);
    mark(d.moon.set, MOON_COLOR, '☾↓', false);
    mark(d.moon.transit, MOON_COLOR, '☾', false);
    // Slider time.
    const now = this.nowMs();
    const s = series.at(now);
    ctx.strokeStyle = cssVar('--accent', '#4ade80');
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    ctx.moveTo(x(this.minute), b.y0);
    ctx.lineTo(x(this.minute), b.y1);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(x(this.minute), y(s.sunAltDeg), 5, 0, 2 * Math.PI);
    ctx.fillStyle = SUN_COLOR;
    ctx.fill();
    drawMoon(ctx, x(this.minute), y(s.moonAltDeg), 7, s.illuminatedFraction, rotateCcw([0, -1], s.limbZenithDeg));
    label(ctx, 'Altitude', b.x0 + 4, b.y0 + 9, text, 'left', 'middle', 11);
  }

  // --- Compass ---------------------------------------------------------------------------

  private drawCompass(ctx: CanvasRenderingContext2D, w: number, hh: number): void {
    const d = this.day;
    const series = this.series;
    if (!d || !series) return;
    const text = cssVar('--text', '#ccc');
    const muted = cssVar('--text-muted', '#999');
    const cx = w / 2;
    const cy = hh / 2;
    const R = Math.max(30, Math.min(w, hh) / 2 - 26);
    // Seen from above: east to the right.
    const project = (alt: number, az: number): [number, number] => {
      const r = (R * (90 - Math.max(0, alt))) / 90;
      return [cx + r * Math.sin(az * DEG), cy - r * Math.cos(az * DEG)];
    };
    const rim = (az: number, k = 1): [number, number] => [cx + k * R * Math.sin(az * DEG), cy - k * R * Math.cos(az * DEG)];
    ctx.fillStyle = 'rgb(128 140 160 / 0.12)';
    ctx.beginPath();
    ctx.arc(cx, cy, R, 0, 2 * Math.PI);
    ctx.fill();
    ctx.strokeStyle = 'rgb(128 128 128 / 0.5)';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.strokeStyle = 'rgb(128 128 128 / 0.25)';
    for (const alt of [30, 60]) {
      ctx.beginPath();
      ctx.arc(cx, cy, (R * (90 - alt)) / 90, 0, 2 * Math.PI);
      ctx.stroke();
    }
    for (let az = 0; az < 360; az += 30) {
      ctx.beginPath();
      ctx.moveTo(...rim(az, 0.96));
      ctx.lineTo(...rim(az, 1));
      ctx.stroke();
    }
    for (const [az, n] of [
      [0, 'N'],
      [90, 'E'],
      [180, 'S'],
      [270, 'W'],
    ] as const) {
      const p = rim(az, 1.12);
      label(ctx, n, p[0], p[1], text, 'center', 'middle', 12);
    }
    // Day arcs above the horizon.
    for (const [color, alt, az] of [
      [SUN_COLOR, 'sunAltDeg', 'sunAzDeg'],
      [MOON_COLOR, 'moonAltDeg', 'moonAzDeg'],
    ] as const) {
      ctx.strokeStyle = color;
      ctx.globalAlpha = 0.7;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      let pen = false;
      for (const s of series.samples) {
        if (s[alt] < 0) {
          pen = false;
          continue;
        }
        const p = project(s[alt], s[az]);
        if (pen) ctx.lineTo(...p);
        else ctx.moveTo(...p);
        pen = true;
      }
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
    // Rise and set azimuths.
    const ray = (azDeg: number | null, color: string, text: string) => {
      if (azDeg === null) return;
      ctx.strokeStyle = color;
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.lineTo(...rim(azDeg));
      ctx.stroke();
      ctx.setLineDash([]);
      const p = rim(azDeg, 0.8);
      label(ctx, text, p[0], p[1], color, 'center', 'middle', 10);
    };
    ray(d.sun.riseAzDeg, SUN_COLOR, `↑${d.sun.riseAzDeg?.toFixed(0)}°`);
    ray(d.sun.setAzDeg, SUN_COLOR, `↓${d.sun.setAzDeg?.toFixed(0)}°`);
    ray(d.moon.riseAzDeg, MOON_COLOR, `↑${d.moon.riseAzDeg?.toFixed(0)}°`);
    ray(d.moon.setAzDeg, MOON_COLOR, `↓${d.moon.setAzDeg?.toFixed(0)}°`);
    // Current positions (hollow on the rim when below the horizon).
    const s = series.at(this.nowMs());
    for (const [color, alt, az] of [
      [SUN_COLOR, s.sunAltDeg, s.sunAzDeg],
      [MOON_COLOR, s.moonAltDeg, s.moonAzDeg],
    ] as const) {
      const p = project(alt, az);
      ctx.beginPath();
      ctx.arc(p[0], p[1], 6, 0, 2 * Math.PI);
      if (alt >= 0) {
        ctx.fillStyle = color;
        ctx.fill();
      } else {
        ctx.strokeStyle = color;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    }
    label(ctx, 'Seen from above · zenith at the centre', 6, hh - 8, muted, 'left', 'middle', 10);
  }
}
