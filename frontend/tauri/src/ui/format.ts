// Display formatting.

import type { CivilDate, UtDateTime } from '../backend';

/** The zone of `formatUtDateTime`: structurally `zone.ts`'s `Tz`. */
interface ZoneLike {
  readonly name: string | null;
  offsetAtMs(ms: number): number;
  abbrevAtMs(ms: number): string;
}

const pad = (n: number, w = 2) => String(Math.trunc(n)).padStart(w, '0');

export function formatLat(latDeg: number, digits = 2): string {
  return `${Math.abs(latDeg).toFixed(digits)}° ${latDeg >= 0 ? 'N' : 'S'}`;
}

export function formatLon(lonDeg: number, digits = 2): string {
  return `${Math.abs(lonDeg).toFixed(digits)}° ${lonDeg >= 0 ? 'E' : 'W'}`;
}

export function formatDate(d: CivilDate): string {
  return `${d.year < 0 ? '-' : ''}${pad(Math.abs(d.year), 4)}-${pad(d.month)}-${pad(d.day)}`;
}

/** Hour and minute as given (no rounding; see `roundToMinute`). */
export function formatTime(t: Pick<UtDateTime, 'hour' | 'minute'>): string {
  return `${pad(t.hour)}:${pad(t.minute)}`;
}

/** Julian calendar before 1582-10-15, Gregorian from then, as the Rust side. */
function isJulian(year: number, month: number, day: number): boolean {
  return year < 1582 || (year === 1582 && (month < 10 || (month === 10 && day < 15)));
}

function daysInMonth(year: number, month: number, julian: boolean): number {
  if (month !== 2) return [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]!;
  const mod = (n: number, k: number) => ((n % k) + k) % k;
  const leap = julian ? mod(year, 4) === 0 : (mod(year, 4) === 0 && mod(year, 100) !== 0) || mod(year, 400) === 0;
  return leap ? 29 : 28;
}

/**
 * A date-time rounded to the nearest minute (seconds 0), carrying into the
 * hour, day, month and year, as egui and the HMNAO/USNO tables do. Dates
 * follow the Rust convention (Julian before 1582-10-15, astronomical years).
 */
export function roundToMinute(t: UtDateTime): UtDateTime {
  let { year, month, day, hour, minute } = t;
  if (t.second >= 30) minute++;
  if (minute >= 60) {
    minute -= 60;
    hour++;
  }
  if (hour >= 24) {
    hour -= 24;
    const julian = isJulian(year, month, day);
    if (year === 1582 && month === 10 && day === 4) day = 15;
    else if (++day > daysInMonth(year, month, julian)) {
      day = 1;
      if (++month > 12) {
        month = 1;
        year++;
      }
    }
  }
  return { year, month, day, hour, minute, second: 0 };
}

/** "UT+3", "UT−5:30", "UT". */
export function formatOffset(hours: number): string {
  if (hours === 0) return 'UT';
  const sign = hours > 0 ? '+' : '−';
  const abs = Math.abs(hours);
  const h = Math.trunc(abs);
  const m = Math.round((abs - h) * 60);
  return `UT${sign}${h}${m ? `:${pad(m)}` : ''}`;
}

/** A JavaScript Date for a UT date-time (proleptic Gregorian). */
export function utToDate(t: UtDateTime): Date {
  const d = new Date(0);
  d.setUTCFullYear(t.year, t.month - 1, t.day);
  d.setUTCHours(t.hour, t.minute, Math.trunc(t.second), 0);
  return d;
}

/** Shift a UT date-time by an offset in hours. */
export function shiftHours(t: UtDateTime, hours: number): UtDateTime {
  const d = new Date(utToDate(t).getTime() + hours * 3_600_000);
  return {
    year: d.getUTCFullYear(),
    month: d.getUTCMonth() + 1,
    day: d.getUTCDate(),
    hour: d.getUTCHours(),
    minute: d.getUTCMinutes(),
    second: d.getUTCSeconds(),
  };
}

/**
 * "2024-03-10 18:42 UT (21:42 UT+3)" or "… (19:42 BST)", rounded to the
 * nearest minute: the local time uses the offset at that instant (the
 * time's own `offsetHours` when a zone-aware service set it).
 */
export function formatUtDateTime(t: UtDateTime | null, tz?: ZoneLike): string {
  if (!t) return '—';
  const r = roundToMinute(t);
  const ut = `${formatDate(r)} ${formatTime(r)} UT`;
  if (!tz) return ut;
  const ms = utToDate(t).getTime();
  const offset = t.offsetHours ?? tz.offsetAtMs(ms);
  if (offset === 0 && !tz.name) return ut;
  const local = shiftHours(r, offset);
  const abbrev = tz.name && /^[A-Za-z]+$/.test(tz.abbrevAtMs(ms)) ? tz.abbrevAtMs(ms) : formatOffset(offset);
  return `${ut} (${formatTime(local)} ${abbrev})`;
}

export function formatNumber(v: number | null | undefined, digits: number, unit = ''): string {
  return v === null || v === undefined || !Number.isFinite(v) ? '—' : `${v.toFixed(digits)}${unit}`;
}

/** The symbol of a criterion's value: Yallop's q, Odeh's V, else "value". */
export function valueSymbol(criterionId: string): string {
  return criterionId === 'yallop' ? 'q' : criterionId === 'odeh' ? 'V' : 'value';
}

/** A criterion value with an explicit sign. */
export function formatValue(v: number, digits = 3): string {
  return `${v >= 0 ? '+' : '−'}${Math.abs(v).toFixed(digits)}`;
}

/** Save base64 data as a file through the browser's download mechanism. */
export function downloadBase64(base64: string, fileName: string, mime: string): void {
  const bin = atob(base64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = fileName;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
