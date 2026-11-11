// Sidebar: date/time, location (with city search), criterion, crescent and
// ephemeris. Writes to the store; re-renders its inputs when the store changes.

import type { AppInfo, Backend, City, CriterionInfo } from '../backend';
import type { AppState, Store } from '../state';
import { debounce, h, replaceChildren } from './dom';
import { formatDate, formatOffset, formatTime, shiftHours, utToDate, zoneOffsetHours } from './format';

export interface SidebarContext {
  backend: Backend;
  store: Store;
  info: AppInfo;
  criteria: CriterionInfo[];
}

function parseNumber(input: HTMLInputElement): number | null {
  const v = input.valueAsNumber;
  if (!Number.isFinite(v) || !input.checkValidity()) return null;
  return v;
}

function field(label: string, control: HTMLElement, hint?: HTMLElement): HTMLElement {
  return h('label', { class: 'field' }, h('span', { class: 'field-label' }, label), control, hint ?? null);
}

export function createSidebar({ backend, store, info, criteria }: SidebarContext): HTMLElement {
  // --- Date and time (UT) ------------------------------------------------
  const dateInput = h('input', { type: 'date', required: true });
  // Plain text so UT always shows as 24-hour HH:MM, whatever the OS locale.
  const timeInput = h('input', {
    type: 'text',
    inputmode: 'numeric',
    pattern: '([01]?[0-9]|2[0-3]):[0-5][0-9]',
    placeholder: 'HH:MM',
    maxlength: 5,
    required: true,
  });
  const tzInput = h('input', { type: 'number', min: -14, max: 14, step: 0.25, required: true });
  const localTime = h('p', { class: 'hint', 'aria-live': 'polite' });

  const commitInstant = () => {
    if (!dateInput.checkValidity() || !timeInput.checkValidity()) return;
    const [y, mo, d] = dateInput.value.split('-').map(Number);
    const [hh, mm] = timeInput.value.split(':').map(Number);
    if ([y, mo, d, hh, mm].some((n) => n === undefined || !Number.isFinite(n))) return;
    store.update({
      instant: { year: y!, month: mo!, day: d!, hour: hh!, minute: mm!, second: 0 },
    });
  };
  dateInput.addEventListener('change', commitInstant);
  timeInput.addEventListener('change', commitInstant);
  tzInput.addEventListener('change', () => {
    const v = parseNumber(tzInput);
    if (v !== null) store.update({ tzOffsetHours: v });
  });

  const nowButton = h(
    'button',
    {
      type: 'button',
      class: 'button small',
      onclick: () => {
        const d = new Date();
        store.update({
          instant: {
            year: d.getUTCFullYear(),
            month: d.getUTCMonth() + 1,
            day: d.getUTCDate(),
            hour: d.getUTCHours(),
            minute: d.getUTCMinutes(),
            second: 0,
          },
        });
      },
    },
    'Now',
  );

  // --- Location ------------------------------------------------------------
  const latInput = h('input', { type: 'number', min: -90, max: 90, step: 'any', required: true });
  const lonInput = h('input', { type: 'number', min: -180, max: 180, step: 'any', required: true });
  const elevInput = h('input', { type: 'number', min: -500, max: 9000, step: 1, required: true });
  const placeName = h('p', { class: 'hint place-name' });

  const commitLocation = () => {
    const lat = parseNumber(latInput);
    const lon = parseNumber(lonInput);
    const elev = parseNumber(elevInput);
    if (lat === null || lon === null || elev === null) return;
    store.update({ location: { latDeg: lat, lonDeg: lon, elevationM: elev }, locationName: null });
  };
  for (const i of [latInput, lonInput, elevInput]) i.addEventListener('change', commitLocation);

  const searchInput = h('input', {
    type: 'search',
    placeholder: 'Search cities…',
    autocomplete: 'off',
    'aria-label': 'Search cities',
    'aria-controls': 'city-results',
  });
  const results = h('ul', { id: 'city-results', class: 'city-results', role: 'listbox', hidden: true });
  let searchSeq = 0;

  const chooseCity = (c: City) => {
    const offset = c.timeZone ? zoneOffsetHours(c.timeZone, utToDate(store.get().instant)) : null;
    store.update({
      location: { ...c.location },
      locationName: `${c.name}, ${c.country}`,
      ...(offset !== null ? { tzOffsetHours: offset } : {}),
    });
    searchInput.value = '';
    results.hidden = true;
  };

  const runSearch = debounce(async (query: string) => {
    const seq = ++searchSeq;
    if (!query.trim()) {
      results.hidden = true;
      return;
    }
    try {
      const cities = await backend.searchCities({ query, limit: 12 });
      if (seq !== searchSeq) return;
      replaceChildren(
        results,
        ...(cities.length
          ? cities.map((c) =>
              h(
                'li',
                {},
                h(
                  'button',
                  { type: 'button', role: 'option', onclick: () => chooseCity(c) },
                  h('span', { class: 'city-name' }, c.name),
                  h('span', { class: 'city-meta' }, [c.admin1, c.country].filter(Boolean).join(', ')),
                ),
              ),
            )
          : [h('li', { class: 'empty' }, 'No matches')]),
      );
      results.hidden = false;
    } catch (err) {
      if (seq !== searchSeq) return;
      replaceChildren(results, h('li', { class: 'empty' }, `Search failed: ${String(err)}`));
      results.hidden = false;
    }
  }, 150);
  searchInput.addEventListener('input', () => runSearch(searchInput.value));
  searchInput.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') results.hidden = true;
    if (e.key === 'Enter') results.querySelector<HTMLButtonElement>('button')?.click();
  });

  // --- Criterion, crescent, ephemeris -------------------------------------
  const criterionSelect = h(
    'select',
    { onchange: () => store.update({ criterionId: criterionSelect.value }) },
    ...criteria.map((c) => h('option', { value: c.id }, c.name)),
  );
  const criterionRef = h('p', { class: 'hint' });

  const crescentButtons = (['evening', 'morning'] as const).map((value) =>
    h(
      'button',
      {
        type: 'button',
        class: 'segment',
        'data-value': value,
        onclick: () => store.update({ crescent: value }),
      },
      value === 'evening' ? 'Evening' : 'Morning',
    ),
  );

  const ephemerisSelect = h(
    'select',
    { onchange: () => store.update({ ephemerisId: ephemerisSelect.value }) },
    ...info.ephemerisBackends.map((b) =>
      h('option', { value: b.id, disabled: !b.available, title: b.description }, b.available ? b.name : `${b.name} (unavailable)`),
    ),
  );

  // --- Sync inputs from state ---------------------------------------------
  const render = (s: AppState) => {
    const t = s.instant;
    // Don't fight the user while they're typing in a field.
    const active = document.activeElement;
    if (active !== dateInput) dateInput.value = formatDate(t);
    if (active !== timeInput) timeInput.value = formatTime(t);
    if (active !== tzInput) tzInput.value = String(s.tzOffsetHours);
    const local = shiftHours(t, s.tzOffsetHours);
    localTime.textContent = `Local: ${formatDate(local)} ${formatTime(local)} (${formatOffset(s.tzOffsetHours)})`;

    if (active !== latInput) latInput.value = String(+s.location.latDeg.toFixed(4));
    if (active !== lonInput) lonInput.value = String(+s.location.lonDeg.toFixed(4));
    if (active !== elevInput) elevInput.value = String(Math.round(s.location.elevationM));
    placeName.textContent = s.locationName ?? 'Custom location';

    criterionSelect.value = s.criterionId;
    criterionRef.textContent = criteria.find((c) => c.id === s.criterionId)?.reference ?? '';
    for (const b of crescentButtons) b.setAttribute('aria-pressed', String(b.dataset['value'] === s.crescent));
    ephemerisSelect.value = s.ephemerisId;
  };
  store.subscribe(render);
  render(store.get());

  return h(
    'aside',
    { class: 'sidebar', 'aria-label': 'Observation settings' },
    h(
      'header',
      { class: 'brand' },
      h('img', { src: '/icon.svg', alt: '', width: 28, height: 28 }),
      h('div', {}, h('h1', {}, 'MoonCalc'), h('span', { class: 'version' }, `v${info.version}`)),
    ),
    h(
      'section',
      { class: 'panel' },
      h('h2', {}, 'Date & time (UT)', nowButton),
      h('div', { class: 'row' }, field('Date', dateInput), field('Time (24 h)', timeInput)),
      field('Time-zone offset (h)', tzInput, localTime),
    ),
    h(
      'section',
      { class: 'panel' },
      h('h2', {}, 'Location'),
      h('div', { class: 'search' }, searchInput, results),
      placeName,
      h('div', { class: 'row' }, field('Latitude °', latInput), field('Longitude °E', lonInput)),
      field('Elevation (m)', elevInput),
    ),
    h(
      'section',
      { class: 'panel' },
      h('h2', {}, 'Criterion'),
      criterionSelect,
      criterionRef,
      h('div', { class: 'segmented', role: 'group', 'aria-label': 'Crescent' }, ...crescentButtons),
    ),
    h('section', { class: 'panel' }, h('h2', {}, 'Ephemeris'), ephemerisSelect),
    h(
      'footer',
      { class: 'sidebar-footer' },
      h('span', { class: `badge badge-${backend.kind}` }, backend.kind === 'tauri' ? 'Desktop' : 'Browser preview'),
      h('span', { class: 'hint' }, info.runtime),
    ),
  );
}
