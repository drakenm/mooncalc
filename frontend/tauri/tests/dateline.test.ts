// The TypeScript mirror of mooncalc_app::dateline: `npm test`.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mapConvention, meanTimeDayShift, observerNote, reportNote, utcOffsetText } from '../src/dateline.ts';

const DATE = { year: 2024, month: 3, day: 10 };

test('day shift matches mooncalc_core::events::mean_time_day_shift', () => {
  // Apia, Nuku'alofa, Kiritimati, the Chatham Islands, 180° on UTC+12.
  for (const [lon, tz] of [[-171.7667, 13], [-175.2011, 13], [-157.4, 14], [-176.56, 12.75], [180, 12]] as const) {
    assert.equal(meanTimeDayShift(lon, tz), -1, `${lon} ${tz}`);
  }
  // London, Ürümqi on UTC+8, Tarawa, Suva, Honolulu, 180° on UTC−12.
  for (const [lon, tz] of [[-0.1276, 0], [87.6, 8], [172.977, 12], [178.44, 12], [-157.86, -10], [180, -12]] as const) {
    assert.equal(meanTimeDayShift(lon, tz), 0, `${lon} ${tz}`);
  }
  assert.equal(meanTimeDayShift(173, -10), 1);
});

test('notes use the Rust wording', () => {
  const n = reportNote(DATE, -171.7667, 13);
  assert.ok(n);
  assert.deepEqual(n.mapDate, { year: 2024, month: 3, day: 9 });
  assert.equal(n.dayShift, -1);
  assert.equal(
    n.text,
    "The world map for 2024-03-09 shows this evening and morning at your longitude; the map's date line is the " +
      '180° meridian (HMNAO convention), and your civil date (UTC+13) runs a day ahead of it. ' +
      'The map for 2024-03-10 shows the evening of 2024-03-11 here.',
  );
  assert.equal(reportNote(DATE, -0.13, 0), null);
  assert.equal(
    observerNote(DATE, -171.7667, 13, 'evening'),
    'At the observer this map shows the evening of local date 2024-03-11 (UTC+13), not of 2024-03-10: ' +
      "the map's date line is the 180° meridian. Their evening of 2024-03-10 is on the map for 2024-03-09.",
  );
  assert.equal(observerNote(DATE, 87.6, 8, 'evening'), null);
  assert.equal(
    mapConvention(DATE, 'morning'),
    'Each location is shown on its own local morning of 2024-03-10; the date line is the 180° meridian (HMNAO convention).',
  );
  assert.equal(utcOffsetText(12.75), 'UTC+12:45');
  assert.equal(utcOffsetText(-5.5), 'UTC−5:30');
});
