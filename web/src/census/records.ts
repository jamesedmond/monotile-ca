// Records behind the census pages. Two sources: the first census's
// seeds (census-seeds.jsonl), and one settled-object record per census
// object (census-objects.jsonl, made and checked by
// crates/certify/census/census_objects.py), looked up by the key in
// its note. The discovery paper's four gliders are settled-object
// records too, cut from the paper's own runs.
import type { CensusObject } from './viewer';
import seedsRaw from '../../../results/certificates/census-seeds.jsonl?raw';
import objectsRaw from '../../../results/certificates/census-objects.jsonl?raw';

type Source = Pick<CensusObject, 'record' | 'downloadName' | 'reproduce' | 'flight'>;
const lines = (raw: string): string[] => raw.split('\n').filter((l) => l.trim() !== '');
const slide = (file: string, index: number, extra = ''): string =>
  `cargo run --release -p tiling-core --example slide results/${file} ${index} 24 --launch-radius 48 --launch-gens 60${extra}`;

const seeds = lines(seedsRaw);
/** A record of the first census's seed file, by index. */
export const seed = (i: number): Source => ({
  record: seeds[i],
  downloadName: `census-seed-${i}.jsonl`,
  reproduce: slide('certificates/census-seeds.jsonl', i),
});

const objects = lines(objectsRaw);
const objectIndex = new Map(
  objects.map((line, i) => [/"note":"census object ([^:]+):/.exec(line)?.[1] ?? `#${i}`, i]),
);
/** A settled-object record, by its key (e.g. 'hatA-638'). */
export const object = (key: string): Source => {
  const i = objectIndex.get(key);
  if (i === undefined) throw new Error(`no census object record for ${key}`);
  return {
    record: objects[i],
    downloadName: `census-object-${key}.jsonl`,
    reproduce: slide('certificates/census-objects.jsonl', i),
  };
};
/** Where a settled-object record sits in its file, for the figures table. */
export const objectRecordFact = (key: string, run = 'a census run'): [string, string] => [
  'record',
  `census-objects.jsonl #${objectIndex.get(key)}, the settled object, cut from ${run} at generation 300`,
];

export const HAT_A = "Hat A's rule";
export const HAT_B = "Hat B's rule";
export const HAT_C = "Hat C's rule";
export const SPECTRE = 'Spectre rule';
export const hatWindow = (hi: string, width: string): [string, string] => [
  'heading window',
  `15.4895° – ${hi}° mod 30 (width ${width}°)`,
];
export const spectreWindow = (lo: string, hi: string, width: string): [string, string] => [
  'heading window',
  `${lo}° – ${hi}° mod 30 (width ${width}°)`,
];
export const HAT_SPEED: [string, string] = ['speed', '≥ 2.1396 units / generation'];
export const sig = (s: string): [string, string] => ['certificate signature', `<code>${s}</code>`];
