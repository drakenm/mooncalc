// The world map's date line versus the observer's civil date.
//
// Mirrors `mooncalc_core::events::mean_time_day_shift` and the wording of
// `mooncalc_app::dateline` (keep them in step). Maps put the date line on the
// 180° meridian (each place on its local-mean-time day, the HMNAO
// convention); the report follows the civil date. East of 180° in UTC+13/+14
// the civil date runs a day ahead, so the map for a date shows the next civil
// evening there. The report's note comes from Rust (`mapDateNote`); the map's
// observer note is computed here because the observer is not part of a map
// request (moving the marker must not recompute the map).
//
// Self-contained (type imports only) so `node --test` can load it.

import type { CivilDate, Crescent, MapDateNote } from './backend';

/**
 * Days from a civil date (zone `tzOffsetHours` ahead of UT) to the map date
 * that shows its evening at east longitude `lonDeg`:
 * round(λ/360° − offset/24 h), λ reduced to [−180°, 180°).
 */
export function meanTimeDayShift(lonDeg: number, tzOffsetHours: number): number {
  const lon = ((((lonDeg + 180) % 360) + 360) % 360) - 180;
  const x = lon / 360 - tzOffsetHours / 24;
  // Rust's f64::round: half away from zero.
  return (x < 0 ? -Math.round(-x) : Math.round(x)) || 0; // no -0
}

/** The date `days` later, proleptic Gregorian (dates near today). */
export function addDays(d: CivilDate, days: number): CivilDate {
  const t = new Date(0);
  t.setUTCFullYear(d.year, d.month - 1, d.day + days);
  return { year: t.getUTCFullYear(), month: t.getUTCMonth() + 1, day: t.getUTCDate() };
}

function dateText(d: CivilDate): string {
  const p = (n: number, w = 2) => String(Math.abs(n)).padStart(w, '0');
  return `${d.year < 0 ? '-' : ''}${p(d.year, 4)}-${p(d.month)}-${p(d.day)}`;
}

/** "UTC+13", "UTC+12:45", "UTC−5", "UTC" (as `dateline::utc_offset_text`). */
export function utcOffsetText(hours: number): string {
  const minutes = Math.round(hours * 60);
  if (minutes === 0) return 'UTC';
  const sign = minutes > 0 ? '+' : '−';
  const h = Math.trunc(Math.abs(minutes) / 60);
  const m = Math.abs(minutes) % 60;
  return m === 0 ? `UTC${sign}${h}` : `UTC${sign}${h}:${String(m).padStart(2, '0')}`;
}

/** As `dateline::map_convention`. */
export function mapConvention(mapDate: CivilDate, crescent: Crescent): string {
  return `Each location is shown on its own local ${crescent} of ${dateText(mapDate)}; the date line is the 180° meridian (HMNAO convention).`;
}

/** As `dateline::report_note` (used by the mock backend). */
export function reportNote(date: CivilDate, lonDeg: number, tzOffsetHours: number): MapDateNote | null {
  const shift = meanTimeDayShift(lonDeg, tzOffsetHours);
  if (shift === 0) return null;
  const mapDate = addDays(date, shift);
  const relation = shift < 0 ? 'ahead of' : 'behind';
  return {
    mapDate,
    dayShift: shift,
    text:
      `The world map for ${dateText(mapDate)} shows this evening and morning at your longitude; the map's ` +
      `date line is the 180° meridian (HMNAO convention), and your civil date (${utcOffsetText(tzOffsetHours)}) runs a ` +
      `day ${relation} it. The map for ${dateText(date)} shows the evening of ${dateText(addDays(date, -shift))} here.`,
  };
}

/**
 * As `dateline::observer_note`: when the evening (morning) a map of `mapDate`
 * shows under the observer is not their civil one of that date.
 */
export function observerNote(mapDate: CivilDate, lonDeg: number, tzOffsetHours: number, crescent: Crescent): string | null {
  const shift = meanTimeDayShift(lonDeg, tzOffsetHours);
  if (shift === 0) return null;
  return (
    `At the observer this map shows the ${crescent} of local date ${dateText(addDays(mapDate, -shift))} ` +
    `(${utcOffsetText(tzOffsetHours)}), not of ${dateText(mapDate)}: the map's date line is the 180° meridian. ` +
    `Their ${crescent} of ${dateText(mapDate)} is on the map for ${dateText(addDays(mapDate, shift))}.`
  );
}
