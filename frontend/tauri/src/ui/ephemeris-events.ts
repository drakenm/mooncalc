// Ephemeris sub-views listing events: rise/set for a month, lunar phases,
// apsides and nodes, and seasons with the equation of time.

import type { ApsidesNodesYear, LunarPhasesYear, RiseSetDay, RiseSetMonth, SeasonsYear, TwilightTimes, UtDateTime } from '../backend';
import { changed, localDate, type Store } from '../state';
import { localDayTime, MONTH_ABBR, MONTH_NAMES, timeLink, yearStepper, zoneLabel } from './app-time';
import { debounce, h, replaceChildren } from './dom';
import { runner, type EphemerisContext, type SubView } from './ephemeris-view';
import { formatLat, formatLon } from './format';

/** Shared plumbing: recompute when shown and the inputs changed. */
function lazy(compute: () => void): { show(): void; invalidate(): void } {
  let stale = true;
  let shown = false;
  const again = debounce(() => {
    if (shown) {
      stale = false;
      compute();
    }
  }, 200);
  return {
    show: () => {
      shown = true;
      if (stale) {
        stale = false;
        compute();
      }
    },
    invalidate: () => {
      stale = true;
      again();
    },
  };
}

/** A year selector that follows the app's local year until edited. */
function followingYear(store: Store, onChange: () => void) {
  let pinned: number | null = null;
  const appYear = () => localDate(store.get()).year;
  const follow = h('button', { type: 'button', class: 'button small', hidden: true, title: "Follow the app's date" }, 'App year');
  const stepper = yearStepper(appYear(), (y) => {
    pinned = y;
    follow.hidden = false;
    onChange();
  });
  follow.addEventListener('click', () => {
    pinned = null;
    follow.hidden = true;
    stepper.set(appYear());
    onChange();
  });
  store.subscribe((s, prev) => {
    if (pinned === null && localDate(s).year !== localDate(prev).year) {
      stepper.set(appYear());
      onChange();
    }
  });
  return { el: h('span', { class: 'inline-row year-row' }, h('span', { class: 'muted' }, 'Year'), stepper.el, follow), year: () => pinned ?? appYear() };
}

const deg = (v: number, digits = 1) => `${v.toFixed(digits)}°`;

// --- rise/set ----------------------------------------------------------------

