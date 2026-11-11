// Tabs whose features are not built yet: say what is coming.

import { h } from './dom';

export interface Placeholder {
  title: string;
  summary: string;
  items: string[];
}

export const PLACEHOLDERS: Record<'ephemeris' | 'calendar' | 'sky' | 'moon' | 'trajectory', Placeholder> = {
  ephemeris: {
    title: 'Ephemeris tables',
    summary: 'Sun and Moon positions and events for the selected place and date range.',
    items: [
      'Apparent RA/Dec, ecliptic longitude/latitude, altitude and azimuth',
      'Rise, set and transit times; twilight',
      'Moon phases, apogee and perigee, eclipses, libration',
      'Choice of the built-in Meeus/ELP theory or a JPL DE440 file',
      'Copy or export tables',
    ],
  },
  calendar: {
    title: 'Hijri calendars',
    summary: 'Islamic calendar conversion and month-start prediction.',
    items: [
      'Tabular (arithmetic) Hijri calendar variants',
      'Umm al-Qura calendar',
      'Visibility-based calendars, with month starts from the chosen criterion',
      'Gregorian ↔ Hijri month grids',
    ],
  },
  sky: {
    title: 'Sky view',
    summary: 'The western (or eastern) horizon at the crescent’s best time.',
    items: [
      'Sun, Moon and bright stars and planets above the local horizon',
      'Altitude/azimuth grid and twilight sky colouring',
      'Time slider from sunset to moonset',
    ],
  },
  moon: {
    title: 'Moon close-up',
    summary: 'The lunar disc as it appears from the selected location.',
    items: [
      'Phase and illuminated fraction',
      'Position angle of the bright limb and crescent orientation',
      'Libration and apparent diameter',
    ],
  },
  trajectory: {
    title: 'Local trajectory',
    summary: 'The paths of the Sun and Moon after sunset (or before sunrise).',
    items: [
      'Moon and Sun tracks against the horizon, minute by minute',
      'Crescent width, altitude and lag along the track',
      'Best-time marker for the chosen criterion',
    ],
  },
};

export function createPlaceholder(p: Placeholder): HTMLElement {
  return h(
    'div',
    { class: 'placeholder' },
    h('h2', {}, p.title),
    h('p', { class: 'lead' }, p.summary),
    h('p', { class: 'badge badge-muted' }, 'Coming in a later release'),
    h('ul', {}, ...p.items.map((i) => h('li', {}, i))),
  );
}
