// Ephemeris table: Sun and Moon rows at a configurable start, step and count,
// with selectable columns, copyable as CSV or TSV.

import type { EphemerisRow, EphemerisTable, UtDateTime } from '../backend';
import { changed } from '../state';
import { copyText, localDateTime, timeLink, zoneLabel } from './app-time';
import { debounce, h, replaceChildren } from './dom';
import { runner, type EphemerisContext, type SubView } from './ephemeris-view';
import { shiftHours } from './format';
import { localToUtMs, offsetAt, type Tz, tzOf, utMs } from '../zone';

const SETTINGS_KEY = 'mooncalc.ephemeris.table.v1';
/** Rows drawn on screen; copying includes every row. */
const MAX_SHOWN = 1500;
const MAX_ROWS = 10_000;
const AU_KM = 149_597_870.7;

type Unit = 'minutes' | 'hours' | 'days';
type Style = 'sexagesimal' | 'decimal';

interface TableSettings {
  stepValue: number;
  stepUnit: Unit;
  count: number;
  columns: string[];
  topocentric: boolean;
  style: Style;
}

const DEFAULTS: TableSettings = {
  stepValue: 1,
  stepUnit: 'days',
  count: 31,
  columns: ['sunRa', 'sunDec', 'moonRa', 'moonDec', 'moonDist', 'elong', 'illum', 'age', 'moonAlt', 'moonAz'],
  topocentric: true,
  style: 'sexagesimal',
};

const UNIT_MINUTES: Record<Unit, number> = { minutes: 1, hours: 60, days: 1440 };

const pad = (n: number, w = 2) => String(n).padStart(w, '0');

/** "12h 34m 56.7s". */
function hms(deg: number): string {
  let s = Math.round(((((deg / 15) % 24) + 24) % 24) * 36000) / 10;
  if (s >= 86400) s -= 86400;
  const hh = Math.floor(s / 3600);
  const mm = Math.floor((s - hh * 3600) / 60);
  const ss = s - hh * 3600 - mm * 60;
  return `${pad(hh)}h ${pad(mm)}m ${ss.toFixed(1).padStart(4, '0')}s`;
}

/** "+12° 34′ 56″". */
function dms(deg: number, signed = true): string {
  const sign = deg < 0 ? '−' : signed ? '+' : '';
  let s = Math.round(Math.abs(deg) * 3600);
  const d = Math.floor(s / 3600);
  s -= d * 3600;
  const m = Math.floor(s / 60);
  s -= m * 60;
  return `${sign}${d}° ${pad(m)}′ ${pad(s)}″`;
}

interface Col {
  id: string;
  group: string;
  header: string;
  description: string;
  /** The raw value (for CSV); null where unavailable. */
  value: (r: EphemerisRow) => number | null;
  /** Display text. */
  show: (v: number, style: Style) => string;
}

const angle = (digits = 4) => (v: number, style: Style) => (style === 'sexagesimal' ? dms(v) : v.toFixed(digits));
const angleU = (digits = 4) => (v: number, style: Style) => (style === 'sexagesimal' ? dms(v, false) : v.toFixed(digits));
const ra = (v: number, style: Style) => (style === 'sexagesimal' ? hms(v) : (v / 15).toFixed(5));
const fixed = (digits: number) => (v: number) => v.toFixed(digits);

