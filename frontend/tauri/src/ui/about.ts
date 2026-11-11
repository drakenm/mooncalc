// About tab: version, licence, credits and data attributions.

import type { AppInfo } from '../backend';
import { h } from './dom';
// The repository licence, bundled at build time (single source of truth).
import MIT_LICENSE from '../../../../LICENSE?raw';

const ATTRIBUTIONS: [string, string][] = [
  ['Natural Earth', 'Coastline and country base-map data. Public domain (naturalearthdata.com).'],
  ['GeoNames', 'City atlas data, licensed under Creative Commons Attribution 4.0 (CC BY 4.0) (geonames.org).'],
  ['JPL DE440', 'Planetary and lunar ephemeris, NASA Jet Propulsion Laboratory (Park et al. 2021), optional.'],
  ['Jean Meeus', 'Astronomical Algorithms, 2nd edition (Willmann-Bell, 1998): the built-in Sun and Moon theory and time scales.'],
  ['B. D. Yallop (1997)', 'A Method for Predicting the First Sighting of the New Crescent Moon, NAO Technical Note No. 69.'],
  ['M. Sh. Odeh (2004)', 'New Criterion for Lunar Crescent Visibility, Experimental Astronomy 18, 39–64.'],
  ['Tauri', 'Desktop application framework (MIT / Apache-2.0).'],
];

export function createAbout(info: AppInfo): HTMLElement {
  return h(
    'div',
    { class: 'about' },
    h('h2', {}, `${info.name} ${info.version}`),
    h(
      'p',
      { class: 'lead' },
      'Crescent Moon visibility maps, ephemerides and Hijri calendars.',
    ),
    h(
      'p',
      {},
      'Inspired by Dr. Monzur Ahmed’s MoonCalc 6.0. This program is an independent reimplementation from ' +
        'published algorithms; it contains no code from the original and is not affiliated with its author.',
    ),
    h('p', { class: 'hint' }, `Runtime: ${info.runtime}`),
    h('h3', {}, 'Credits and data sources'),
    h('dl', { class: 'attributions' }, ...ATTRIBUTIONS.flatMap(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])),
    h('h3', {}, 'Licence'),
    h('pre', { class: 'license' }, MIT_LICENSE),
  );
}
