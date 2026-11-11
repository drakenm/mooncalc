// Display formatting.

import type { CivilDate, UtDateTime } from '../backend';

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

export function formatTime(t: Pick<UtDateTime, 'hour' | 'minute'>): string {
  return `${pad(t.hour)}:${pad(t.minute)}`;
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

export function formatUtDateTime(t: UtDateTime | null, tzOffsetHours?: number): string {
  if (!t) return '—';
  const ut = `${formatDate(t)} ${formatTime(t)} UT`;
  if (tzOffsetHours === undefined || tzOffsetHours === 0) return ut;
  const local = shiftHours(t, tzOffsetHours);
  return `${ut} (${formatTime(local)} ${formatOffset(tzOffsetHours)})`;
}

export function formatNumber(v: number | null | undefined, digits: number, unit = ''): string {
  return v === null || v === undefined || !Number.isFinite(v) ? '—' : `${v.toFixed(digits)}${unit}`;
}

/**
 * The UT offset, in hours, of an IANA time zone at a given moment, or null if
 * this browser does not know the zone.
 */
export function zoneOffsetHours(timeZone: string, at: Date): number | null {
  try {
    const part = new Intl.DateTimeFormat('en-US', { timeZone, timeZoneName: 'longOffset' })
      .formatToParts(at)
      .find((p) => p.type === 'timeZoneName')?.value;
    if (!part || part === 'GMT') return 0;
    const m = /GMT([+-])(\d{1,2})(?::(\d{2}))?/.exec(part);
    if (!m) return null;
    const hours = Number(m[2]) + Number(m[3] ?? 0) / 60;
    return m[1] === '-' ? -hours : hours;
  } catch {
    return null;
  }
}