const COLUMNS: Col[] = [
  { id: 'jd', group: 'Time', header: 'JD (UT)', description: 'Julian Day (UT)', value: (r) => r.jdUt, show: fixed(5) },
  { id: 'sunRa', group: 'Sun', header: 'Sun RA', description: 'Geocentric apparent right ascension', value: (r) => r.sun.raDeg, show: ra },
  { id: 'sunDec', group: 'Sun', header: 'Sun Dec', description: 'Geocentric apparent declination', value: (r) => r.sun.decDeg, show: angle() },
  { id: 'sunLon', group: 'Sun', header: 'Sun λ', description: 'Apparent ecliptic longitude (true equinox of date)', value: (r) => r.sun.lonDeg, show: angleU() },
  { id: 'sunLat', group: 'Sun', header: 'Sun β', description: 'Apparent ecliptic latitude', value: (r) => r.sun.latDeg, show: angle(5) },
  { id: 'sunDist', group: 'Sun', header: 'Sun dist (AU)', description: 'Geocentric distance, astronomical units', value: (r) => r.sun.distanceKm / AU_KM, show: fixed(7) },
  { id: 'sunSd', group: 'Sun', header: 'Sun SD (′)', description: 'Geocentric semi-diameter, arcminutes', value: (r) => r.sun.semidiameterArcmin, show: fixed(3) },
  { id: 'sunHp', group: 'Sun', header: 'Sun HP (″)', description: 'Equatorial horizontal parallax, arcseconds', value: (r) => r.sun.parallaxArcmin * 60, show: fixed(2) },
  { id: 'moonRa', group: 'Moon', header: 'Moon RA', description: 'Geocentric apparent right ascension', value: (r) => r.moon.raDeg, show: ra },
  { id: 'moonDec', group: 'Moon', header: 'Moon Dec', description: 'Geocentric apparent declination', value: (r) => r.moon.decDeg, show: angle() },
  { id: 'moonLon', group: 'Moon', header: 'Moon λ', description: 'Apparent ecliptic longitude (true equinox of date)', value: (r) => r.moon.lonDeg, show: angleU() },
  { id: 'moonLat', group: 'Moon', header: 'Moon β', description: 'Apparent ecliptic latitude', value: (r) => r.moon.latDeg, show: angle() },
  { id: 'moonDist', group: 'Moon', header: 'Moon dist (km)', description: 'Geocentric distance, km', value: (r) => r.moon.distanceKm, show: fixed(0) },
  { id: 'moonSd', group: 'Moon', header: 'Moon SD (′)', description: 'Geocentric semi-diameter, arcminutes', value: (r) => r.moon.semidiameterArcmin, show: fixed(3) },
  { id: 'moonHp', group: 'Moon', header: 'Moon HP (′)', description: 'Equatorial horizontal parallax, arcminutes', value: (r) => r.moon.parallaxArcmin, show: fixed(3) },
  { id: 'elong', group: 'Phase', header: 'Elongation', description: 'Geocentric elongation of the Moon from the Sun', value: (r) => r.elongationDeg, show: angleU(3) },
  { id: 'phaseAngle', group: 'Phase', header: 'Phase angle', description: 'Sun–Moon–Earth angle', value: (r) => r.phaseAngleDeg, show: angleU(3) },
  { id: 'illum', group: 'Phase', header: 'Illum. (%)', description: 'Illuminated fraction of the disc', value: (r) => r.illuminatedFraction * 100, show: fixed(2) },
  { id: 'limb', group: 'Phase', header: 'Bright limb χ', description: 'Position angle of the bright limb (from north, eastward)', value: (r) => r.brightLimbDeg, show: (v) => v.toFixed(2) + '°' },
  { id: 'age', group: 'Phase', header: 'Age (d)', description: 'Days since the last new Moon (conjunction)', value: (r) => r.moonAgeDays, show: fixed(3) },
  { id: 'sunAlt', group: 'Topocentric', header: 'Sun alt', description: 'Altitude with standard refraction, app location', value: (r) => r.sun.topocentric?.altDeg ?? null, show: angle(3) },
  { id: 'sunAz', group: 'Topocentric', header: 'Sun az', description: 'Azimuth (north through east), app location', value: (r) => r.sun.topocentric?.azDeg ?? null, show: angleU(3) },
  { id: 'moonAlt', group: 'Topocentric', header: 'Moon alt', description: 'Altitude with standard refraction, app location', value: (r) => r.moon.topocentric?.altDeg ?? null, show: angle(3) },
  { id: 'moonAz', group: 'Topocentric', header: 'Moon az', description: 'Azimuth (north through east), app location', value: (r) => r.moon.topocentric?.azDeg ?? null, show: angleU(3) },
  { id: 'moonTopoSd', group: 'Topocentric', header: 'Moon topo SD (′)', description: 'Topocentric semi-diameter of the Moon', value: (r) => r.moon.topocentric?.semidiameterArcmin ?? null, show: fixed(3) },
  { id: 'topoElong', group: 'Topocentric', header: 'Topo elong.', description: 'Elongation seen from the app location', value: (r) => r.topocentricElongationDeg, show: angleU(3) },
  { id: 'libLon', group: 'Libration', header: 'Lib. l', description: 'Total geocentric libration in longitude', value: (r) => r.libration.lonDeg, show: (v) => `${v >= 0 ? '+' : '−'}${Math.abs(v).toFixed(3)}°` },
  { id: 'libLat', group: 'Libration', header: 'Lib. b', description: 'Total geocentric libration in latitude', value: (r) => r.libration.latDeg, show: (v) => `${v >= 0 ? '+' : '−'}${Math.abs(v).toFixed(3)}°` },
  { id: 'axisP', group: 'Libration', header: 'Axis P', description: "Position angle of the Moon's axis", value: (r) => r.libration.axisPositionAngleDeg, show: (v) => v.toFixed(3) + '°' },
  { id: 'colong', group: 'Libration', header: 'Colongitude', description: 'Selenographic colongitude of the Sun', value: (r) => r.libration.colongitudeDeg, show: (v) => v.toFixed(3) + '°' },
];

