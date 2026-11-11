// Time zones for the in-browser mock backend, from the browser's own IANA
// data (Intl). The desktop app gets them from Rust (the IANA Time Zone
// Database embedded in mooncalc-atlas); this is only a stand-in for
// `npm run dev`. It scans 1970–2060 weekly and bisects each change to the
// second, so earlier and later dates keep the nearest scanned offset.

import { BackendError } from './backend';
import type { ZoneOffsets, ZoneOffsetsParams, ZoneTransition } from './types';

const SCAN_YEARS: [number, number] = [1970, 2060];
const WEEK_MS = 7 * 86_400_000;

function formatter(timeZone: string, name: 'longOffset' | 'short'): Intl.DateTimeFormat {
  return new Intl.DateTimeFormat('en-US', { timeZone, timeZoneName: name });
}

function part(f: Intl.DateTimeFormat, ms: number): string {
  return f.formatToParts(new Date(ms)).find((p) => p.type === 'timeZoneName')?.value ?? '';
}

/** Seconds east of an Intl `longOffset` value: `GMT`, `GMT+5:30`, `GMT-03:00`. */
function offsetSeconds(f: Intl.DateTimeFormat, ms: number): number {
  const m = /GMT([+-])(\d{1,2})(?::(\d{2}))?(?::(\d{2}))?/.exec(part(f, ms));
  if (!m) return 0;
  const s = Number(m[2]) * 3600 + Number(m[3] ?? 0) * 60 + Number(m[4] ?? 0);
  return m[1] === '-' ? -s : s;
}

export function mockTimeZoneOffsets(p: ZoneOffsetsParams): ZoneOffsets {
  let off: Intl.DateTimeFormat;
  let abbr: Intl.DateTimeFormat;
  try {
    off = formatter(p.timeZone, 'longOffset');
    abbr = formatter(p.timeZone, 'short');
  } catch {
    throw new BackendError('invalid-params', `timeZone "${p.timeZone}" is not an IANA time zone`);
  }
  const from = Math.max(p.fromYear ?? 1800, SCAN_YEARS[0]);
  const to = Math.min(p.toYear ?? 2500, SCAN_YEARS[1]);
  const fromMs = Date.UTC(from, 0, 1);
  const toMs = Date.UTC(to, 0, 1);
  const transitions: ZoneTransition[] = [];
  let prev = offsetSeconds(off, fromMs);
  for (let t = fromMs; t < toMs; t += WEEK_MS) {
    const next = Math.min(t + WEEK_MS, toMs);
    const o = offsetSeconds(off, next);
    if (o === prev) continue;
    // Bisect to the first second with the new offset.
    let lo = t;
    let hi = next;
    while (hi - lo > 1000) {
      const mid = Math.floor((lo + hi) / 2000) * 1000;
      if (offsetSeconds(off, mid) === prev) lo = mid;
      else hi = mid;
    }
    transitions.push({ unixSeconds: hi / 1000, offsetSeconds: o, abbreviation: part(abbr, hi), dst: false });
    prev = o;
  }
  return {
    timeZone: off.resolvedOptions().timeZone,
    tzdbVersion: 'browser',
    fromYear: from,
    toYear: to,
    fromUnixSeconds: fromMs / 1000,
    toUnixSeconds: toMs / 1000,
    initialOffsetSeconds: offsetSeconds(off, fromMs),
    initialAbbreviation: part(abbr, fromMs),
    initialDst: false,
    transitions,
  };
}

export function mockTimeZoneNames(): string[] {
  const intl = Intl as unknown as { supportedValuesOf?: (key: string) => string[] };
  return intl.supportedValuesOf?.('timeZone') ?? ['UTC', 'Europe/London', 'America/New_York', 'Asia/Riyadh', 'Asia/Kolkata'];
}
