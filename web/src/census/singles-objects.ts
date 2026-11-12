// The census's single gliders, one list per rule (FINDINGS 10.11.5,
// 10.11.58, 10.11.63, 10.11.67). Every one flies from a settled-object
// record, so a viewer shows the glider alone; the discovery paper's four
// are cut from the paper's own runs. Shared by the single-gliders page
// and the index.
import type { CensusObject } from './viewer';
import { object, objectRecordFact, HAT_A, HAT_B, HAT_C, SPECTRE, hatWindow, spectreWindow, HAT_SPEED, sig } from './records';

const launches = (n: number, of: number): [string, string] => [
  'launches',
  `${n} in the first census (${of} seedings)`,
];
const COLLAR_5: [string, string] = [
  'certificate',
  'collar 5; at collar 4 the 257, 256 and 127 share one closed set of 630 situations',
];
const PAPER_RUN = "the discovery paper's run";

/** The five gliders that hat A's and hat B's rules share, in either alphabet. */
const family = (
  rule: string,
  prefix: 'hatA' | 'hatB',
  population: [string, string, string],
  signatures: [string, string, string, string, string],
  referenceNote?: string,
): CensusObject[] => [
  {
    key: `${prefix}-381`,
    tab: '381',
    rule,
    title: prefix === 'hatA' ? 'Hat A, 381 states' : 'Hat B, 381 states',
    kind: "The discovery paper's glider.",
    ...object(`${prefix}-381`),
    facts: [
      ['states', '381 (271 class-blind)'],
      ['population', population[0]],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(53, 188),
      sig(signatures[0]),
      objectRecordFact(`${prefix}-381`, PAPER_RUN),
    ],
    note: referenceNote,
  },
  {
    key: `${prefix}-380`,
    tab: '380',
    rule,
    title: 'Second single glider, 380 states',
    kind: 'The most common single glider of this rule.',
    ...object(`${prefix}-380`),
    facts: [
      ['states', '380 (265 class-blind)'],
      ['population', population[1]],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(104, 188),
      sig(signatures[1]),
      objectRecordFact(`${prefix}-380`),
    ],
    note:
      'It shares no situation with the 381-state glider. With the terrain stripped away it shares no pattern ' +
      'with it either.',
  },
  {
    key: `${prefix}-257`,
    tab: '257',
    rule,
    title: '257 states',
    kind:
      prefix === 'hatA'
        ? "The discovery paper's captured object, first met as a hat A flight that appeared to change lane."
        : "Hat B's form of the discovery paper's captured object.",
    ...object(`${prefix}-257`),
    facts: [
      ['states', '257 (190 class-blind)'],
      ['population', population[2]],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(39, 188),
      sig(signatures[2]),
      COLLAR_5,
      objectRecordFact(`${prefix}-257`),
    ],
  },
  {
    key: `${prefix}-256`,
    tab: '256',
    rule,
    title: '256 states',
    kind: 'A close relative of the 257: the two share 45 of about 65 patterns.',
    ...object(`${prefix}-256`),
    facts: [
      ['states', '256 (189 class-blind)'],
      ['population', population[2]],
      hatWindow('15.6087', '0.1192'),
      HAT_SPEED,
      launches(67, 188),
      sig(signatures[3]),
      COLLAR_5,
      objectRecordFact(`${prefix}-256`),
    ],
  },
  {
    key: `${prefix}-127`,
    tab: '127',
    rule,
    title: '127 states',
    kind: "The smallest certificate under this rule.",
    ...object(`${prefix}-127`),
    facts: [
      ['states', '127 (84 class-blind)'],
      ['population', population[2]],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(93, 188),
      sig(signatures[4]),
      COLLAR_5,
      objectRecordFact(`${prefix}-127`),
    ],
  },
];

export const hatA = family(
  HAT_A,
  'hatA',
  ['6–8', '6–7', '6–8'],
  ['cc524fb136bc', '1dbb41448014', 'e9040de0b644', 'f51cff2f64ec', '90e54cf418a2'],
  "The discovery paper's seed launches two gliders, and one dies on the wake of the other at generation 42. " +
    'This flight starts from the survivor.',
);
export const hatB = family(
  HAT_B,
  'hatB',
  ['5–7', '5–6', '5–7'],
  ['9ba8346bd1da', '25b8be6525b8', '371745109611', '69d85b70c75b', '76e3086b87d3'],
);

