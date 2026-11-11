// Time zones in the frontend: an IANA zone with its DST and historical rules,
// or a fixed offset from UT.
//
// The rules come from the Rust backend (`time_zone_offsets`, over the IANA
// Time Zone Database embedded in mooncalc-atlas), as a table of offset
// changes over 1800–2500 that this module looks up synchronously, so any
// instant (a slider position, a hover readout) can be shown in local time.
// Times returned by zone-aware services also carry their own `offsetHours`,
// which `offsetAt` prefers.
//
// Mirrors `mooncalc_atlas::tz::Zone`: offsets are east-positive, a local
// wall-clock time is handled as milliseconds "as if UT", and local → UT uses
// the "compatible" rule (a skipped time moves forward by the gap, a repeated
// time takes its first occurrence).

import type { CivilDate, UtDateTime, ZoneOffsets } from './backend/types';

/** "UT+3", "UT−5:30", "UT": the same text as `formatOffset` in ui/format.ts
 * (kept here so this module loads without the UI, e.g. under `node --test`). */
export function formatOffset(hours: number): string {
  if (hours === 0) return 'UT';
  const sign = hours > 0 ? '+' : '−';
  const abs = Math.abs(hours);
  const h = Math.trunc(abs);
  const m = Math.round((abs - h) * 60);
  return `UT${sign}${h}${m ? `:${String(m).padStart(2, '0')}` : ''}`;
}

/** A time zone: the offset and abbreviation at any instant. */
export interface Tz {
  /** IANA name; null for a fixed offset. */
  readonly name: string | null;
  /** UTC offset at an instant (ms since 1970 UT), hours east. */
  offsetAtMs(ms: number): number;
  /** Abbreviation at an instant (`BST`, `+03`); `UT+3` for a fixed offset. */
  abbrevAtMs(ms: number): string;
}

/** 400 Gregorian years, in ms: the calendar (and every DST rule) repeats. */
const CYCLE_MS = 146_097 * 86_400_000;
const DAY_MS = 86_400_000;

/** A fixed offset from UT. */
export function fixedTz(hours: number): Tz {
  return {
    name: null,
    offsetAtMs: () => hours,
    abbrevAtMs: () => formatOffset(hours),
  };
}

/** An IANA zone from its offset table. */
export class ZoneTable implements Tz {
  readonly name: string;
  readonly tzdbVersion: string;
  private readonly at: number[];
  /** offsets[0] before the first change, offsets[i + 1] from at[i]; seconds. */
  private readonly offsets: number[];
  private readonly abbrevs: string[];
  private readonly endMs: number;

  constructor(t: ZoneOffsets) {
    this.name = t.timeZone;
    this.tzdbVersion = t.tzdbVersion;
    this.at = t.transitions.map((x) => x.unixSeconds * 1000);
    this.offsets = [t.initialOffsetSeconds, ...t.transitions.map((x) => x.offsetSeconds)];
    this.abbrevs = [t.initialAbbreviation, ...t.transitions.map((x) => x.abbreviation)];
    this.endMs = t.toUnixSeconds * 1000;
  }

  /** Index into `offsets` for an instant. After the table, instants are
   * folded back by whole 400-year cycles (the table's last 400 years follow
   * each zone's current rule); before it the initial offset holds. */
  private index(ms: number): number {
    if (ms >= this.endMs) ms -= Math.ceil((ms - this.endMs + 1) / CYCLE_MS) * CYCLE_MS;
    let lo = 0;
    let hi = this.at.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.at[mid]! <= ms) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  offsetAtMs(ms: number): number {
    return this.offsets[this.index(ms)]! / 3600;
  }

  abbrevAtMs(ms: number): string {
    return this.abbrevs[this.index(ms)]!;
  }
}

/** Milliseconds since the Unix epoch of a UT date-time (proleptic Gregorian). */
export function utMs(t: UtDateTime): number {
  const d = new Date(0);
  d.setUTCFullYear(t.year, t.month - 1, t.day);
  d.setUTCHours(t.hour, t.minute, 0, Math.round(t.second * 1000));
  return d.getTime();
}

/** Milliseconds of 00:00 on a calendar date, "as if UT". */
export function dateMs(d: CivilDate): number {
  const x = new Date(0);
  x.setUTCFullYear(d.year, d.month - 1, d.day);
  return x.getTime();
}

/** The offset at `t`: its own `offsetHours` (set by zone-aware services)
 * when present, else the zone's. */
export function offsetAt(tz: Tz, t: UtDateTime): number {
  return t.offsetHours ?? tz.offsetAtMs(utMs(t));
}

/**
 * The UT instant (ms) at which the clock shows `localMs` (a wall-clock time
 * as ms "as if UT"). A time skipped by a DST change moves forward by the
 * gap; a repeated one is taken at its first occurrence.
 */
export function localToUtMs(tz: Tz, localMs: number): number {
  const before = tz.offsetAtMs(localMs - DAY_MS);
  const after = tz.offsetAtMs(localMs + DAY_MS);
  const valid = [before, after]
    .map((o) => localMs - o * 3_600_000)
    .filter((ut, i) => tz.offsetAtMs(ut) === [before, after][i]);
  if (valid.length) return Math.min(...valid);
  // In a gap: read the time with the offset in force before it.
  return localMs - before * 3_600_000;
}

/** The local calendar date of an instant. */
export function localDateOf(tz: Tz, t: UtDateTime): CivilDate {
  const d = new Date(utMs(t) + offsetAt(tz, t) * 3_600_000);
  return { year: d.getUTCFullYear(), month: d.getUTCMonth() + 1, day: d.getUTCDate() };
}

/** The offset at local civil noon of a date, hours. */
export function noonOffset(tz: Tz, date: CivilDate): number {
  return tz.offsetAtMs(localToUtMs(tz, dateMs(date) + 12 * 3_600_000));
}

/** A short label of the zone at an instant: its abbreviation when that is a
 * word (`BST`, `EDT`), else the offset (`UT+3`). */
export function zoneAbbrev(tz: Tz, ms: number): string {
  if (tz.name) {
    const a = tz.abbrevAtMs(ms);
    if (/^[A-Za-z]+$/.test(a)) return a;
  }
  return formatOffset(tz.offsetAtMs(ms));
}

/** A header label: the IANA name (its offset varies), or the fixed offset. */
export function zoneLabel(tz: Tz): string {
  return tz.name ?? formatOffset(tz.offsetAtMs(0));
}

// --- Loaded zones ----------------------------------------------------------------

const tables = new Map<string, ZoneTable>();

/** Keep a zone's table, under its canonical name and the name asked for. */
export function registerZone(t: ZoneOffsets, alias?: string): ZoneTable {
  const z = new ZoneTable(t);
  tables.set(z.name, z);
  if (alias) tables.set(alias, z);
  return z;
}

/** A loaded zone, if any. */
export function loadedZone(name: string): ZoneTable | undefined {
  return tables.get(name);
}

/** Load a zone's table once (from the backend) and return it. */
export async function ensureZone(name: string, load: (name: string) => Promise<ZoneOffsets>): Promise<ZoneTable> {
  return tables.get(name) ?? registerZone(await load(name), name);
}

/** The zone of the app state: its IANA zone when loaded, else the fixed
 * offset. */
export function tzOf(s: { timeZone: string | null; tzOffsetHours: number }): Tz {
  return (s.timeZone ? tables.get(s.timeZone) : undefined) ?? fixedTz(s.tzOffsetHours);
}