export function createRiseSetView({ backend, store }: EphemerisContext): SubView {
  const status = h('span', { class: 'status' });
  const run = runner(status);
  const out = h('div', { class: 'table-wrap' });
  let pinned: { year: number; month: number } | null = null;
  const app = () => {
    const d = localDate(store.get());
    return { year: d.year, month: d.month };
  };
  const monthSel = h('select', { 'aria-label': 'Month' }, ...MONTH_NAMES.map((m, i) => h('option', { value: i + 1 }, m)));
  const yearIn = h('input', { type: 'number', min: -4000, max: 9999, step: 1, 'aria-label': 'Year', class: 'year-input' });
  const follow = h('button', { type: 'button', class: 'button small', hidden: true }, 'App month');
  const current = () => pinned ?? app();
  const syncInputs = () => {
    const c = current();
    monthSel.value = String(c.month);
    yearIn.value = String(c.year);
    follow.hidden = pinned === null;
  };
  const pin = (year: number, month: number) => {
    const i = year * 12 + month - 1;
    pinned = { year: Math.floor(i / 12), month: (i % 12) + 1 };
    syncInputs();
    l.invalidate();
  };
  monthSel.addEventListener('change', () => pin(Number(yearIn.value), Number(monthSel.value)));
  yearIn.addEventListener('change', () => pin(Number(yearIn.value), Number(monthSel.value)));
  follow.addEventListener('click', () => {
    pinned = null;
    syncInputs();
    l.invalidate();
  });

  const compute = () => {
    const s = store.get();
    const c = current();
    syncInputs();
    void run('Computing rise and set', () => backend.tables.riseSetMonth({ year: c.year, month: c.month, tzOffsetHours: s.tzOffsetHours, location: s.location, ephemerisId: s.ephemerisId }), render);
  };
  const l = lazy(compute);
  store.subscribe((s, prev) => {
    if (changed(s, prev, ['location', 'tzOffsetHours', 'ephemerisId'])) l.invalidate();
    else if (pinned === null && changed(s, prev, ['instant'])) {
      const a = localDate(s);
      const b = localDate(prev);
      if (a.year !== b.year || a.month !== b.month) l.invalidate();
    }
  });

  const twi = (t: TwilightTimes) => h('td', { class: 'num' }, timeLink(store, t.dawn), ' – ', timeLink(store, t.dusk));
  const ev = (e: { time: UtDateTime; angleDeg: number } | null, digits = 0) =>
    h('td', { class: 'num' }, e ? h('span', {}, timeLink(store, e.time), h('span', { class: 'muted angle' }, ` ${deg(e.angleDeg, digits)}`)) : h('span', { class: 'muted' }, '—'));
  const circ = (c: string | null) => (c === 'always-up' ? 'up all day' : c === 'always-down' ? 'down all day' : '—');

  const render = (m: RiseSetMonth) => {
    const s = store.get();
    const sel = localDate(s);
    const row = (d: RiseSetDay) =>
      h(
        'tr',
        { class: d.date.day === sel.day && d.date.month === sel.month && d.date.year === sel.year ? 'selected' : '' },
        h('td', {}, `${d.date.day} ${MONTH_ABBR[d.date.month - 1]}`),
        ...(d.sun.circumpolar ? [h('td', { colspan: 3, class: 'muted' }, `Sun ${circ(d.sun.circumpolar)}`)] : [ev(d.sun.rise), ev(d.sun.transit, 1), ev(d.sun.set)]),
        ...(d.moon.circumpolar ? [h('td', { colspan: 3, class: 'muted' }, `Moon ${circ(d.moon.circumpolar)}`)] : [ev(d.moon.rise), ev(d.moon.transit, 1), ev(d.moon.set)]),
        twi(d.civil),
        twi(d.nautical),
        twi(d.astronomical),
      );
    replaceChildren(
      out,
      h(
        'table',
        { class: 'criteria-table eph-table riseset-table' },
        h(
          'thead',
          {},
          h('tr', {}, h('th', {}, ''), h('th', { colspan: 3 }, 'Sun'), h('th', { colspan: 3 }, 'Moon'), h('th', { colspan: 3 }, 'Twilight (dawn – dusk)')),
          h(
            'tr',
            {},
            h('th', {}, `Date (${zoneLabel(m.tzOffsetHours)})`),
            ...['Rise (az)', 'Transit (alt)', 'Set (az)', 'Rise (az)', 'Transit (alt)', 'Set (az)', 'Civil −6°', 'Nautical −12°', 'Astronomical −18°'].map((t) => h('th', { class: 'num' }, t)),
          ),
        ),
        h('tbody', {}, ...m.days.map(row)),
      ),
      h(
        'p',
        { class: 'hint' },
        `${s.locationName ?? 'Custom location'} · ${formatLat(m.location.latDeg, 3)} ${formatLon(m.location.lonDeg, 3)} · ${MONTH_NAMES[m.month - 1]} ${m.year} · ${m.ephemeris}. `,
        'Rise and set: upper limb, 34′ refraction, level horizon; transit altitude with refraction. Click a time to set the app date and time.',
      ),
    );
  };

  syncInputs();
  const el = h(
    'div',
    { class: 'eph-sub' },
    h(
      'div',
      { class: 'toolbar eph-toolbar' },
      h('button', { type: 'button', class: 'button small', title: 'Previous month', onclick: () => pin(current().year, current().month - 1) }, '«'),
      monthSel,
      yearIn,
      h('button', { type: 'button', class: 'button small', title: 'Next month', onclick: () => pin(current().year, current().month + 1) }, '»'),
      follow,
      status,
    ),
    out,
  );
  return { el, show: l.show };
}

// --- lunar phases ------------------------------------------------------------

const PHASES = ['New Moon', 'First quarter', 'Full Moon', 'Last quarter'];
const PHASE_ICONS = ['●', '◐', '○', '◑'];

export function createPhasesView({ backend, store }: EphemerisContext): SubView {
  const status = h('span', { class: 'status' });
  const run = runner(status);
  const out = h('div', { class: 'table-wrap' });
  const year = followingYear(store, () => l.invalidate());
  const compute = () => {
    const s = store.get();
    void run('Computing phases', () => backend.tables.lunarPhases({ year: year.year(), tzOffsetHours: s.tzOffsetHours, ephemerisId: s.ephemerisId }), render);
  };
  const l = lazy(compute);
  store.subscribe((s, prev) => {
    if (changed(s, prev, ['tzOffsetHours', 'ephemerisId'])) l.invalidate();
  });
  const render = (y: LunarPhasesYear) => {
    const tz = store.get().tzOffsetHours;
    replaceChildren(
      out,
      h(
        'table',
        { class: 'criteria-table eph-table' },
        h(
          'thead',
          {},
          h('tr', {}, h('th', { class: 'num' }, 'Lunation'), ...PHASES.map((p, i) => h('th', {}, `${PHASE_ICONS[i]} ${p}`)), h('th', { class: 'num' }, 'Length')),
        ),
        h(
          'tbody',
          {},
          ...y.lunations.map((lu) =>
            h(
              'tr',
              {},
              h('td', { class: 'num' }, String(lu.k)),
              ...lu.phases.map((t, i) =>
                h('td', {}, t ? h('span', {}, timeLink(store, t, localDayTime(t, tz)), i > 0 && lu.agesDays[i] != null ? h('span', { class: 'muted angle' }, ` ${lu.agesDays[i]!.toFixed(2)} d`) : '') : h('span', { class: 'muted' }, '—')),
              ),
              h('td', { class: 'num' }, `${Math.floor(lu.lengthDays)}d ${((lu.lengthDays % 1) * 24).toFixed(1)}h`),
            ),
          ),
        ),
      ),
      h('p', { class: 'hint' }, `Times ${zoneLabel(tz)}; the Moon's age at each phase after the new Moon in grey. Lunation numbers are Meeus/NASA's (0 = 2000 January 6). ${y.ephemeris}. Click a time to set the app date and time.`),
    );
  };
  const el = h('div', { class: 'eph-sub' }, h('div', { class: 'toolbar eph-toolbar' }, year.el, status), out);
  return { el, show: l.show };
}

