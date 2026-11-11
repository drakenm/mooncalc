// Eclipses of a range of years: a list, and a details panel with the refined
// circumstances and the local circumstances at the app location.

import type { EclipseInfo, EclipseKindFilter, EclipseList, SolarContact } from '../backend';
import { changed, localDate } from '../state';
import { localDateTime, localDayTime, timeLink, utLabel, zoneLabel } from './app-time';
import { debounce, h, replaceChildren } from './dom';
import { runner, type EphemerisContext, type SubView } from './ephemeris-view';
import { formatLat, formatLon } from './format';

type Row = [string, string | Node];

function kv(rows: Row[]): HTMLTableElement {
  return h('table', { class: 'kv' }, h('tbody', {}, ...rows.map(([k, v]) => h('tr', {}, h('th', { scope: 'row' }, k), h('td', {}, v)))));
}

const pct = (v: number, digits = 1) => `${(v * 100).toFixed(digits)} %`;
const minutes = (v: number | null) => (v === null ? '—' : v >= 60 ? `${Math.floor(v / 60)}h ${String(Math.round(v % 60)).padStart(2, '0')}m` : `${Math.floor(v)}m ${String(Math.round((v % 1) * 60)).padStart(2, '0')}s`);

/** What the observer sees, in a few words. */
function localSummary(e: EclipseInfo, hasLocation: boolean): string {
  if (!hasLocation) return '';
  if (e.solar) {
    const l = e.solar.local;
    if (!l) return 'not seen';
    if (!l.visible) return `${l.kind}, Sun below horizon`;
    return l.kind === 'partial' ? `partial, ${pct(l.obscuration, 0)} covered` : `${l.kind}, ${minutes(l.centralDurationMin)}`;
  }
  if (e.lunar) {
    if (e.lunar.visible === null) return '';
    return e.lunar.visible ? 'visible' : 'Moon below horizon';
  }
  return '';
}