export const hatC: CensusObject[] = [
  {
    key: 'hatC-381',
    tab: '381',
    rule: HAT_C,
    title: 'Hat C, 381 states',
    kind: "The discovery paper's glider on the 345° heading.",
    ...object('hatC-381'),
    facts: [
      ['states', '381 (275 class-blind)'],
      ['population', '6–9'],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(147, 188),
      sig('b679e54222be'),
      objectRecordFact('hatC-381', PAPER_RUN),
    ],
    note:
      "The discovery paper's seed wanders for about 150 generations and then breaks into three gliders. This " +
      'flight starts from one of them. With its state-2 cells removed, 237 of this glider’s 381 situations ' +
      'are situations of hat B’s glider, but no transition carries over, so it is a different object.',
  },
  {
    key: 'hatC-386',
    tab: '386',
    rule: HAT_C,
    title: '386 states',
    kind: 'A second single glider.',
    ...object('hatC-386'),
    facts: [
      ['states', '386 (275 class-blind)'],
      ['population', '6–10'],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(21, 188),
      sig('76337d9ae120'),
      objectRecordFact('hatC-386'),
    ],
  },
  {
    key: 'hatC-129',
    tab: '129',
    rule: HAT_C,
    title: '129 states',
    kind: 'The smallest certificate under this rule.',
    ...object('hatC-129'),
    facts: [
      ['states', '129 (85 class-blind)'],
      ['population', '6–10'],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(38, 188),
      sig('be7e896d2b60'),
      objectRecordFact('hatC-129'),
    ],
  },
  {
    key: 'hatC-257a',
    tab: '257',
    rule: HAT_C,
    title: '257 states',
    kind: 'The glider the discovery paper flew on its 105° heading.',
    ...object('hatC-257a'),
    facts: [
      ['states', '257 (191 class-blind)'],
      ['population', '6–10'],
      hatWindow('15.6084', '0.1189'),
      HAT_SPEED,
      launches(25, 188),
      sig('5c29f970e0b1'),
      objectRecordFact('hatC-257a'),
    ],
  },
  {
    key: 'hatC-257b',
    tab: '257′',
    rule: HAT_C,
    title: 'Second 257-state glider',
    kind: 'A different object with the same number of situations.',
    ...object('hatC-257b'),
    facts: [
      ['states', '257 (195 class-blind)'],
      ['population', '6–10'],
      hatWindow('15.6087', '0.1192'),
      HAT_SPEED,
      launches(35, 188),
      sig('6248ef60819e'),
      objectRecordFact('hatC-257b'),
    ],
  },
];

export const spectre: CensusObject[] = [
  {
    key: 'spectre-2542',
    tab: '2,542',
    rule: SPECTRE,
    title: 'The spectre glider, 2,542 states',
    kind: "The discovery paper's glider, on the first lane family.",
    ...object('spectre-2542'),
    facts: [
      ['states', '2,542 (1,920 class-blind)'],
      ['population', '4–6'],
      spectreWindow('17.7448', '18.1374', '0.3926'),
      ['speed', '≥ 1.5807 units / generation'],
      launches(164, 114),
      sig('6239ceef8610'),
      objectRecordFact('spectre-2542', PAPER_RUN),
    ],
    note:
      "The discovery paper's seed launches two of these gliders 120° apart. This flight starts from one of them.",
  },
  {
    key: 'spectre-1454',
    tab: '1,454',
    rule: SPECTRE,
    title: 'Second spectre glider, 1,454 states',
    kind: 'A glider on a second lane family, about 9° from the first.',
    ...object('spectre-1454'),
    facts: [
      ['states', '1,454 (1,110 class-blind)'],
      ['population', '4–8'],
      spectreWindow('26.9589', '27.5126', '0.5537'),
      ['speed', '≥ 1.5441 units / generation'],
      launches(96, 114),
      sig('c5c1b13c1fd6'),
      objectRecordFact('spectre-1454'),
    ],
    note:
      'Flown for a million rings, its heading is 9.156° from the first glider’s. The certificate’s ' +
      'window excludes every heading the first glider can take.',
  },
];