// --- apsides and nodes ---------------------------------------------------------

export function createApsidesView({ backend, store }: EphemerisContext): SubView {
  const status = h('span', { class: 'status' });
  const run = runner(status);
  const out = h('div', { class: 'apsides-grid' });
  const year = followingYear(store, () => l.invalidate());
  const compute = () => {
    const s = store.get();
    void run('Computing apsides and nodes', () => backend.tables.apsidesNodes({ year: year.year(), tzOffsetHours: s.tzOffsetHours, ephemerisId: s.ephemerisId }), render);
  };
  const l = lazy(compute);
  store.subscribe((s, prev) => {
    if (changed(s, prev, ['tzOffsetHours', 'ephemerisId'])) l.invalidate();
  });
  const table = (title: string, heads: string[], rows: HTMLElement[]) =>
    h('section', {}, h('h3', {}, title), h('table', { class: 'criteria-table eph-table' }, h('thead', {}, h('tr', {}, ...heads.map((t, i) => h('th', { class: i ? 'num' : '' }, t)))), h('tbody', {}, ...rows)));
  const render = (y: ApsidesNodesYear) => {
    const tz = store.get().tzOffsetHours;
    const minP = Math.min(...y.apsides.filter((a) => a.kind === 'perigee').map((a) => a.distanceKm));
    const maxA = Math.max(...y.apsides.filter((a) => a.kind === 'apogee').map((a) => a.distanceKm));
    replaceChildren(
      out,
      table(
        'Perigee and apogee',
        [`Date (${zoneLabel(tz)})`, 'Kind', 'Distance', 'Diameter'],
        y.apsides.map((a) =>
          h(
            'tr',
            { class: a.distanceKm === minP || a.distanceKm === maxA ? 'extreme' : '' },
            h('td', {}, timeLink(store, a.time, localDayTime(a.time, tz))),
            h('td', { class: 'num' }, a.kind),
            h('td', { class: 'num' }, `${Math.round(a.distanceKm).toLocaleString('en-US')} km`),
            h('td', { class: 'num' }, `${a.diameterArcmin.toFixed(2)}′`),
          ),
        ),
      ),
      table(
        'Node passages',
        [`Date (${zoneLabel(tz)})`, 'Node'],
        y.nodes.map((n) => h('tr', {}, h('td', {}, timeLink(store, n.time, localDayTime(n.time, tz))), h('td', { class: 'num' }, n.kind))),
      ),
      table(
        'Declination extremes',
        [`Date (${zoneLabel(tz)})`, 'Declination'],
        y.declinations.map((d) => h('tr', {}, h('td', {}, timeLink(store, d.time, localDayTime(d.time, tz))), h('td', { class: 'num' }, `${d.decDeg >= 0 ? '+' : '−'}${Math.abs(d.decDeg).toFixed(3)}°`))),
      ),
      h('p', { class: 'hint wide' }, `Geocentric. The year's closest perigee and farthest apogee are highlighted. ${y.ephemeris}. Click a time to set the app date and time.`),
    );
  };
  const el = h('div', { class: 'eph-sub' }, h('div', { class: 'toolbar eph-toolbar' }, year.el, status), out);
  return { el, show: l.show };
}

// --- seasons -----------------------------------------------------------------

