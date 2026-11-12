// Census index: every certified object in one table of kinds against
// rules, each linking to its tab on a category page, and the same
// objects as a list. Built from the lists the category pages use, so
// the index cannot disagree with them.
import '../essay/essay.css';
import './census.css';
import type { CensusObject } from './viewer';
import { hatA, hatB, hatC, spectre } from './singles-objects';
import { formation, bound, passengers } from './convoys-objects';
import { HAT_A, HAT_B, HAT_C, SPECTRE } from './records';

interface Kind {
  name: string;
  /** Category page, relative to the index. */
  page: string;
  objects: CensusObject[];
}
const KINDS: Kind[] = [
  { name: 'Single gliders', page: 'singles/', objects: [...hatA, ...hatB, ...hatC, ...spectre] },
  { name: 'Formation pairs', page: 'convoys/', objects: formation },
  { name: 'Bound pairs', page: 'convoys/', objects: bound },
  { name: 'Gliders with passengers', page: 'convoys/', objects: passengers },
];
const RULES = [HAT_A, HAT_B, HAT_C, SPECTRE];
const TOTAL = 32;

const all = KINDS.flatMap((k) => k.objects);
if (all.length !== TOTAL || new Set(all.map((o) => o.key)).size !== TOTAL) {
  throw new Error(`the index expects ${TOTAL} distinct objects and found ${all.length}`);
}

const plain = (html: string): string => html.replace(/<[^>]+>/g, '').replace(/"/g, '&quot;');
const fact = (o: CensusObject, label: string): string => o.facts.find(([k]) => k === label)?.[1] ?? '';
const link = (k: Kind, o: CensusObject, cls = ''): string =>
  `<a${cls ? ` class="${cls}"` : ''} href="${k.page}#${o.key}" title="${plain(`${o.title}. ${o.kind}`)}">${o.tab}</a>`;

// --- kinds against rules ----------------------------------------------
const count = (n: number): string => `<span class="cx-count">${n}</span>`;
document.getElementById('census-table')!.innerHTML =
  `<table class="cx-matrix">` +
  `<thead><tr><th></th>${RULES.map((r) => `<th scope="col">${r} ${count(all.filter((o) => o.rule === r).length)}</th>`).join('')}</tr></thead>` +
  `<tbody>${KINDS.map(
    (k) =>
      `<tr><th scope="row"><a href="${k.page}">${k.name}</a> ${count(k.objects.length)}</th>${RULES.map((r) => {
        const here = k.objects.filter((o) => o.rule === r);
        return `<td>${here.length > 0 ? here.map((o) => link(k, o, 'cx-chip')).join('') : '<span class="cx-none">none certified</span>'}</td>`;
      }).join('')}</tr>`,
  ).join('')}</tbody></table>`;

// --- the same objects as a list -----------------------------------------
document.getElementById('census-list')!.innerHTML =
  `<table class="cx-list">` +
  `<thead><tr><th scope="col">object</th><th scope="col">rule</th><th scope="col">what it is</th><th scope="col">situations</th><th scope="col">cells</th></tr></thead>` +
  KINDS.map(
    (k) =>
      `<tbody><tr class="cx-kind"><th colspan="5" scope="rowgroup">${k.name}</th></tr>${k.objects
        .map(
          (o) =>
            `<tr><td>${link(k, o)}</td><td>${o.rule.replace(/ rule$|'s rule$/, '')}</td><td>${o.kind}</td>` +
            `<td>${fact(o, 'states').replace(/ \(.*$/, '')}</td><td>${fact(o, 'population')}</td></tr>`,
        )
        .join('')}</tbody>`,
  ).join('') +
  `</table>`;
