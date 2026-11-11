// Report tab: rise/set and twilight for the local date, then the evening and
// morning crescents with their full geometry and every criterion's verdict.

import {
  BackendError,
  type Backend,
  type CrescentReport,
  type CriterionInfo,
  type CriterionVerdict,
  type LocationReport,
  type UtDateTime,
} from '../backend';
import { changed, localDate, type Store } from '../state';
import { debounce, h, replaceChildren } from './dom';
import { formatDate, formatLat, formatLon, formatNumber, formatOffset, formatUtDateTime, formatValue } from './format';

type Row = [string, string | Node];

function kv(rows: Row[]): HTMLTableElement {
  return h('table', { class: 'kv' }, h('tbody', {}, ...rows.map(([k, v]) => h('tr', {}, h('th', { scope: 'row' }, k), h('td', {}, v)))));
}

function chip(rgb: [number, number, number], code: string): HTMLElement {
  const el = h('span', { class: 'zone-chip' }, code);
  el.style.background = `rgb(${rgb.join(' ')})`;
  // Dark text on light zone colours, light text on dark ones.
  const lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
  el.style.color = lum > 140 ? '#111' : '#fff';
  return el;
}

const STATUS_LABEL: Record<string, string> = {
  crescent: 'Crescent',
  'before-conjunction': 'Before conjunction',
  'moon-sets-first': 'Moon sets first',
  'moon-always-up': 'Moon always up',
  'moon-always-down': 'Moon always down',
  'midnight-sun': 'Midnight Sun',
  'polar-night': 'Polar night',
  'sun-altitude-not-reached': 'Sun altitude not reached',
  'outside-ephemeris': 'Outside the ephemeris coverage',
};

export class ReportView {
  readonly el = h('div', { class: 'report-view' });
  private active = false;
  private stale = true;
  private seq = 0;

  constructor(
    private readonly backend: Backend,
    private readonly store: Store,
    private readonly criteria: CriterionInfo[],
  ) {
    const refresh = debounce(() => void this.refresh(), 150);
    this.store.subscribe((s, prev) => {
      if (changed(s, prev, ['instant', 'tzOffsetHours', 'location', 'ephemerisId'])) {
        this.stale = true;
        if (this.active) refresh();
      } else if (changed(s, prev, ['criterionId', 'crescent', 'locationName']) && this.last) {
        this.render(this.last);
      }
    });
  }

  private last: LocationReport | null = null;

  setActive(active: boolean): void {
    this.active = active;
    if (active && this.stale) void this.refresh();
  }

  private async refresh(): Promise<void> {
    const s = this.store.get();
    const seq = ++this.seq;
    this.stale = false;
    try {
      const report = await this.backend.locationReport({
        date: localDate(s),
        tzOffsetHours: s.tzOffsetHours,
        location: s.location,
        ephemerisId: s.ephemerisId,
      });
      if (seq !== this.seq) return;
      this.last = report;
      this.render(report);
    } catch (err) {
      if (seq !== this.seq) return;
      this.stale = true;
      replaceChildren(this.el, h('p', { class: 'error' }, `Report failed: ${BackendError.from(err).message}`));
    }
  }

  private render(r: LocationReport): void {
    const s = this.store.get();
    const tz = r.tzOffsetHours;
    const t = (x: UtDateTime | null) => formatUtDateTime(x, tz);
    const d = r.day;
    const first = s.crescent === 'evening' ? r.evening : r.morning;
    const second = s.crescent === 'evening' ? r.morning : r.evening;

    replaceChildren(
      this.el,
      h('h2', {}, 'Location report'),
      h(
        'p',
        { class: 'lead' },
        `${s.locationName ?? 'Custom location'} · ${formatLat(r.location.latDeg, 4)} ${formatLon(r.location.lonDeg, 4)} · ` +
          `${r.location.elevationM.toFixed(0)} m · local date ${formatDate(r.date)} (${formatOffset(tz)})`,
      ),
      r.mapDateNote ? h('p', { class: 'note map-date-note', role: 'note' }, r.mapDateNote.text) : null,
      h(
        'div',
        { class: 'report-grid' },
        h(
          'section',
          {},
          h('h3', {}, 'Sun and Moon'),
          kv([
            ['Sunrise', t(d.sunrise)],
            ['Solar noon', t(d.solarNoon)],
            ['Sunset', t(d.sunset)],
            ['Civil twilight', `${t(d.civilDawn)} – ${t(d.civilDusk)}`],
            ['Nautical twilight', `${t(d.nauticalDawn)} – ${t(d.nauticalDusk)}`],
            ['Moonrise', t(d.moonrise)],
            ['Moon transit', t(d.moonTransit)],
            ['Moonset', t(d.moonset)],
            ['Moon age at noon', formatNumber(r.moonAgeAtNoonHours, 1, ' h')],
            ['Illumination at noon', formatNumber(r.illuminationAtNoon * 100, 1, ' %')],
          ]),
        ),
      ),
      this.crescentSection(first, tz, s.criterionId),
      this.crescentSection(second, tz, s.criterionId),
      h('p', { class: 'hint' }, `Ephemeris: ${r.ephemeris}. Times UT, local in brackets. Rise and set: upper limb, 34′ refraction, level horizon.`),
    );
  }