function eotChart(y: SeasonsYear): SVGSVGElement {
  const W = 760;
  const H = 240;
  const m = { l: 44, r: 12, t: 12, b: 26 };
  const pts = y.equationOfTime;
  const n = pts.length;
  const lo = Math.floor(Math.min(...pts.map((p) => p.minutes)) / 5) * 5;
  const hi = Math.ceil(Math.max(...pts.map((p) => p.minutes)) / 5) * 5;
  const x = (i: number) => m.l + (i / Math.max(1, n - 1)) * (W - m.l - m.r);
  const yv = (v: number) => m.t + ((hi - v) / (hi - lo)) * (H - m.t - m.b);
  const NS = 'http://www.w3.org/2000/svg';
  const el = <K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number>, text?: string) => {
    const e = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
    if (text) e.textContent = text;
    return e;
  };
  const svg = el('svg', { viewBox: `0 0 ${W} ${H}`, class: 'eot-chart', role: 'img', 'aria-label': 'Equation of time through the year' });
  for (let v = lo; v <= hi; v += 5) {
    svg.append(el('line', { x1: m.l, x2: W - m.r, y1: yv(v), y2: yv(v), class: v === 0 ? 'axis' : 'grid' }));
    svg.append(el('text', { x: m.l - 6, y: yv(v) + 4, class: 'tick', 'text-anchor': 'end' }, `${v > 0 ? '+' : v < 0 ? '−' : ''}${Math.abs(v)}`));
  }
  pts.forEach((p, i) => {
    if (p.date.day === 1) {
      svg.append(el('line', { x1: x(i), x2: x(i), y1: m.t, y2: H - m.b, class: 'grid' }));
      svg.append(el('text', { x: x(i) + 3, y: H - 8, class: 'tick' }, MONTH_ABBR[p.date.month - 1]!));
    }
  });
  svg.append(el('path', { d: pts.map((p, i) => `${i ? 'L' : 'M'}${x(i).toFixed(1)},${yv(p.minutes).toFixed(1)}`).join(''), class: 'line' }));
  for (const s of y.seasons) {
    const i = pts.findIndex((p) => p.date.month === s.time.month && p.date.day === s.time.day);
    if (i >= 0) svg.append(el('circle', { cx: x(i), cy: yv(s.equationOfTimeMin), r: 4, class: 'mark' }, s.name));
  }
  svg.append(el('text', { x: m.l + 4, y: m.t + 12, class: 'tick' }, 'minutes (sundial ahead of clock when positive)'));
  return svg;
}

export function createSeasonsView({ backend, store }: EphemerisContext): SubView {
  const status = h('span', { class: 'status' });
  const run = runner(status);
  const out = h('div', {});
  const year = followingYear(store, () => l.invalidate());
  const compute = () => {
    const s = store.get();
    void run('Computing seasons', () => backend.tables.seasons({ year: year.year(), tzOffsetHours: s.tzOffsetHours, ephemerisId: s.ephemerisId }), render);
  };
  const l = lazy(compute);
  store.subscribe((s, prev) => {
    if (changed(s, prev, ['tzOffsetHours', 'ephemerisId'])) l.invalidate();
  });
  const mmss = (v: number) => {
    const sgn = v < 0 ? '−' : '+';
    const a = Math.abs(v);
    const mm = Math.floor(a);
    return `${sgn}${mm}m ${String(Math.round((a - mm) * 60)).padStart(2, '0')}s`;
  };
  const render = (y: SeasonsYear) => {
    const tz = store.get().tzOffsetHours;
    const max = y.equationOfTime.reduce((a, b) => (b.minutes > a.minutes ? b : a));
    const min = y.equationOfTime.reduce((a, b) => (b.minutes < a.minutes ? b : a));
    replaceChildren(
      out,
      h(
        'table',
        { class: 'criteria-table eph-table seasons-table' },
        h('thead', {}, h('tr', {}, h('th', {}, 'Event'), h('th', {}, `Date (${zoneLabel(tz)})`), h('th', { class: 'num' }, 'Equation of time'))),
        h('tbody', {}, ...y.seasons.map((s) => h('tr', {}, h('td', {}, s.name), h('td', {}, timeLink(store, s.time, localDayTime(s.time, tz))), h('td', { class: 'num' }, mmss(s.equationOfTimeMin))))),
      ),
      h('h3', {}, 'Equation of time'),
      eotChart(y),
      h(
        'p',
        { class: 'hint' },
        `Maximum ${mmss(max.minutes)} on ${max.date.day} ${MONTH_NAMES[max.date.month - 1]}, minimum ${mmss(min.minutes)} on ${min.date.day} ${MONTH_NAMES[min.date.month - 1]} (at local noon). `,
        `Apparent minus mean solar time (Meeus eq. 28.1). ${y.ephemeris}. Click an event to set the app date and time.`,
      ),
    );
  };
  const el = h('div', { class: 'eph-sub' }, h('div', { class: 'toolbar eph-toolbar' }, year.el, status), out);
  return { el, show: l.show };
}
