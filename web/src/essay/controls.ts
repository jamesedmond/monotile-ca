// Section 12 bootstrap: structural controls. One exhibit — the
// boundary-sniffing genome, grafted onto a committed hat record's root
// so the exploit runs exactly as evolved: seed at the centre, corners
// ignite at generation 1.
import { mountNav } from './nav';
import { mountEssayPanel } from '../essay-panel';
import gallery from '../../../results/gallery.jsonl?raw';
import s21 from '../../../results/hat-tableevolve-r48-s21.jsonl?raw';
import s32 from '../../../results/spectre-tableevolve-r48-s32.jsonl?raw';
import s11 from '../../../results/penrosep3-tableevolve-r48-s11.jsonl?raw';

mountNav();

// Build the sniffer record: the gallery's hat walker record supplies a
// legal (family, root, radius, edge-neighbourhood) shell; the table
// rule is the ablation's champion genome, verbatim.
const sniffer = ((): string => {
  const r = JSON.parse(gallery.split('\n')[0]);
  r.table_rule = {
    states: 4,
    rows: [
      { conds: [[0, 3]], next: 0 }, // any cell with >= 3 dead neighbours: stay/fall dead
      { next: 1 }, // everything else — only the low-degree rim qualifies — ignites
    ],
  };
  r.initial_cells = [0];
  r.initial_states = [1];
  r.note = 'boundary-sniffing champion genome (ablation, edge-table cell)';
  return JSON.stringify(r);
})();

mountEssayPanel(document.getElementById('sniffer')!, {
  record: sniffer,
  speed: 2,
  autoplay: true,
  loopAtGeneration: 6,
  loopPauseMs: 2500,
  caption:
    '<b>The boundary sniffer</b> — a single seed at the centre of a ' +
    'radius-48 edge-adjacency patch, under the champion genome exactly ' +
    'as evolved. At generation 1 the seed is gone and two low-degree ' +
    'corner tiles ignite instead — at ring 48, the boundary itself — ' +
    'and then nothing ever moves again (the ablation’s own patches lit ' +
    'up to five). Cross-radius verification passes it: every bigger ' +
    'patch has corners too. The causality filter kills it: no physical ' +
    'signal reaches ring 48 in one generation.',
  lightCone: 1,
});

mountEssayPanel(document.getElementById('cone-glider')!, {
  record: s21.split('\n')[0],
  speed: 4,
  autoplay: true,
  loopAtGeneration: 30,
  loopPauseMs: 2000,
  lightCone: 2,
  downloadName: 'hat-tableevolve-r48-s21.jsonl',
  caption:
    '<b>Hat A under the causality overlay</b> — the allowed cone grows ' +
    'two rings per generation (the vertex fan margin); the glider ' +
    'advances one ring every two generations. Physics stays deep ' +
    'inside its light cone, and the filter costs a genuine glider ' +
    'nothing.',
});

const cross = document.getElementById('crossradius')!;
const mountCrossLive = (): void => {
  cross.innerHTML = '';
  mountEssayPanel(cross, crossOptions);
};
cross.innerHTML = `
  <figure class="xr-static">
    <img src="${import.meta.env.BASE_URL}essay-assets/looper-trace.png" alt="Lifetime trace of the spectre looper on a radius-192 board, with verification rings at radius 48, 96 and 192" />
    <figcaption>
      <b>The looper against the rings</b> — lifetime trace, rendered by
      the same engine as every live panel: transient track in dim gold,
      the settled period-60 and period-550 circuits in amber, and the
      full state repeating with period 3,300 ever after. Escape at one
      radius is a claim; escape at r, 2r, 4r is a verification.
      <button class="xr-live">▶ watch it live — builds the 124,237-tile board in your browser; takes ~10 seconds, it is not stuck</button>
    </figcaption>
  </figure>`;
cross.querySelector('.xr-live')!.addEventListener('click', mountCrossLive);

const crossOptions = {
  record: s32.split('\n')[0],
  radius: 192,
  speed: 60,
  autoplay: true,
  follow: true,
  followZoom: 2.5,
  followCluster: 0.06,
  followFarthest: true,
  loopAtGeneration: 1150,
  loopPauseMs: 2500,
  rings: [48, 96, 192],
  downloadName: 'spectre-tableevolve-r48-s32.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/spectre-tableevolve-r48-s32.jsonl 0 192 20000',
  caption:
    '<b>The looper against the rings, live</b> — 124,237 tiles, ' +
    'verification bands at radius 48, 96 and 192. The camera follows ' +
    'the travelling wanderer across the first two bands to its capture ' +
    'at ring 189; the full state then repeats with period 3,300 ' +
    'forever.',
};

mountEssayPanel(document.getElementById('cone-phoenix')!, {
  record: s11.split('\n')[0],
  speed: 6,
  autoplay: true,
  lightCone: 1,
  loopAtGeneration: 50,
  loopPauseMs: 2200,
  downloadName: 'penrosep3-tableevolve-r48-s11.jsonl',
  caption:
    '<b>The phoenix on the rim</b> — both gliders sit exactly on the ' +
    'edge of the one-ring-per-generation disc and stay there as it ' +
    'grows: no cell of the pattern survives a generation, so what ' +
    'travels is rebirth at the maximum sustainable rate. Note this ' +
    'overlay is a measurement, not the filter: the sound causality ' +
    'bound on P3 is three rings per generation, and that cone would ' +
    'race ahead.',
});
