// Report tab: crescent circumstances at the selected location.
//
// Already wired to `location_report`; the backend returns placeholders (all
// computed fields empty) until the circumstances engine lands.

import { BackendError, type Backend, type CriterionInfo, type LocationReport } from '../backend';
import { changed, type Store } from '../state';
import { debounce, h, replaceChildren } from './dom';
import { formatLat, formatLon, formatNumber, formatUtDateTime } from './format';

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
      if (changed(s, prev, ['instant', 'tzOffsetHours', 'location', 'criterionId', 'crescent', 'ephemerisId'])) {
        this.stale = true;
        if (this.active) refresh();
      }
    });
  }

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
        instant: s.instant,
        tzOffsetHours: s.tzOffsetHours,
        location: s.location,
        criterionId: s.criterionId,
        crescent: s.crescent,
        ephemerisId: s.ephemerisId,
      });
      if (seq === this.seq) this.render(report);
    } catch (err) {
      if (seq !== this.seq) return;
      this.stale = true;
      replaceChildren(this.el, h('p', { class: 'error' }, `Report failed: ${BackendError.from(err).message}`));
    }
  }

  private render(r: LocationReport): void {
    const s = this.store.get();
    const tz = s.tzOffsetHours;
    const evening = r.crescent === 'evening';
    const g = r.geometry;
    const criterion = this.criteria.find((c) => c.id === r.criterionId);
    const rows: [string, string][] = [
      ['Location', `${s.locationName ?? 'Custom'} · ${formatLat(r.location.latDeg, 4)} ${formatLon(r.location.lonDeg, 4)} · ${r.location.elevationM.toFixed(0)} m`],
      ['Instant', formatUtDateTime(s.instant, tz)],
      ['Criterion', `${criterion?.name ?? r.criterionId} · ${evening ? 'evening' : 'morning'} crescent`],
      [evening ? 'Sunset' : 'Sunrise', formatUtDateTime(r.sunEvent, tz)],
      [evening ? 'Moonset' : 'Moonrise', formatUtDateTime(r.moonEvent, tz)],
      ['Best time', formatUtDateTime(r.bestTime, tz)],
      ['Conjunction', formatUtDateTime(r.conjunction, tz)],
      ['Moon age', formatNumber(g?.ageHours, 1, ' h')],
      ['Lag', formatNumber(g?.lagMinutes, 0, ' min')],
      ['ARCL (elongation)', formatNumber(g?.arclDeg, 2, '°')],
      ['ARCV', formatNumber(g?.arcvDeg, 2, '°')],
      ['DAZ', formatNumber(g?.dazDeg, 2, '°')],
      ["Crescent width W′", formatNumber(g?.widthTopoArcmin, 2, '′')],
      ['Moon altitude', formatNumber(g?.moonAltDeg, 2, '°')],
      ['Sun altitude', formatNumber(g?.sunAltDeg, 2, '°')],
      ['Illumination', formatNumber(g ? g.illuminatedFraction * 100 : null, 1, ' %')],
      [
        'Verdict',
        r.verdict
          ? `${r.verdict.code}: ${r.verdict.description}${r.verdict.value !== null ? ` (${r.verdict.value.toFixed(3)})` : ''}`
          : '—',
      ],
    ];
    replaceChildren(
      this.el,
      h('h2', {}, 'Location report'),
      r.synthetic ? h('p', { class: 'banner' }, 'Preview: the report layout is wired to the backend, but the values are placeholders.') : null,
      h('table', { class: 'kv' }, h('tbody', {}, ...rows.map(([k, v]) => h('tr', {}, h('th', { scope: 'row' }, k), h('td', {}, v))))),
      r.notes.length ? h('ul', { class: 'notes' }, ...r.notes.map((n) => h('li', {}, n))) : null,
    );
  }
}
