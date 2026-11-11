// Shared helpers of the Sky, Trajectory and Moon tabs: time arithmetic on
// UT date-times, interpolation of sampled series, canvas setup, sky
// colours, and drawing the phased Moon, the Sun and stars.
//
// Orientation convention (as mooncalc-app): every view is drawn as seen on
// the sky, so position angles (from north or the zenith, through east) turn
// counter-clockwise on screen.

import type { UtDateTime } from '../backend';
import { formatOffset } from './format';

export const DEG = Math.PI / 180;

export const norm360 = (d: number): number => ((d % 360) + 360) % 360;
export const norm180 = (d: number): number => norm360(d + 180) - 180;

// --- Time ---------------------------------------------------------------------

/** Milliseconds since the Unix epoch of a UT date-time. */
export function utMs(t: UtDateTime): number {
  const d = new Date(0);
  d.setUTCFullYear(t.year, t.month - 1, t.day);
  d.setUTCHours(t.hour, t.minute, 0, Math.round(t.second * 1000));
  return d.getTime();
}

export function msToUt(ms: number): UtDateTime {
  const d = new Date(Math.round(ms / 1000) * 1000);
  return {
    year: d.getUTCFullYear(),
    month: d.getUTCMonth() + 1,
    day: d.getUTCDate(),
    hour: d.getUTCHours(),
    minute: d.getUTCMinutes(),
    second: d.getUTCSeconds(),
  };
}

const pad = (n: number) => String(n).padStart(2, '0');

/** "HH:MM" in the zone `tz` hours ahead of UT. */
export function hm(ms: number, tz: number): string {
  const d = new Date(ms + tz * 3_600_000);
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

/** "YYYY-MM-DD HH:MM" local. */
export function dateHm(ms: number, tz: number): string {
  const d = new Date(ms + tz * 3_600_000);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

/** "HH:MM UT+3" for an optional UT date-time. */
export function localHm(t: UtDateTime | null | undefined, tz: number): string {
  return t ? `${hm(utMs(t), tz)}` : '—';
}

export function zoneLabel(tz: number): string {
  return formatOffset(tz);
}

// --- Sampled series ---------------------------------------------------------------

/** Fields interpolated as angles (wrapping at 360°). */
const ANGLE_FIELDS = new Set(['sunAzDeg', 'moonAzDeg', 'sunRaDeg', 'moonRaDeg', 'lstDeg', 'brightLimbDeg', 'parallacticDeg', 'limbZenithDeg', 'elongationLonDeg']);

/** Evenly spaced samples of numeric records, interpolated linearly in time. */
export class Series<T extends object> {
  constructor(
    readonly startMs: number,
    readonly stepMs: number,
    readonly samples: T[],
  ) {}

  get endMs(): number {
    return this.startMs + (this.samples.length - 1) * this.stepMs;
  }

  timeOf(i: number): number {
    return this.startMs + i * this.stepMs;
  }

  /** The record at `ms` (clamped to the sampled span). */
  at(ms: number): T {
    const n = this.samples.length;
    const x = Math.max(0, Math.min(n - 1, (ms - this.startMs) / this.stepMs));
    const i = Math.min(n - 2, Math.floor(x));
    if (n < 2 || i < 0) return this.samples[0]!;
    const f = x - i;
    const a = this.samples[i]! as Record<string, unknown>;
    const b = this.samples[i + 1]! as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(a)) {
      const va = a[k];
      const vb = b[k];
      if (typeof va === 'number' && typeof vb === 'number') {
        out[k] = ANGLE_FIELDS.has(k) ? norm360(va + norm180(vb - va) * f) : va + (vb - va) * f;
      } else {
        out[k] = f < 0.5 ? va : vb;
      }
    }
    return out as T;
  }
}

// --- Canvas ---------------------------------------------------------------------

/**
 * A canvas that fills its parent at device resolution and redraws on
 * resize. `draw` receives a context scaled to CSS pixels.
 */
export class Stage {
  readonly canvas: HTMLCanvasElement;
  width = 0;
  height = 0;
  private frame = 0;

  constructor(
    readonly el: HTMLElement,
    private readonly draw: (ctx: CanvasRenderingContext2D, w: number, h: number) => void,
  ) {
    this.canvas = document.createElement('canvas');
    this.canvas.className = 'sky-canvas';
    el.appendChild(this.canvas);
    new ResizeObserver(() => this.redraw()).observe(el);
  }

  redraw(): void {
    cancelAnimationFrame(this.frame);
    this.frame = requestAnimationFrame(() => this.paint());
  }

  private paint(): void {
    const rect = this.el.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(rect.width * dpr);
    const h = Math.round(rect.height * dpr);
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.width = rect.width;
    this.height = rect.height;
    const ctx = this.canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, rect.width, rect.height);
    this.draw(ctx, rect.width, rect.height);
  }
}

