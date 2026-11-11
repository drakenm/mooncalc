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
    const [info, criteria] = await Promise.all([backend.appInfo(), backend.criteriaList()]);
    const store = new Store();

    // Repair persisted choices that this backend doesn't offer.
    const s = store.get();
    if (!criteria.some((c) => c.id === s.criterionId) && criteria[0]) {
      store.update({ criterionId: criteria[0].id });
    }
    if (!info.ephemerisBackends.some((b) => b.id === s.ephemerisId && b.available)) {
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