const GROUPS = ['Time', 'Sun', 'Moon', 'Phase', 'Topocentric', 'Libration'];

function load(): TableSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return structuredClone(DEFAULTS);
    const s = { ...DEFAULTS, ...(JSON.parse(raw) as Partial<TableSettings>) };
    s.columns = s.columns.filter((c) => COLUMNS.some((k) => k.id === c));
    return s;
  } catch {
    return structuredClone(DEFAULTS);
  }
}

function save(s: TableSettings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // Not remembered.
  }
}

/** "YYYY-MM-DDTHH:MM" of a UT instant in the zone, for a datetime-local input. */
function toInputValue(t: UtDateTime, tz: Tz): string {
  const l = shiftHours({ ...t, second: 0 }, offsetAt(tz, t));
  return `${pad(l.year, 4)}-${pad(l.month)}-${pad(l.day)}T${pad(l.hour)}:${pad(l.minute)}`;
}

/** The UT instant of a local datetime-local value (a time skipped by a DST
 * change moves forward; a repeated one takes its first occurrence). */
function fromInputValue(v: string, tz: Tz): UtDateTime | null {
  const m = /^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(v);
  if (!m) return null;
  const local = { year: Number(m[1]), month: Number(m[2]), day: Number(m[3]), hour: Number(m[4]), minute: Number(m[5]), second: 0 };
  const ms = localToUtMs(tz, utMs(local));
  return shiftHours(local, (ms - utMs(local)) / 3_600_000);
}

