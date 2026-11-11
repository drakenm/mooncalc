// Entry point: choose a backend transport, load static data, build the UI.

import './styles.css';

import { BackendError, createBackend } from './backend';
import { Store } from './state';
import { createApp } from './ui/app';
import { h } from './ui/dom';

async function main(): Promise<void> {
  const root = document.getElementById('app');
  if (!root) throw new Error('#app missing');
  try {
    const backend = await createBackend();
    let [info, criteria] = await Promise.all([backend.appInfo(), backend.criteriaList()]);
    const store = new Store();

    // Repair persisted choices that this backend doesn't offer; reload a
    // previously chosen DE440 file.
    const s = store.get();
    if (!criteria.some((c) => c.id === s.criterionId) && criteria[0]) {
      store.update({ criterionId: criteria[0].id });
    }
    const de440 = info.ephemerisBackends.find((b) => b.id === 'de440');
    if (s.ephemerisId === 'de440' && de440?.available && !de440.ready && s.de440Path) {
      try {
        await backend.loadDe440({ path: s.de440Path });
        info = await backend.appInfo();
      } catch (err) {
        console.warn('DE440 file could not be reloaded', err);
      }
    }
    if (!info.ephemerisBackends.some((b) => b.id === store.get().ephemerisId && b.available && b.ready)) {
      store.update({ ephemerisId: info.defaultEphemeris });
    }

    root.replaceChildren(createApp({ backend, store, info, criteria }));
  } catch (err) {
    root.replaceChildren(
      h('div', { class: 'fatal' }, h('h1', {}, 'MoonCalc could not start'), h('pre', {}, BackendError.from(err).message)),
    );
    throw err;
  }
}

void main();