export function createEclipsesView({ backend, store }: EphemerisContext): SubView {
  const status = h('span', { class: 'status' });
  const run = runner(status);
  const list = h('div', { class: 'table-wrap eclipse-list' });
  const details = h('div', { class: 'eclipse-details' });
  let pinnedFrom: number | null = null;
  let selected = 0;
  let last: EclipseList | null = null;

  const from = h('input', { type: 'number', min: -4000, max: 9999, step: 1, 'aria-label': 'From year', class: 'year-input' });
  const years = h('select', { 'aria-label': 'Years' }, ...[1, 2, 5, 10, 20, 50].map((n) => h('option', { value: n, selected: n === 1 }, n === 1 ? '1 year' : `${n} years`)));
  const filter = h('select', { 'aria-label': 'Kind' }, h('option', { value: 'any' }, 'Solar and lunar'), h('option', { value: 'solar' }, 'Solar'), h('option', { value: 'lunar' }, 'Lunar'));
  const local = h('input', { type: 'checkbox', checked: true });
  const firstYear = () => pinnedFrom ?? localDate(store.get()).year;
  from.value = String(firstYear());

  const compute = () => {
    const s = store.get();
    const first = firstYear();
    from.value = String(first);
    void run('Computing eclipses', () =>
      backend.tables.eclipses({
        firstYear: first,
        lastYear: first + Number(years.value) - 1,
        filter: filter.value as EclipseKindFilter,
        location: local.checked ? s.location : null,
        ephemerisId: s.ephemerisId,
      }),
      (r) => {
        const keep = last?.eclipses[selected]?.greatest;
        last = r;
        // Keep the selection if the same eclipse is still listed; else pick
        // the first eclipse on or after the app date.
        const idx = keep ? r.eclipses.findIndex((e) => utLabel(e.greatest) === utLabel(keep)) : -1;
        const app = store.get().instant;
        const next = r.eclipses.findIndex((e) => utLabel(e.greatest) >= utLabel(app));
        selected = idx >= 0 ? idx : Math.max(0, next);
        render();
      },
    );
  };
  let stale = true;
  let shown = false;
  const invalidate = debounce(() => {
    stale = true;
    if (shown) {
      stale = false;
      compute();
    }
  }, 250);
  from.addEventListener('change', () => {
    pinnedFrom = Math.round(from.valueAsNumber);
    invalidate();
  });
  for (const el of [years, filter, local]) el.addEventListener('change', invalidate);
  store.subscribe((s, prev) => {
    if (changed(s, prev, ['ephemerisId']) || (local.checked && changed(s, prev, ['location']))) invalidate();
    else if (pinnedFrom === null && localDate(s).year !== localDate(prev).year) invalidate();
    else if (changed(s, prev, ['tzOffsetHours', 'locationName']) && last) render();
  });

  const render = () => {
    if (!last) return;
    const tz = store.get().tzOffsetHours;
    const hasLoc = last.location !== null;
    replaceChildren(
      list,
      last.eclipses.length === 0
        ? h('p', { class: 'muted' }, 'No eclipses in these years.')
        : h(
            'table',
            { class: 'criteria-table eph-table' },
            h('thead', {}, h('tr', {}, h('th', {}, `Greatest (${zoneLabel(tz)})`), h('th', {}, 'Type'), h('th', { class: 'num' }, 'γ'), h('th', { class: 'num' }, 'Mag.'), hasLoc ? h('th', {}, 'Here') : null)),
            h(
              'tbody',
              {},
              ...last.eclipses.map((e, i) =>
                h(
                  'tr',
                  {
                    class: `${i === selected ? 'selected' : ''} ${e.body}`,
                    onclick: () => {
                      selected = i;
                      render();
                    },
                  },
                  h('td', {}, localDateTime(e.greatest, tz)),
                  h('td', {}, h('span', { class: `eclipse-icon ${e.body}` }, e.body === 'solar' ? '☉' : '☾'), ` ${e.label}`),
                  h('td', { class: 'num' }, e.gamma.toFixed(4)),
                  h('td', { class: 'num' }, e.magnitude.toFixed(4)),
                  hasLoc ? h('td', {}, localSummary(e, hasLoc)) : null,
                ),
              ),
            ),
          ),
    );
    const e = last.eclipses[selected];
    replaceChildren(details, e ? renderDetails(e, last, tz) : '');
  };

  const contact = (label: string, c: SolarContact | null, tz: number) =>
    c
      ? h(
          'tr',
          { class: c.sunAltDeg < -0.833 ? 'below' : '' },
          h('td', {}, label),
          h('td', {}, timeLink(store, c.time, localDateTime(c.time, tz))),
          h('td', { class: 'num' }, `${c.sunAltDeg.toFixed(1)}°`),
          h('td', { class: 'num' }, `${c.sunAzDeg.toFixed(1)}°`),
        )
      : null;

  const renderDetails = (e: EclipseInfo, l: EclipseList, tz: number): HTMLElement => {
    const s = store.get();
    const place = l.location ? `${s.locationName ?? 'Custom location'} (${formatLat(l.location.latDeg, 2)} ${formatLon(l.location.lonDeg, 2)})` : null;
    const head = h('h3', {}, h('span', { class: `eclipse-icon ${e.body}` }, e.body === 'solar' ? '☉' : '☾'), ` ${e.label} eclipse of ${localDayTime(e.greatest, tz).replace(/ \d\d:\d\d$/, '')} ${e.greatest.year}`);
    const common: Row[] = [
      ['Greatest eclipse', timeLink(store, e.greatest, `${localDateTime(e.greatest, tz)} (${utLabel(e.greatest)})`)],
      ['γ', e.gamma.toFixed(4)],
      ['Lunation', String(e.lunation)],
    ];
    if (e.solar) {
      const sd = e.solar;
      const loc = sd.local;
      return h(
        'div',
        {},
        head,
        kv([
          ...common,
          ['Magnitude', e.magnitude.toFixed(4)],
          ['Central', sd.central ? 'yes' : 'no'],
          ['Point of greatest eclipse', `${formatLat(sd.greatestLatDeg, 2)} ${formatLon(sd.greatestLonDeg, 2)}`],
          ['Sun altitude there', `${sd.sunAltDeg.toFixed(1)}°`],
        ]),
        h('h3', {}, 'Local circumstances'),
        !place
          ? h('p', { class: 'muted' }, 'Turn on “local circumstances” to see the eclipse from the app location.')
          : !loc
            ? h('p', { class: 'muted' }, `No phase of this eclipse reaches ${place}.`)
            : h(
                'div',
                {},
                h('p', { class: 'hint' }, place, loc.visible ? '' : ' — the Sun is below the horizon throughout.'),
                h(
                  'table',
                  { class: 'criteria-table eph-table contacts' },
                  h('thead', {}, h('tr', {}, h('th', {}, 'Contact'), h('th', {}, `Time (${zoneLabel(tz)})`), h('th', { class: 'num' }, 'Sun alt'), h('th', { class: 'num' }, 'Sun az'))),
                  h(
                    'tbody',
                    {},
                    contact('C1 partial begins', loc.c1, tz),
                    contact(`C2 ${loc.kind} begins`, loc.c2, tz),
                    contact('Maximum', loc.max, tz),
                    contact(`C3 ${loc.kind} ends`, loc.c3, tz),
                    contact('C4 partial ends', loc.c4, tz),
                  ),
                ),
                kv([
                  ['Local type', loc.kind],
                  ['Magnitude', loc.magnitude.toFixed(4)],
                  ['Obscuration', pct(loc.obscuration, 2)],
                  ['Moon/Sun diameter ratio', loc.moonSunRatio.toFixed(4)],
                  ...(loc.centralDurationMin !== null ? [[`Duration of ${loc.kind}`, minutes(loc.centralDurationMin)] as Row] : []),
                  ['Duration (C1–C4)', minutes(loc.durationMin)],
                ]),
              ),
      );
    }
    const ld = e.lunar!;
    return h(
      'div',
      {},
      head,
      kv([
        ...common,
        ['Umbral magnitude', ld.umbralMagnitude.toFixed(4)],
        ['Penumbral magnitude', ld.penumbralMagnitude.toFixed(4)],
        ['Penumbral phase', minutes(ld.penumbralDurationMin)],
        ['Partial phase', minutes(ld.partialDurationMin)],
        ['Totality', minutes(ld.totalDurationMin)],
        ['Moon in the zenith at', `${formatLat(ld.zenithLatDeg, 2)} ${formatLon(ld.zenithLonDeg, 2)}`],
      ]),
      h('h3', {}, 'Contacts', place ? h('span', { class: 'hint' }, ` — the Moon seen from ${place}`) : ''),
      h(
        'table',
        { class: 'criteria-table eph-table contacts' },
        h('thead', {}, h('tr', {}, h('th', {}, 'Contact'), h('th', {}, `Time (${zoneLabel(tz)})`), place ? h('th', { class: 'num' }, 'Moon alt') : null, place ? h('th', {}, '') : null)),
        h(
          'tbody',
          {},
          ...ld.contacts.map((c) =>
            h(
              'tr',
              { class: c.aboveHorizon === false ? 'below' : '' },
              h('td', {}, `${c.code === 'max' ? '' : `${c.code} `}${c.label}`),
              h('td', {}, timeLink(store, c.time, localDateTime(c.time, tz))),
              place ? h('td', { class: 'num' }, c.moonAltDeg !== null ? `${c.moonAltDeg.toFixed(1)}°` : '—') : null,
              place ? h('td', {}, c.aboveHorizon ? 'above horizon' : 'below horizon') : null,
            ),
          ),
        ),
      ),
    );
  };

  const el = h(
    'div',
    { class: 'eph-sub' },
    h(
      'div',
      { class: 'toolbar eph-toolbar' },
      h('label', { class: 'inline' }, 'From ', from),
      years,
      filter,
      h('label', { class: 'inline' }, local, ' Local circumstances (app location)'),
      status,
    ),
    h('div', { class: 'eclipse-layout' }, list, details),
    h('p', { class: 'hint' }, 'Meeus ch. 54 refined against the ephemeris: Besselian elements for solar eclipses, the Earth’s shadow (Danjon) for lunar ones. Rows in grey: the body is below the horizon. Click a time to set the app date and time.'),
  );
  return {
    el,
    show: () => {
      shown = true;
      if (stale) {
        stale = false;
        compute();
      }
    },
  };
}