export function createTableView({ backend, store }: EphemerisContext): SubView {
  const s = load();
  const status = h('span', { class: 'status' });
  const run = runner(status);
  const out = h('div', { class: 'table-wrap' });
  let last: EphemerisTable | null = null;
  let stale = true;
  let visible = false;

  const start = h('input', { type: 'datetime-local', 'aria-label': 'Start (local time)', class: 'dt-input' });
  // The start follows the app's date and time until edited.
  let pinnedStart = false;
  const startLabel = h('span', {});
  const fromApp = () => {
    const st = store.get();
    start.value = toInputValue(st.instant, tzOf(st));
    startLabel.textContent = `Start (${zoneLabel(tzOf(st))}) `;
  };
  fromApp();
  const stepValue = h('input', { type: 'number', min: 0.1, step: 'any', value: String(s.stepValue), 'aria-label': 'Step', class: 'num-input' });
  const stepUnit = h('select', { 'aria-label': 'Step unit' }, ...(['minutes', 'hours', 'days'] as Unit[]).map((u) => h('option', { value: u, selected: u === s.stepUnit }, u)));
  const count = h('input', { type: 'number', min: 1, max: MAX_ROWS, step: 1, value: String(s.count), 'aria-label': 'Rows', class: 'num-input' });
  const topo = h('input', { type: 'checkbox', checked: s.topocentric, 'aria-label': 'Topocentric columns' });
  const style = h('select', { 'aria-label': 'Angle format' }, h('option', { value: 'sexagesimal', selected: s.style === 'sexagesimal' }, '12h 34m / +12° 34′'), h('option', { value: 'decimal', selected: s.style === 'decimal' }, 'Decimal'));

  const columnsMenu = h(
    'div',
    { class: 'columns-menu' },
    ...GROUPS.map((g) =>
      h(
        'fieldset',
        {},
        h('legend', {}, g),
        ...COLUMNS.filter((c) => c.group === g).map((c) => {
          const box = h('input', { type: 'checkbox', checked: s.columns.includes(c.id) });
          box.addEventListener('change', () => {
            s.columns = COLUMNS.filter((k) => (k.id === c.id ? box.checked : s.columns.includes(k.id))).map((k) => k.id);
            save(s);
            if (last) render(last);
          });
          return h('label', { title: c.description }, box, ` ${c.header}`);
        }),
      ),
    ),
  );
  const columns = h('details', { class: 'columns-picker' }, h('summary', { class: 'button small' }, 'Columns'), columnsMenu);

  const params = () => {
    const st = store.get();
    const t = fromInputValue(start.value, tzOf(st));
    const step = Math.max(0.1, Number(stepValue.value) || 1) * UNIT_MINUTES[stepUnit.value as Unit];
    const n = Math.min(MAX_ROWS, Math.max(1, Math.round(Number(count.value) || 1)));
    return t ? { start: t, stepMinutes: step, count: n, location: topo.checked ? st.location : null, ephemerisId: st.ephemerisId } : null;
  };

  const compute = () => {
    const p = params();
    if (!p) {
      replaceChildren(status, h('span', { class: 'error' }, 'Enter a start date and time'));
      return;
    }
    stale = false;
    void run(`Computing ${p.count} rows`, () => backend.tables.ephemerisTable(p), (t) => {
      last = t;
      render(t);
    });
  };
  const recompute = debounce(() => {
    stale = true;
    if (visible) compute();
  }, 350);

  const onSetting = () => {
    s.stepValue = Number(stepValue.value) || 1;
    s.stepUnit = stepUnit.value as Unit;
    s.count = Math.round(Number(count.value) || 1);
    s.topocentric = topo.checked;
    s.style = style.value as Style;
    save(s);
  };
  start.addEventListener('change', () => {
    pinnedStart = true;
  });
  for (const el of [start, stepValue, stepUnit, count, topo]) {
    el.addEventListener('change', () => {
      onSetting();
      recompute();
    });
  }
  style.addEventListener('change', () => {
    onSetting();
    if (last) render(last);
  });

  store.subscribe((st, prev) => {
    if (changed(st, prev, ['ephemerisId']) || (topo.checked && changed(st, prev, ['location']))) recompute();
    if (!pinnedStart && changed(st, prev, ['instant', 'tzOffsetHours', 'timeZone'])) {
      fromApp();
      recompute();
    } else if (changed(st, prev, ['tzOffsetHours', 'timeZone'])) {
      // Keep the same instant; show it in the new zone.
      const t = fromInputValue(start.value, tzOf(prev));
      if (t) start.value = toInputValue(t, tzOf(st));
      startLabel.textContent = `Start (${zoneLabel(tzOf(st))}) `;
      if (last) render(last);
    }
  });

  const selected = () => COLUMNS.filter((c) => s.columns.includes(c.id));

  const toText = (sep: string) => {
    if (!last) return '';
    const tz = tzOf(store.get());
    const cols = selected();
    const quote = (v: string) => (sep === ',' && /[",\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);
    const lines = [[`Time (${zoneLabel(tz)})`, 'Time (UT)', ...cols.map((c) => c.header)].map(quote).join(sep)];
    for (const r of last.rows) {
      const ut = `${r.time.year}-${pad(r.time.month)}-${pad(r.time.day)} ${pad(r.time.hour)}:${pad(r.time.minute)}`;
      lines.push([localDateTime(r.time, tz), ut, ...cols.map((c) => {
        const v = c.value(r);
        return v === null ? '' : String(Number(v.toPrecision(12)));
      })].map(quote).join(sep));
    }
    return lines.join('\n') + '\n';
  };
  const copied = h('span', { class: 'hint' });
  const copy = (sep: string, label: string) => async () => {
    try {
      await copyText(toText(sep));
      copied.textContent = `${label} copied (${last?.rows.length ?? 0} rows).`;
    } catch (err) {
      copied.textContent = `Copy failed: ${String(err)}`;
    }
  };

  const render = (t: EphemerisTable) => {
    const tz = tzOf(store.get());
    const cols = selected();
    const st = (style.value as Style) ?? 'sexagesimal';
    const shown = t.rows.slice(0, MAX_SHOWN);
    replaceChildren(
      out,
      h(
        'table',
        { class: 'criteria-table eph-table' },
        h('thead', {}, h('tr', {}, h('th', {}, `Date (${zoneLabel(tz)})`), ...cols.map((c) => h('th', { class: 'num', title: c.description }, c.header)))),
        h(
          'tbody',
          {},
          ...shown.map((r) =>
            h(
              'tr',
              {},
              h('td', {}, timeLink(store, r.time, localDateTime(r.time, tz))),
              ...cols.map((c) => {
                const v = c.value(r);
                return h('td', { class: 'num' }, v === null ? '—' : c.show(v, st));
              }),
            ),
          ),
        ),
      ),
      t.rows.length > shown.length ? h('p', { class: 'hint' }, `Showing the first ${shown.length} of ${t.rows.length} rows; copying includes all of them.`) : '',
      h('p', { class: 'hint' }, `Ephemeris: ${t.ephemeris}. Geocentric apparent places, true equinox of date. Topocentric altitudes include standard refraction. Click a time to set the app date.`),
    );
  };

  const el = h(
    'div',
    { class: 'eph-sub' },
    h(
      'div',
      { class: 'toolbar eph-toolbar' },
      h('label', { class: 'inline' }, startLabel, start),
      h('button', { type: 'button', class: 'button small', title: "Follow the app's date and time", onclick: () => {
        pinnedStart = false;
        fromApp();
        recompute();
      } }, 'App time'),
      h('label', { class: 'inline' }, 'Step ', stepValue, stepUnit),
      h('label', { class: 'inline' }, 'Rows ', count),
      h('label', { class: 'inline' }, topo, ' Topocentric (app location)'),
      h('label', { class: 'inline' }, 'Angles ', style),
      columns,
      status,
    ),
    h(
      'div',
      { class: 'toolbar eph-toolbar' },
      h('button', { type: 'button', class: 'button small', onclick: copy(',', 'CSV') }, 'Copy CSV'),
      h('button', { type: 'button', class: 'button small', onclick: copy('\t', 'TSV') }, 'Copy TSV'),
      copied,
    ),
    out,
  );

  return {
    el,
    show: () => {
      visible = true;
      if (stale) compute();
    },
  };
}
