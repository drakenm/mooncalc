// Sidebar: date/time, location (with city search), criterion, crescent and
// ephemeris. Writes to the store; re-renders its inputs when the store changes.

import { BackendError, type AppInfo, type Backend, type City, type CriterionInfo } from '../backend';
import type { AppState, Store } from '../state';
import { ensureZone, offsetAt, tzOf, utMs, zoneAbbrev } from '../zone';
import { debounce, h, replaceChildren } from './dom';
import { formatDate, formatTime, shiftHours } from './format';

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
  const tzInput = h('input', { type: 'number', min: -14, max: 14, step: 0.25, required: true, 'aria-label': 'Fixed offset from UT, hours' });
  const localTime = h('p', { class: 'hint', 'aria-live': 'polite' });
  // Time zone: an IANA zone (DST and historical rules, from the backend's
  // IANA Time Zone Database) or a fixed offset.
  const tzMode = h(
    'select',
    { 'aria-label': 'Time-zone kind' },
    h('option', { value: 'zone' }, 'IANA zone'),
    h('option', { value: 'fixed' }, 'Fixed offset'),
  );
  const zoneNames = h('datalist', { id: 'tz-names' });
  const zoneInput = h('input', {
    type: 'text',
    list: 'tz-names',
    placeholder: 'e.g. Europe/London',
    spellcheck: 'false',
    autocomplete: 'off',
    'aria-label': 'IANA time zone',
  });
  const zoneError = h('p', { class: 'hint error', 'aria-live': 'polite' });
  let namesLoaded = false;
  zoneInput.addEventListener('focus', async () => {
    if (namesLoaded) return;
    namesLoaded = true;
    try {
      const names = await backend.timeZoneNames();
      replaceChildren(zoneNames, ...names.map((n) => h('option', { value: n })));
    } catch {
      namesLoaded = false;
    }
  });
  const quarter = (hours: number) => Math.round(hours * 4) / 4;
  /** Load a zone's rules, then make it the app's zone (with `patch`). */
  const selectZone = async (name: string, patch: Partial<AppState> = {}): Promise<boolean> => {
    try {
      const z = await ensureZone(name.trim(), (n) => backend.timeZoneOffsets({ timeZone: n }));
      const offset = z.offsetAtMs(utMs(store.get().instant));
      store.update({ ...patch, timeZone: z.name, tzOffsetHours: quarter(offset) });
      zoneError.textContent = '';
      return true;
    } catch (err) {
      zoneError.textContent = `Time zone "${name}": ${BackendError.from(err).message}`;
      return false;
    }
  };
  zoneInput.addEventListener('change', () => {
    if (zoneInput.value.trim()) void selectZone(zoneInput.value);
  });
  tzMode.addEventListener('change', () => {
    const s = store.get();
    if (tzMode.value === 'fixed') {
      store.update({ timeZone: null, tzOffsetHours: quarter(offsetAt(tzOf(s), s.instant)) });
    } else {
      void selectZone(zoneInput.value.trim() || 'Etc/UTC');
    }
  });

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

  const chooseCity = async (c: City) => {
    // The city's IANA zone gives the offset on every date (DST, history);
    // without one, its standard offset.
    const place = { location: { ...c.location }, locationName: `${c.name}, ${c.country}` };
    searchInput.value = '';
    results.hidden = true;
    if (!(c.timeZone && (await selectZone(c.timeZone, place)))) {
      store.update({ ...place, timeZone: null, tzOffsetHours: c.stdOffsetHours });
    }
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
                  { type: 'button', role: 'option', onclick: () => void chooseCity(c) },
                  h('span', { class: 'city-name' }, c.name),
                  h(
                    'span',
                    { class: 'city-meta' },
                    [c.admin1, c.country].filter(Boolean).join(', ') +
                      (c.population ? ` · ${(c.population / 1e6 >= 1 ? `${(c.population / 1e6).toFixed(1)} M` : `${Math.round(c.population / 1e3)} k`)}` : ''),
                  ),
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

  let backends = info.ephemerisBackends;
  const ephemerisSelect = h(
    'select',
    { onchange: () => store.update({ ephemerisId: ephemerisSelect.value }) },
    ...backends.map((b) =>
      h('option', { value: b.id, disabled: !b.available, title: b.description }, b.available ? b.name : `${b.name} (not in this build)`),
    ),
  );
  const ephemerisHint = h('p', { class: 'hint' });
  const de440Input = h('input', { type: 'text', placeholder: 'C:\path\to\de440s.bsp', spellcheck: 'false', 'aria-label': 'DE440 file path' });
  const de440Status = h('p', { class: 'hint', 'aria-live': 'polite' });
  const de440Load = h(
    'button',
    {
      type: 'button',
      class: 'button small',
      onclick: async () => {
        const path = de440Input.value.trim();
        if (!path) return;
        de440Load.disabled = true;
        de440Status.textContent = 'Loading…';
        try {
          const b = await backend.loadDe440({ path });
          backends = backends.map((x) => (x.id === b.id ? b : x));
          store.update({ de440Path: path, ephemerisId: 'de440' });
          de440Status.textContent = b.detail ?? 'Loaded';
        } catch (err) {
          de440Status.textContent = `Could not load: ${BackendError.from(err).message}`;
        } finally {
          de440Load.disabled = false;
        }
      },
    },
    'Load',
  );
  const de440Row = h('div', { class: 'de440' }, h('div', { class: 'inline-row' }, de440Input, de440Load), de440Status);
  const de440Available = info.ephemerisBackends.some((b) => b.id === 'de440' && b.available);
  de440Row.hidden = !de440Available;

  // --- Sync inputs from state ---------------------------------------------
  const render = (s: AppState) => {
    const t = s.instant;
    // Don't fight the user while they're typing in a field.
    const active = document.activeElement;
    if (active !== dateInput) dateInput.value = formatDate(t);
    if (active !== timeInput) timeInput.value = formatTime(t);
    if (active !== tzInput) tzInput.value = String(s.tzOffsetHours);
    tzMode.value = s.timeZone ? 'zone' : 'fixed';
    zoneInput.hidden = !s.timeZone;
    tzInput.hidden = !!s.timeZone;
    if (active !== zoneInput && s.timeZone) zoneInput.value = s.timeZone;
    const tz = tzOf(s);
    const local = shiftHours(t, offsetAt(tz, t));
    localTime.textContent = `Local: ${formatDate(local)} ${formatTime(local)} (${zoneAbbrev(tz, utMs(t))})`;

    if (active !== latInput) latInput.value = String(+s.location.latDeg.toFixed(4));
    if (active !== lonInput) lonInput.value = String(+s.location.lonDeg.toFixed(4));
    if (active !== elevInput) elevInput.value = String(Math.round(s.location.elevationM));
    placeName.textContent = s.locationName ?? 'Custom location';

    criterionSelect.value = s.criterionId;
    criterionRef.textContent = criteria.find((c) => c.id === s.criterionId)?.reference ?? '';
    for (const b of crescentButtons) b.setAttribute('aria-pressed', String(b.dataset['value'] === s.crescent));
    ephemerisSelect.value = s.ephemerisId;
    const eb = backends.find((b) => b.id === s.ephemerisId);
    ephemerisHint.textContent = eb ? (eb.ready ? eb.description : `${eb.description} Not loaded yet.`) : '';
    if (active !== de440Input && !de440Input.value) de440Input.value = s.de440Path;
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
      field('Time zone', h('div', { class: 'inline-row tz-row' }, tzMode, zoneInput, tzInput), localTime),
      zoneNames,
      zoneError,
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
    h('section', { class: 'panel' }, h('h2', {}, 'Ephemeris'), ephemerisSelect, ephemerisHint, de440Row),
    h(
      'footer',
      { class: 'sidebar-footer' },
      h('span', { class: `badge badge-${backend.kind}` }, backend.kind === 'tauri' ? 'Desktop' : 'Browser preview'),
      h('span', { class: 'hint' }, info.runtime),
    ),
  );
}
