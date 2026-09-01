// Section 10 bootstrap: the atlas. One sub-section per glider — the
// live flight, the rule table rendered from the committed record's own
// table_rule (provenance-true, dead rows and all), and the worldtube.
import { mountNav } from './nav';
import { mountFlightPanel } from '../flight-panel';
import { mountSpaceTimePanel } from '../spacetime-panel';
import s21 from '../../../results/hat-tableevolve-r48-s21.jsonl?raw';
import s22 from '../../../results/hat-tableevolve-r48-s22.jsonl?raw';
import s23 from '../../../results/hat-tableevolve-r48-s23.jsonl?raw';
import s33 from '../../../results/spectre-tableevolve-r48-s33.jsonl?raw';

mountNav();

const line = (raw: string): string => raw.split('\n')[0];
const slideCmd = (file: string, extra = ''): string =>
  `cargo run --release -p tiling-core --example slide results/${file} 0 24 --launch-radius 48 --launch-gens 60${extra}`;

interface TableRow {
  own?: number;
  conds?: [number, number][];
  next: number;
}

/** Render a record's table_rule as the essay's priority-table HTML. */
const ruleTableHtml = (record: string): string => {
  const tr = JSON.parse(record).table_rule as { states: number; rows: TableRow[] };
  const rows = tr.rows
    .map((r) => {
      const own = r.own === undefined ? '*' : String(r.own);
      const conds =
        r.conds && r.conds.length > 0
          ? r.conds.map(([s, m]) => `n<sub>${s}</sub> ≥ ${m}`).join(' ∧ ')
          : '—';
      return `<tr><td>${own}</td><td>${conds}</td><td>${r.next}</td></tr>`;
    })
    .join('');
  return (
    `<table class="rule"><tr><th>own state</th><th>conditions</th><th>next state</th></tr>` +
    rows +
    `<tr><td>*</td><td>otherwise</td><td>0</td></tr></table>`
  );
};

interface GliderEntry {
  key: string;
  record: string;
  download: string;
  flight: { launchGens?: number; selectHeading?: number };
  flyCaption: string;
  tubeCaption: string;
  tubeSelectHeading?: number;
}

const ENTRIES: GliderEntry[] = [
  {
    key: 'hat-a',
    record: line(s21),
    download: 'hat-tableevolve-r48-s21.jsonl',
    flight: {},
    flyCaption:
      '<b>Hat A</b> — the minimal relay: one or two firing cells, reborn ' +
      'ahead each generation, population never above 16. The first ' +
      'glider ever seen on a hat tiling, found at GA generation 5 of ' +
      'the first run; the object the essay’s opening page flies.',
    tubeCaption:
      '<b>Hat A in space-time</b> — the launch pair braid until one dies ' +
      'on the other’s wake at generation 42, a worldline vertex; the ' +
      'survivor’s tube climbs at constant slope, two generations per ' +
      'ring, for as long as you watch. History fades astern over 64 ' +
      'generations.',
  },
  {
    key: 'hat-b',
    record: line(s22),
    download: 'hat-tableevolve-r48-s22.jsonl',
    flight: {},
    flyCaption:
      '<b>Hat B</b> — an unrelated genome with a phase-alternating gait: ' +
      'four or five firing cells swinging side-to-side across the axis ' +
      'of travel. It flies the same lane as A — a fact the survey ' +
      'returns to.',
    tubeCaption:
      '<b>Hat B</b> — the phase-alternating gait reads as a fine weave ' +
      'in the column. Same lane as A, different rule, same slope.',
  },
  {
    key: 'hat-c',
    record: line(s23),
    download: 'hat-tableevolve-r48-s23.jsonl',
    flight: { launchGens: 150, selectHeading: 345 },
    flyCaption:
      '<b>Hat C, the lone lane</b> — the wanderer-born glider from the ' +
      'hunt, flown alone: the launch runs 150 generations so the ' +
      'wanderer finishes decaying into three gliders, then lane ' +
      'selection keeps the one nearest 345° and drops its siblings.',
    tubeCaption:
      '<b>Hat C, the lone lane</b> — 150 generations of wandering ' +
      'adolescence at the base of the tube, then lane selection and a ' +
      'clean climb on the 345° heading. The startup is the best argument ' +
      'in the atlas that wanderers and gliders are one family.',
  },
  {
    key: 'spectre-a',
    record: line(s33),
    download: 'spectre-tableevolve-r48-s33.jsonl',
    flight: { selectHeading: 108 },
    flyCaption:
      '<b>Spectre A, the 108° lane</b> — maximally minimal: exactly one ' +
      'firing cell per step, population 12. The worldtube below flies ' +
      'its twin — the 348° lane from the same seed.',
    tubeCaption:
      '<b>Spectre A, the 348° lane</b> — the twin from the same seed, ' +
      'flying the opposite arm of the launch: one firing cell per step, ' +
      'the thinnest possible worldtube, climbing dead straight.',
    tubeSelectHeading: 348,
  },
];

for (const e of ENTRIES) {
  mountFlightPanel(document.getElementById(`fly-${e.key}`)!, {
    record: e.record,
    speed: 8,
    autoplay: true,
    zoomLevel: 0.85,
    trail: 10,
    launchGens: e.flight.launchGens,
    selectHeading: e.flight.selectHeading,
    caption: e.flyCaption,
    downloadName: e.download,
    reproduce: slideCmd(
      e.download,
      (e.flight.launchGens !== undefined ? ` --launch-gens ${e.flight.launchGens}` : '') +
        (e.flight.selectHeading !== undefined ? ` --select-heading ${e.flight.selectHeading}` : ''),
    ),
  });
  document.getElementById(`rule-${e.key}`)!.innerHTML = ruleTableHtml(e.record);
  mountSpaceTimePanel(document.getElementById(`tube-${e.key}`)!, {
    record: e.record,
    flight: {
      launchGens: e.flight.launchGens,
      selectHeading: e.tubeSelectHeading ?? e.flight.selectHeading,
    },
    speed: 10,
    autoplay: true,
    dzPerGeneration: 1.6,
    fadeGenerations: 64,
    cameraDistance: 110,
    caption: e.tubeCaption,
    downloadName: e.download,
  });
}
