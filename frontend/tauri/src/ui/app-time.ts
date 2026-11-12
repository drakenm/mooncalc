// Helpers shared by the Calendar and Ephemeris tabs: local-time display of
// UT instants, and setting the app's date/time from a clicked event.

import type { CivilDate, UtDateTime } from '../backend';
import { localDate, type Store } from '../state';
import { dateMs, localToUtMs, offsetAt, type Tz, tzOf, zoneLabel as tzLabel } from '../zone';
import { h } from './dom';
import { roundToMinute, shiftHours } from './format';

const pad = (n: number, w = 2) => String(Math.trunc(n)).padStart(w, '0');

export const MONTH_NAMES = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

export const MONTH_ABBR = MONTH_NAMES.map((m) => m.slice(0, 3));

/** "11 March 2024". */
export function longDate(d: CivilDate): string {
  return `${d.day} ${MONTH_NAMES[d.month - 1]} ${d.year}`;
}

/** "11 Mar 2024". */
export function shortDate(d: CivilDate): string {
  return `${d.day} ${MONTH_ABBR[d.month - 1]} ${d.year}`;
}

/** A UT instant shifted into the zone (its offset then), rounded to the minute. */
export function toLocal(t: UtDateTime, tz: Tz): UtDateTime {
  return shiftHours(roundToMinute(t), offsetAt(tz, t));
}

/** "13:42" in the zone. */
export function localTime(t: UtDateTime | null, tz: Tz): string {
  if (!t) return '—';
  const l = toLocal(t, tz);
  return `${pad(l.hour)}:${pad(l.minute)}`;
}

/** "2024-04-08 13:42" in the zone. */
export function localDateTime(t: UtDateTime | null, tz: Tz): string {
  if (!t) return '—';
  const l = toLocal(t, tz);
  return `${l.year}-${pad(l.month)}-${pad(l.day)} ${pad(l.hour)}:${pad(l.minute)}`;
}

/** "8 Apr 13:42" in the zone. */
export function localDayTime(t: UtDateTime | null, tz: Tz): string {
  if (!t) return '—';
  const l = toLocal(t, tz);
  return `${l.day} ${MONTH_ABBR[l.month - 1]} ${pad(l.hour)}:${pad(l.minute)}`;
}

/** "2024-04-08 18:42:40 UT". */
export function utLabel(t: UtDateTime): string {
  return `${t.year}-${pad(t.month)}-${pad(t.day)} ${pad(t.hour)}:${pad(t.minute)}:${pad(Math.floor(t.second))} UT`;
}

/** Header label of a zone: its IANA name, or the fixed offset (`UT+3`). */
export function zoneLabel(tz: Tz): string {
  return tzLabel(tz);
}

/** Set the app's instant (UT), rounded to the minute. */
export function setAppInstant(store: Store, t: UtDateTime): void {
  store.update({ instant: roundToMinute(t) });
}

/** Move the app to a local civil day, keeping the local time of day. */
export function setAppDay(store: Store, date: CivilDate): void {
  const s = store.get();
  const tz = tzOf(s);
  const local = shiftHours(s.instant, offsetAt(tz, s.instant));
  const ms = localToUtMs(tz, dateMs(date) + (local.hour * 60 + local.minute) * 60_000);
  store.update({ instant: { ...shiftHours({ ...date, hour: 0, minute: 0, second: 0 }, (ms - dateMs(date)) / 3_600_000), second: 0 } });
}

/** The app's selected local date. */
export function appDay(store: Store): CivilDate {
  return localDate(store.get());
}

/** Today's date in the app's time zone. */
export function todayLocal(tz: Tz): CivilDate {
  const now = Date.now();
  const d = new Date(now + tz.offsetAtMs(now) * 3_600_000);
  return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, day: d.getUTCDate() };
}

export const sameDay = (a: CivilDate | null | undefined, b: CivilDate | null | undefined): boolean =>
  !!a && !!b && a.year === b.year && a.month === b.month && a.day === b.day;

/**
 * A clickable local time: shows `text` (default the local HH:MM), the UT
 * instant on hover, and sets the app's date/time when clicked.
 */
export function timeLink(store: Store, t: UtDateTime | null, text?: string): HTMLElement {
  if (!t) return h('span', { class: 'muted' }, '—');
  const tz = tzOf(store.get());
  return h(
    'button',
    {
      type: 'button',
      class: 'time-link',
      title: `${utLabel(t)} — click to set the app date and time`,
      onclick: () => setAppInstant(store, t),
    },
    text ?? localTime(t, tz),
  );
}

/** Copy text to the clipboard (with a fallback for older webviews). */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Fall through to the selection-based copy.
  }
  const ta = h('textarea', {}, text);
  ta.style.position = 'fixed';
  ta.style.opacity = '0';
  document.body.appendChild(ta);
  ta.select();
  const ok = document.execCommand('copy');
  ta.remove();
  if (!ok) throw new Error('the clipboard is not available');
}

/** A year selector: « [year] » that reports changes. */
export function yearStepper(initial: number, onChange: (year: number) => void, label = 'Year'): { el: HTMLElement; set(y: number): void; get(): number } {
  const input = h('input', { type: 'number', min: -4000, max: 9999, step: 1, value: String(initial), 'aria-label': label, class: 'year-input' });
  const emit = () => {
    const y = Math.round(input.valueAsNumber);
    if (Number.isFinite(y)) onChange(y);
  };
  const bump = (d: number) => {
    input.value = String(Math.round(input.valueAsNumber || initial) + d);
    emit();
  };
  input.addEventListener('change', emit);
  const el = h(
    'span',
    { class: 'stepper' },
    h('button', { type: 'button', class: 'button small', title: `Previous ${label.toLowerCase()}`, onclick: () => bump(-1) }, '«'),
    input,
    h('button', { type: 'button', class: 'button small', title: `Next ${label.toLowerCase()}`, onclick: () => bump(1) }, '»'),
  );
  return {
    el,
    set: (y) => {
      input.value = String(y);
    },
    get: () => Math.round(input.valueAsNumber || initial),
  };
}
