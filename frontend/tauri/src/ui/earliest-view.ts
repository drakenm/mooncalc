// Earliest-sighting panel: for the new Moon nearest the selected date, the
// first evening each zone of the criterion is reached anywhere, and where.

import { BackendError, type Backend, type CivilDate, type CriterionInfo, type EarliestResult } from '../backend';
import { localDate, type Store } from '../state';
import { h, replaceChildren } from './dom';
import { formatDate, formatLat, formatLon, formatUtDateTime, formatValue, valueSymbol } from './format';

const RESOLUTIONS = [
  { deg: 4, label: '4° (fast)' },
  { deg: 2, label: '2°' },
  { deg: 1, label: '1°' },
  { deg: 0.5, label: '0.5° (slow)' },
];

export interface EarliestContext {
  backend: Backend;
  store: Store;
  criteria: CriterionInfo[];
  /** Show the map for an evening, centred on a point. */
  showMap: (date: CivilDate, latDeg: number, lonDeg: number) => void;
}

export function createEarliestView({ backend, store, criteria, showMap }: EarliestContext): HTMLElement {
  const resolution = h('select', { 'aria-label': 'Scan resolution' }, ...RESOLUTIONS.map((r) => h('option', { value: r.deg }, r.label)));
  resolution.value = '2';
  const evenings = h('input', { type: 'number', min: 1, max: 10, step: 1, value: '4', 'aria-label': 'Evenings to scan' });
  const run = h('button', { type: 'button', class: 'button' }, 'Scan');
  const summary = h('p', { class: 'hint' }, 'Scans the evenings around the new Moon nearest the selected date, on a world grid.');
  const output = h('div', {});
  let seq = 0;

  const render = (r: EarliestResult) => {
    const sym = valueSymbol(r.criterionId);
    const chip = (rgb: [number, number, number], code: string) => {
      const el = h('span', { class: 'zone-chip' }, code);
      el.style.background = `rgb(${rgb.join(' ')})`;
      el.style.color = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2] > 140 ? '#111' : '#fff';
      return el;
    };
    replaceChildren(
      summary,
      `${r.criterionName} · conjunction ${formatUtDateTime(r.conjunction)} · evenings ${r.datesScanned.map(formatDate).join(', ')} · ` +
        `${r.resolutionDeg}° grid · ${r.timing.totalMs.toFixed(0)} ms (${r.timing.threads} threads)`,
    );
    replaceChildren(
      output,
      h(
        'table',
        { class: 'criteria-table' },
        h(
          'thead',
          {},
          h('tr', {}, h('th', {}, 'Zone'), h('th', {}, 'Description'), h('th', {}, 'First evening'), h('th', {}, 'Where (best cell)'), h('th', { class: 'num' }, 'Cells'), h('th', { class: 'num' }, sym), h('th', {}, '')),
        ),
        h(
          'tbody',
          {},
          ...r.zones.map((z) => {
            const f = z.first;
            return h(
              'tr',
              {},
              h('td', {}, chip(z.rgb, z.code)),
              h('td', {}, z.description),
              h('td', {}, f ? formatDate(f.date) : 'Not within the scanned evenings'),
              h('td', {}, f ? `${formatLat(f.latDeg, 1)} ${formatLon(f.lonDeg, 1)}${f.nearestCity ? ` · nearest city ${f.nearestCity}` : ''}` : '—'),
              h('td', { class: 'num' }, f ? String(f.cells) : ''),
              h('td', { class: 'num' }, f?.value != null ? formatValue(f.value) : ''),
              h('td', {}, f ? h('button', { type: 'button', class: 'button small', onclick: () => showMap(f.date, f.latDeg, f.lonDeg) }, 'Map') : null),
            );
          }),
        ),
      ),
    );
  };

  run.addEventListener('click', async () => {
    const s = store.get();
    const my = ++seq;
    run.disabled = true;
    replaceChildren(summary, 'Scanning…');
    try {
      const r = await backend.earliestSighting({
        date: localDate(s),
        criterionId: s.criterionId,
        resolutionDeg: Number(resolution.value),
        evenings: Math.min(10, Math.max(1, Math.round(evenings.valueAsNumber || 4))),
        ephemerisId: s.ephemerisId,
      });
      if (my === seq) render(r);
    } catch (err) {
      if (my === seq) replaceChildren(summary, h('span', { class: 'error' }, `Scan failed: ${BackendError.from(err).message}`));
    } finally {
      run.disabled = false;
    }
  });

  const criterionName = h('strong', {});
  const sync = () => {
    criterionName.textContent = criteria.find((c) => c.id === store.get().criterionId)?.name ?? '';
  };
  store.subscribe(sync);
  sync();

  return h(
    'div',
    { class: 'report-view earliest-view' },
    h('h2', {}, 'Earliest sighting'),
    h('p', { class: 'lead' }, 'Where and when each zone of ', criterionName, ' is first reached after the new Moon.'),
    h(
      'div',
      { class: 'toolbar' },
      h('label', { class: 'inline' }, 'Grid ', resolution),
      h('label', { class: 'inline' }, 'Evenings ', evenings),
      run,
    ),
    summary,
    output,
  );
}