  private crescentSection(c: CrescentReport, tz: number, selected: string): HTMLElement {
    const t = (x: UtDateTime | null) => formatUtDateTime(x, tz);
    const evening = c.crescent === 'evening';
    const g = c.geometry;
    const verdict = c.criteria.find((v) => v.criterionId === selected);
    return h(
      'section',
      { class: 'crescent-section' },
      h(
        'h3',
        {},
        evening ? 'Evening (young crescent)' : 'Morning (old crescent)',
        verdict?.verdict ? chip(verdict.verdict.rgb, verdict.verdict.code) : null,
      ),
      h('p', { class: `status-line status-${c.status}` }, c.statusText),
      h(
        'div',
        { class: 'report-grid' },
        kv([
          [evening ? 'Sunset' : 'Sunrise', t(c.sunEvent)],
          [evening ? 'Moonset' : 'Moonrise', t(c.moonEvent)],
          ['Lag', formatNumber(g?.lagMinutes, 0, ' min')],
          ['Best time', t(c.bestTime)],
          ['Conjunction', t(c.conjunction)],
          [evening ? 'Moon age' : 'Time to conjunction', formatNumber(g?.ageHours, 2, ' h')],
          ['Illumination', formatNumber(g ? g.illuminatedFraction * 100 : null, 2, ' %')],
          ['Moon distance', formatNumber(g?.moonDistanceKm, 0, ' km')],
        ]),
        kv([
          ['ARCL topocentric / geocentric', g ? `${g.arclDeg.toFixed(2)}° / ${g.arclGeoDeg.toFixed(2)}°` : '—'],
          ['ARCV geocentric / topocentric', g ? `${g.arcvDeg.toFixed(2)}° / ${g.arcvTopoDeg.toFixed(2)}°` : '—'],
          ['DAZ', formatNumber(g?.dazDeg, 2, '°')],
          ['Width W′ topo / W geo', g ? `${g.widthTopoArcmin.toFixed(3)}′ / ${g.widthGeoArcmin.toFixed(3)}′` : '—'],
          ['Semi-diameter (topo)', formatNumber(g?.semiDiameterTopoArcmin, 2, '′')],
          ['Moon altitude (apparent / airless)', g ? `${g.moonAltDeg.toFixed(2)}° / ${g.moonAltAirlessDeg.toFixed(2)}°` : '—'],
          ['Sun altitude', formatNumber(g?.sunAltDeg, 2, '°')],
          ['Moon / Sun azimuth', g ? `${g.moonAzDeg.toFixed(2)}° / ${g.sunAzDeg.toFixed(2)}°` : '—'],
        ]),
      ),
      this.criteriaTable(c.criteria, tz, selected),
    );
  }

  private criteriaTable(rows: CriterionVerdict[], tz: number, selected: string): HTMLElement {
    const ref = (id: string) => this.criteria.find((c) => c.id === id)?.reference ?? '';
    return h(
      'table',
      { class: 'criteria-table' },
      h(
        'thead',
        {},
        h('tr', {}, h('th', {}, 'Criterion'), h('th', {}, 'Zone'), h('th', {}, 'Result'), h('th', { class: 'num' }, 'Value'), h('th', {}, 'Evaluated at')),
      ),
      h(
        'tbody',
        {},
        ...rows.map((v) =>
          h(
            'tr',
            {
              class: v.criterionId === selected ? 'selected' : '',
              title: ref(v.criterionId),
              onclick: () => this.store.update({ criterionId: v.criterionId }),
            },
            h('td', {}, v.name),
            h('td', {}, v.verdict ? chip(v.verdict.rgb, v.verdict.code) : h('span', { class: 'hint' }, '—')),
            h('td', {}, v.verdict ? v.verdict.description : STATUS_LABEL[v.status] ?? v.status),
            h('td', { class: 'num' }, v.verdict?.value != null ? formatValue(v.verdict.value) : ''),
            h('td', {}, v.evaluatedAt ? `${formatUtDateTime(v.evaluatedAt, tz)}` : '—', h('div', { class: 'hint' }, v.evaluationTime)),
          ),
        ),
      ),
    );
  }
}
