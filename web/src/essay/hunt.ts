// Section 8 bootstrap: the hunt. Four verified gliders and the two
// rejections, replayed from the committed campaign records.
import { mountNav } from './nav';
import { mountEssayPanel } from '../essay-panel';
import s21 from '../../../results/hat-tableevolve-r48-s21.jsonl?raw';
import s23 from '../../../results/hat-tableevolve-r48-s23.jsonl?raw';
import s33 from '../../../results/spectre-tableevolve-r48-s33.jsonl?raw';
import s32 from '../../../results/spectre-tableevolve-r48-s32.jsonl?raw';

mountNav();

const line = (raw: string): string => raw.split('\n')[0];

mountEssayPanel(document.getElementById('first')!, {
  record: line(s21),
  speed: 8,
  autoplay: true,
  follow: true,
  followZoom: 5,
  loopAtGeneration: 92,
  loopPauseMs: 2200,
  downloadName: 'hat-tableevolve-r48-s21.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/hat-tableevolve-r48-s21.jsonl 0 384 60000',
  caption:
    '<b>Hat glider A (run s21)</b> — the first glider we found on a hat ' +
    'tiling, from a two-cell seed at GA generation 5 of the first run. ' +
    'Two parallel gliders launch together; one dies at generation 42 on ' +
    'the other’s wake; the survivor holds max population 16, dead ' +
    'south-west, two generations per ring, verified flat to radius 768. ' +
    'This is the object the essay’s opening page flies forever.',
});

mountEssayPanel(document.getElementById('spectre-pair')!, {
  record: line(s33),
  radius: 32,
  speed: 10,
  autoplay: true,
  loopAtGeneration: 58,
  loopPauseMs: 2000,
  downloadName: 'spectre-tableevolve-r48-s33.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/spectre-tableevolve-r48-s33.jsonl 0 384 60000',
  caption:
    '<b>Spectre glider A (run s33)</b> — the spectre’s first: one ' +
    'two-cell seed, two gliders separating at ~120°, each exactly one ' +
    'firing cell per step, max population 12, the same two generations ' +
    'per ring. Both families host gliders; neither has rails.',
});

mountEssayPanel(document.getElementById('showpiece')!, {
  record: line(s23),
  speed: 10,
  autoplay: true,
  loopAtGeneration: 165,
  loopPauseMs: 2200,
  downloadName: 'hat-tableevolve-r48-s23.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example verify_candidate results/hat-tableevolve-r48-s23.jsonl 0 384 60000',
  caption:
    '<b>Hat glider C (run s23)</b> — born a wanderer: it meanders near ' +
    'the origin, population up to 32, then decays into two, then three ' +
    'directional gliders (two NNW, one SE). The 62-generation transient ' +
    'in its arrival law is exactly this adolescence; once launched it ' +
    'rides the universal clock.',
});

mountEssayPanel(document.getElementById('megalooper')!, {
  record: line(s32),
  radius: 96,
  speed: 20,
  autoplay: true,
  // Camera parked on wanderer 1's capture site (probe-located: the
  // 207-tile period-60 track centres on cell 2604 at this radius).
  focus: { cell: 2604, zoom: 3.3 }, // 6 minus two zoom-button notches (/1.35^2)
  downloadName: 'spectre-tableevolve-r48-s32.jsonl',
  reproduce:
    'cargo run --release -p tiling-core --example cycle_components results/spectre-tableevolve-r48-s32.jsonl 0 192 3300',
  caption:
    '<b>The looper (run s32)</b>, replayed at radius 96, camera fixed. ' +
    'Two wanderers launch. One leaves the frame and escapes this patch ' +
    'exactly like the verified gliders — at radius 96 nothing ' +
    'distinguishes it. The other is captured almost at once: from ' +
    'generation 92 it circulates the hexagonal track in view — a ' +
    '207-tile circuit, exact period 60 — forever. At radius 192 the ' +
    'escaper is caught too, 189 rings out (generation 992, period 550), ' +
    'and the full state locks with period 3,300: the beat of the two ' +
    'independent loops.',
});
