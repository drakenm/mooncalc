// Frontend time zones (src/zone.ts), against tables shaped like the Rust
// `time_zone_offsets` output: `npm test`.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { UtDateTime, ZoneOffsets } from '../src/backend/types.ts';
import { formatUtDateTime } from '../src/ui/format.ts';
import { fixedTz, localDateOf, localToUtMs, noonOffset, offsetAt, utMs, zoneAbbrev, zoneLabel, ZoneTable } from '../src/zone.ts';

const H = 3_600_000;
const s = (iso: string) => Date.parse(iso) / 1000;
const t = (year: number, month: number, day: number, hour = 0, minute = 0): UtDateTime => ({ year, month, day, hour, minute, second: 0 });

/** Europe/London around 2024 (EU rule: last Sundays of March and October, 01:00 UT). */
const LONDON: ZoneOffsets = {
  timeZone: 'Europe/London',
  tzdbVersion: '2026e',
  fromYear: 2023,
  toYear: 2025,
  fromUnixSeconds: s('2023-01-01T00:00:00Z'),
  toUnixSeconds: s('2025-01-01T00:00:00Z'),
  initialOffsetSeconds: 0,
  initialAbbreviation: 'GMT',
  initialDst: false,
  transitions: [
    { unixSeconds: s('2023-03-26T01:00:00Z'), offsetSeconds: 3600, abbreviation: 'BST', dst: true },
    { unixSeconds: s('2023-10-29T01:00:00Z'), offsetSeconds: 0, abbreviation: 'GMT', dst: false },
    { unixSeconds: s('2024-03-31T01:00:00Z'), offsetSeconds: 3600, abbreviation: 'BST', dst: true },
    { unixSeconds: s('2024-10-27T01:00:00Z'), offsetSeconds: 0, abbreviation: 'GMT', dst: false },
  ],
};

test('offsets follow the transitions', () => {
  const z = new ZoneTable(LONDON);
  assert.equal(z.offsetAtMs(Date.parse('2024-03-31T00:59:59Z')), 0);
  assert.equal(z.offsetAtMs(Date.parse('2024-03-31T01:00:00Z')), 1);
  assert.equal(z.abbrevAtMs(Date.parse('2024-07-01T12:00:00Z')), 'BST');
  assert.equal(z.offsetAtMs(Date.parse('2024-10-27T01:00:00Z')), 0);
  // Before the table: the initial offset.
  assert.equal(z.offsetAtMs(Date.parse('1990-07-01T12:00:00Z')), 0);
  assert.equal(zoneLabel(z), 'Europe/London');
  assert.equal(zoneAbbrev(z, Date.parse('2024-07-01T12:00:00Z')), 'BST');
});

test('instants after the table fold back by 400-year cycles', () => {
  // A table whose last 400 years are 2100–2500 in the real service; here a
  // two-year table folds 2424 back onto 2024 (the calendar repeats).
  const z = new ZoneTable({ ...LONDON, toUnixSeconds: s('2025-01-01T00:00:00Z') });
  assert.equal(z.offsetAtMs(Date.parse('2424-07-01T12:00:00Z')), 1);
  assert.equal(z.offsetAtMs(Date.parse('2424-12-01T12:00:00Z')), 0);
  assert.equal(z.offsetAtMs(Date.parse('2424-03-31T00:59:00Z')), 0);
  assert.equal(z.offsetAtMs(Date.parse('2424-03-31T01:00:00Z')), 1);
});

test('local to UT handles gaps and folds', () => {
  const z = new ZoneTable(LONDON);
  const local = (iso: string) => Date.parse(`${iso}Z`);
  // An ordinary time.
  assert.equal(localToUtMs(z, local('2024-07-01T12:00:00')), Date.parse('2024-07-01T11:00:00Z'));
  // 01:30 on 31 March was skipped: it moves forward to 02:30 BST (01:30 UT).
  assert.equal(localToUtMs(z, local('2024-03-31T01:30:00')), Date.parse('2024-03-31T01:30:00Z'));
  // 01:30 on 27 October happened twice: the first, BST (00:30 UT).
  assert.equal(localToUtMs(z, local('2024-10-27T01:30:00')), Date.parse('2024-10-27T00:30:00Z'));
  // Round trip every 10 minutes across both changes.
  for (const change of ['2024-03-31T01:00:00Z', '2024-10-27T01:00:00Z']) {
    for (let m = -120; m <= 120; m += 10) {
      const ut = Date.parse(change) + m * 60_000;
      const back = localToUtMs(z, ut + z.offsetAtMs(ut) * H);
      const inFold = back !== ut && z.offsetAtMs(back) !== z.offsetAtMs(ut);
      assert.ok(back === ut || inFold, `${change} ${m}`);
    }
  }
});

test('local dates and civil noon use the offset of the day', () => {
  const z = new ZoneTable(LONDON);
  // 23:30 UT on 30 June is 00:30 BST on 1 July.
  assert.deepEqual(localDateOf(z, t(2024, 6, 30, 23, 30)), { year: 2024, month: 7, day: 1 });
  assert.deepEqual(localDateOf(z, t(2024, 1, 31, 23, 30)), { year: 2024, month: 1, day: 31 });
  assert.equal(noonOffset(z, { year: 2024, month: 3, day: 30 }), 0);
  assert.equal(noonOffset(z, { year: 2024, month: 3, day: 31 }), 1);
  // A time's own offset (set by a zone-aware service) wins.
  assert.equal(offsetAt(z, { ...t(2024, 7, 1, 12), offsetHours: 5.5 }), 5.5);
  assert.equal(offsetAt(z, t(2024, 7, 1, 12)), 1);
});

test('fixed offsets and labels', () => {
  const k = fixedTz(5.5);
  assert.equal(k.offsetAtMs(0), 5.5);
  assert.equal(zoneLabel(k), 'UT+5:30');
  assert.equal(zoneAbbrev(k, 0), 'UT+5:30');
  assert.equal(localToUtMs(k, utMs(t(2024, 3, 10, 5, 30))), utMs(t(2024, 3, 10)));
  // A zone with a numeric abbreviation shows the offset instead.
  const riyadh = new ZoneTable({ ...LONDON, timeZone: 'Asia/Riyadh', initialOffsetSeconds: 10_800, initialAbbreviation: '+03', transitions: [] });
  assert.equal(zoneAbbrev(riyadh, 0), 'UT+3');
});

test('formatUtDateTime shows the local time of each instant', () => {
  const z = new ZoneTable(LONDON);
  assert.equal(formatUtDateTime(t(2024, 3, 30, 18, 0), z), '2024-03-30 18:00 UT (18:00 GMT)');
  assert.equal(formatUtDateTime(t(2024, 3, 31, 18, 0), z), '2024-03-31 18:00 UT (19:00 BST)');
  assert.equal(formatUtDateTime(t(2024, 3, 10, 18, 0), fixedTz(3)), '2024-03-10 18:00 UT (21:00 UT+3)');
  assert.equal(formatUtDateTime(t(2024, 3, 10, 18, 0), fixedTz(0)), '2024-03-10 18:00 UT');
});