/** A CSS custom property of the document (theme colour). */
export function cssVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

// --- Colours ------------------------------------------------------------------------

type Rgb = [number, number, number];

function mix(a: Rgb, b: Rgb, t: number): Rgb {
  const u = Math.max(0, Math.min(1, t));
  return [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u];
}

function smoothstep(e0: number, e1: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
}

/**
 * An illustrative sky colour (0–1 sRGB) at altitude `altDeg`, `sepDeg` from
 * the Sun, with the Sun at `sunAltDeg`: day, civil, nautical and night
 * palettes keyed on the Sun's altitude, plus a warm glow toward the Sun
 * near the horizon in twilight. Not a radiative-transfer model; the same
 * blend as the egui Sky tab.
 */
export function skyColor(sunAltDeg: number, altDeg: number, sepDeg: number): Rgb {
  const h = Math.max(0, Math.min(90, altDeg));
  const away = smoothstep(60, 170, sepDeg);
  const zt = Math.max(smoothstep(0, 60, h), 0.65 * away);
  const day = mix([0.62, 0.78, 0.95], [0.22, 0.47, 0.86], zt);
  const civil = mix([0.55, 0.52, 0.62], [0.13, 0.22, 0.45], zt);
  const nautical = mix([0.12, 0.15, 0.3], [0.04, 0.07, 0.18], zt);
  const night = mix([0.03, 0.04, 0.09], [0.01, 0.015, 0.045], zt);
  const s = sunAltDeg;
  let c = s >= -6 ? mix(civil, day, smoothstep(-2, 6, s)) : s >= -12 ? mix(nautical, civil, smoothstep(-12, -6, s)) : mix(night, nautical, smoothstep(-18, -12, s));
  const strength = smoothstep(-16, -3, s) * (1 - smoothstep(2, 10, s));
  const glow = strength * Math.exp(-sepDeg / 35) * Math.exp(-h / 12);
  c = mix(c, [1, 0.55, 0.22], glow * 0.85);
  return c;
}

export function rgbCss(c: Rgb, alpha = 1): string {
  const b = (x: number) => Math.round(Math.max(0, Math.min(1, x)) * 255);
  return `rgb(${b(c[0])} ${b(c[1])} ${b(c[2])} / ${alpha})`;
}

/** Star opacity against a sky with the Sun at `sunAltDeg`. */
export function starVisibility(sunAltDeg: number): number {
  return 1 - smoothstep(-16, -4, sunAltDeg);
}

export function starRadius(vmag: number): number {
  return Math.max(0.9, Math.min(4.8, 3.6 - 0.75 * vmag));
}

export function starTint(bv: number): Rgb {
  return mix([0.75, 0.85, 1], [1, 0.78, 0.52], (bv + 0.3) / 2);
}

/** Angular separation of two (alt, az) points, degrees. */
export function separation(alt1: number, az1: number, alt2: number, az2: number): number {
  const c = Math.sin(alt1 * DEG) * Math.sin(alt2 * DEG) + Math.cos(alt1 * DEG) * Math.cos(alt2 * DEG) * Math.cos((az1 - az2) * DEG);
  return Math.acos(Math.max(-1, Math.min(1, c))) / DEG;
}

/**
 * Apparent altitude and azimuth of a star (RA/Dec of date) for latitude
 * `latDeg` at local sidereal time `lstDeg`, with Bennett's refraction.
 */
export function starAltAz(raDeg: number, decDeg: number, latDeg: number, lstDeg: number): { alt: number; az: number } {
  const H = (lstDeg - raDeg) * DEG;
  const d = decDeg * DEG;
  const p = latDeg * DEG;
  const alt = Math.asin(Math.sin(p) * Math.sin(d) + Math.cos(p) * Math.cos(d) * Math.cos(H)) / DEG;
  const az = norm360(Math.atan2(-Math.cos(d) * Math.sin(H), Math.sin(d) * Math.cos(p) - Math.cos(d) * Math.cos(H) * Math.sin(p)) / DEG);
  const refr = alt > -1.5 ? 1 / Math.tan((alt + 7.31 / (alt + 4.4)) * DEG) / 60 : 0;
  return { alt: alt + refr, az };
}

