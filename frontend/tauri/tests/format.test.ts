// Formatter checks: `npm test` (node --test, no extra dependencies).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatOffset, formatUtDateTime, roundToMinute } from '../src/ui/format.ts';
import { fixedTz, formatOffset as zoneFormatOffset } from '../src/zone.ts';

const t = (year: number, month: number, day: number, hour: number, minute: number, second: number) => ({
  year,
  month,
  day,
  hour,
  minute,
  second,
});

test('event times round to the nearest minute', () => {
  assert.equal(formatUtDateTime(t(2024, 3, 10, 18, 42, 29.9)), '2024-03-10 18:42 UT');
  assert.equal(formatUtDateTime(t(2024, 3, 10, 18, 42, 30)), '2024-03-10 18:43 UT');
  assert.equal(formatUtDateTime(t(2024, 3, 10, 18, 42, 59.999)), '2024-03-10 18:43 UT');
});

test('rounding carries into the hour, day, month and year', () => {
  assert.equal(formatUtDateTime(t(2024, 3, 10, 18, 59, 45)), '2024-03-10 19:00 UT');
  assert.equal(formatUtDateTime(t(2024, 3, 10, 23, 59, 31)), '2024-03-11 00:00 UT');
  assert.equal(formatUtDateTime(t(2024, 2, 28, 23, 59, 40)), '2024-02-29 00:00 UT');
  assert.equal(formatUtDateTime(t(2023, 2, 28, 23, 59, 40)), '2023-03-01 00:00 UT');
  assert.equal(formatUtDateTime(t(2100, 2, 28, 23, 59, 40)), '2100-03-01 00:00 UT');
  assert.equal(formatUtDateTime(t(2000, 2, 28, 23, 59, 40)), '2000-02-29 00:00 UT');
  assert.equal(formatUtDateTime(t(2024, 12, 31, 23, 59, 30)), '2025-01-01 00:00 UT');
  assert.equal(formatUtDateTime(t(2024, 4, 30, 23, 59, 30)), '2024-05-01 00:00 UT');
});

test('the local time follows the rounded UT time', () => {
  // 23:59:40 UT rounds to 00:00 UT the next day; UT+3 is 03:00.
  assert.equal(formatUtDateTime(t(2024, 3, 10, 23, 59, 40), fixedTz(3)), '2024-03-11 00:00 UT (03:00 UT+3)');
  assert.equal(formatUtDateTime(t(2024, 3, 10, 18, 42, 30), fixedTz(-5.5)), '2024-03-10 18:43 UT (13:13 UT−5:30)');
  assert.equal(formatUtDateTime(null, fixedTz(3)), '—');
});

test('the zone module formats offsets like the UI', () => {
  for (const h of [0, 3, -5.5, 5.75, 12.75, -9.5, 14]) assert.equal(zoneFormatOffset(h), formatOffset(h));
});

test('Julian dates before 1582-10-15 keep their own leap years and the reform gap', () => {
  // 1500 is a Julian leap year (not Gregorian).
  assert.deepEqual(roundToMinute(t(1500, 2, 28, 23, 59, 30)), t(1500, 2, 29, 0, 0, 0));
  assert.deepEqual(roundToMinute(t(1582, 10, 4, 23, 59, 30)), t(1582, 10, 15, 0, 0, 0));
  assert.deepEqual(roundToMinute(t(-44, 3, 14, 23, 59, 50)), t(-44, 3, 15, 0, 0, 0));
  assert.deepEqual(roundToMinute(t(2024, 3, 10, 6, 7, 8)), t(2024, 3, 10, 6, 7, 0));
});