/** RA/Dec (degrees) of a point at (alt, az) for latitude and LST. */
export function altAzToRaDec(altDeg: number, azDeg: number, latDeg: number, lstDeg: number): { ra: number; dec: number } {
  const a = altDeg * DEG;
  const A = azDeg * DEG;
  const p = latDeg * DEG;
  const dec = Math.asin(Math.sin(p) * Math.sin(a) + Math.cos(p) * Math.cos(a) * Math.cos(A));
  const H = Math.atan2(-Math.sin(A) * Math.cos(a), Math.sin(a) * Math.cos(p) - Math.cos(a) * Math.cos(A) * Math.sin(p));
  return { ra: norm360(lstDeg - H / DEG), dec: dec / DEG };
}

// --- Drawing -------------------------------------------------------------------------

/** Screen unit vector of a direction `angleDeg` counter-clockwise from `ref` (y down). */
export function rotateCcw(ref: [number, number], angleDeg: number): [number, number] {
  const len = Math.hypot(ref[0], ref[1]) || 1;
  const [x, y] = [ref[0] / len, ref[1] / len];
  const s = Math.sin(angleDeg * DEG);
  const c = Math.cos(angleDeg * DEG);
  return [x * c + y * s, -x * s + y * c];
}

/**
 * Draw the Moon as a phased disc of radius `r` with illuminated fraction
 * `k`, the bright limb pointing along screen direction `limb` (unit
 * vector). The terminator is the ellipse of semi-axis |1 − 2k| (Meeus eq.
 * 48.1; `mooncalc_app::sky::geometry::crescent_rows`).
 */
export function drawMoon(ctx: CanvasRenderingContext2D, x: number, y: number, r: number, k: number, limb: [number, number], earthshine = 'rgb(60 64 78 / 0.85)'): void {
  const [ux, uy] = limb;
  const [vx, vy] = [uy, -ux];
  ctx.save();
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.fillStyle = earthshine;
  ctx.fill();
  const kk = Math.max(0, Math.min(1, k));
  const pt = (u: number, v: number): [number, number] => [x + r * (u * ux + v * vx), y + r * (u * uy + v * vy)];
  ctx.beginPath();
  const n = 48;
  for (let j = 0; j <= n; j++) {
    const t = -Math.PI / 2 + (Math.PI * j) / n;
    const [px, py] = pt(Math.cos(t), Math.sin(t));
    if (j === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  }
  for (let j = n; j >= 0; j--) {
    const t = -Math.PI / 2 + (Math.PI * j) / n;
    const [px, py] = pt((1 - 2 * kk) * Math.cos(t), Math.sin(t));
    ctx.lineTo(px, py);
  }
  ctx.closePath();
  ctx.fillStyle = '#f4f0dc';
  ctx.fill();
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.strokeStyle = 'rgb(240 236 215 / 0.35)';
  ctx.lineWidth = 0.8;
  ctx.stroke();
  ctx.restore();
}

/** The Sun: a disc with a soft halo, dimmed below the horizon. */
export function drawSun(ctx: CanvasRenderingContext2D, x: number, y: number, r: number, below: boolean): void {
  ctx.save();
  const g = ctx.createRadialGradient(x, y, r * 0.5, x, y, r * 3);
  g.addColorStop(0, below ? 'rgb(255 170 60 / 0.35)' : 'rgb(255 220 120 / 0.6)');
  g.addColorStop(1, 'rgb(255 170 60 / 0)');
  ctx.fillStyle = g;
  ctx.beginPath();
  ctx.arc(x, y, r * 3, 0, 2 * Math.PI);
  ctx.fill();
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.fillStyle = below ? 'rgb(255 160 50 / 0.55)' : '#ffd36b';
  ctx.fill();
  ctx.strokeStyle = 'rgb(255 140 30 / 0.9)';
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.restore();
}

export function drawStar(ctx: CanvasRenderingContext2D, x: number, y: number, vmag: number, bv: number, alpha: number): void {
  ctx.beginPath();
  ctx.arc(x, y, starRadius(vmag), 0, 2 * Math.PI);
  ctx.fillStyle = rgbCss(starTint(bv), alpha);
  ctx.fill();
}

/** Text with a dark halo, readable over any sky. */
export function label(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, color: string, align: CanvasTextAlign = 'left', baseline: CanvasTextBaseline = 'middle', size = 11): void {
  ctx.save();
  ctx.font = `${size}px system-ui, sans-serif`;
  ctx.textAlign = align;
  ctx.textBaseline = baseline;
  ctx.lineWidth = 3;
  ctx.strokeStyle = 'rgb(0 0 0 / 0.55)';
  ctx.strokeText(text, x, y);
  ctx.fillStyle = color;
  ctx.fillText(text, x, y);
  ctx.restore();
}

/** A polyline that breaks where consecutive points jump more than `maxJump` px (wrap-around). */
export function polyline(ctx: CanvasRenderingContext2D, pts: [number, number][], maxJump: number): void {
  ctx.beginPath();
  let pen = false;
  let prev: [number, number] | null = null;
  for (const p of pts) {
    if (prev && Math.hypot(p[0] - prev[0], p[1] - prev[1]) > maxJump) pen = false;
    if (pen) ctx.lineTo(p[0], p[1]);
    else ctx.moveTo(p[0], p[1]);
    pen = true;
    prev = p;
  }
  ctx.stroke();
}

export const SUN_COLOR = '#ffb347';
export const MOON_COLOR = '#d9dcf0';

/** The name of the phase for an elongation in longitude λ☾ − λ☉ (degrees), as `mooncalc_app::sky::geometry::phase_name`. */
export function phaseName(elongLonDeg: number): string {
  const e = norm360(elongLonDeg);
  const near = (x: number) => Math.abs(e - x) <= 6;
  if (e <= 6 || e >= 354) return 'New Moon';
  if (near(90)) return 'First quarter';
  if (near(180)) return 'Full Moon';
  if (near(270)) return 'Last quarter';
  if (e < 90) return 'Waxing crescent';
  if (e < 180) return 'Waxing gibbous';
  if (e < 270) return 'Waning gibbous';
  return 'Waning crescent';
}

/** Sixteen-point compass name. */
export function compassPoint(azDeg: number): string {
  const names = ['N', 'NNE', 'NE', 'ENE', 'E', 'ESE', 'SE', 'SSE', 'S', 'SSW', 'SW', 'WSW', 'W', 'WNW', 'NW', 'NNW'];
  return names[Math.round(norm360(azDeg) / 22.5) % 16]!;
}

/** A play/pause time slider over [min, max] minutes. */
export class TimeSlider {
  readonly el: HTMLElement;
  readonly input: HTMLInputElement;
  private readonly playBtn: HTMLButtonElement;
  private readonly text: HTMLElement;
  private timer = 0;
  private lastTick = 0;

  constructor(
    private readonly onChange: (minute: number) => void,
    private readonly format: (minute: number) => string,
    /** Simulated minutes per real second while playing. */
    private readonly speed = 20,
  ) {
    this.playBtn = document.createElement('button');
    this.playBtn.type = 'button';
    this.playBtn.className = 'button small sky-play';
    this.playBtn.textContent = '▶';
    this.playBtn.setAttribute('aria-label', 'Play');
    this.playBtn.addEventListener('click', () => this.toggle());
    this.input = document.createElement('input');
    this.input.type = 'range';
    this.input.className = 'sky-slider';
    this.input.step = '1';
    this.input.setAttribute('aria-label', 'Time');
    this.input.addEventListener('input', () => this.emit());
    this.text = document.createElement('span');
    this.text.className = 'sky-time';
    this.el = document.createElement('div');
    this.el.className = 'sky-timebar';
    this.el.append(this.playBtn, this.input, this.text);
  }

  setRange(min: number, max: number, value: number): void {
    this.input.min = String(Math.floor(min));
    this.input.max = String(Math.ceil(max));
    this.input.value = String(Math.round(value));
    this.text.textContent = this.format(Number(this.input.value));
  }

  get value(): number {
    return Number(this.input.value);
  }

  private emit(): void {
    const v = Number(this.input.value);
    this.text.textContent = this.format(v);
    this.onChange(v);
  }

  toggle(): void {
    if (this.timer) this.stop();
    else this.play();
  }

  play(): void {
    if (this.timer) return;
    const min = Number(this.input.min);
    const max = Number(this.input.max);
    let pos = Number(this.input.value);
    if (pos >= max) pos = min;
    this.playBtn.textContent = '❚❚';
    this.playBtn.setAttribute('aria-label', 'Pause');
    this.lastTick = performance.now();
    const tick = (now: number) => {
      pos = Math.min(max, pos + ((now - this.lastTick) / 1000) * this.speed);
      this.lastTick = now;
      this.input.value = String(Math.round(pos));
      this.text.textContent = this.format(pos);
      this.onChange(pos);
      if (pos >= max) {
        this.stop();
        return;
      }
      this.timer = requestAnimationFrame(tick);
    };
    this.timer = requestAnimationFrame(tick);
  }

  stop(): void {
    cancelAnimationFrame(this.timer);
    this.timer = 0;
    this.playBtn.textContent = '▶';
    this.playBtn.setAttribute('aria-label', 'Play');
  }
}
