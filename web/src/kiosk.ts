// Kiosk bootstrap: one glider, flying indefinitely (the sliding-window
// instrument of paper §2.4 as a reception-screen exhibit). Scroll to
// zoom between the close-up (an endless flight) and the wide view (the
// window re-rooting ahead of the glider).
import { mountFlightPanel } from './flight-panel';
import s21 from '../../results/hat-tableevolve-r48-s21.jsonl?raw';

// URL params: ?speed=N (gens/s), ?zoom=0..1, ?trail=N, ?fade=0..1,
// ?theme=dark|light — reception knobs. Speed 6 is the peaceful
// default; mid-twenties is about the practical ceiling before motion
// blurs perceptually. Light theme is the paper's print palette.
const params = new URLSearchParams(location.search);
const speed = Number(params.get('speed') ?? '') || 6;
const zoom = Number(params.get('zoom') ?? '');
const trail = Number(params.get('trail') ?? '');
const fade = Number(params.get('fade') ?? '');
const theme = params.get('theme') === 'light' ? ('light' as const) : ('dark' as const);
if (theme === 'light') document.body.classList.add('light');

mountFlightPanel(document.getElementById('flight')!, {
  record: s21.split('\n')[0],
  speed,
  autoplay: true,
  kiosk: true,
  zoomLevel: Number.isFinite(zoom) && params.has('zoom') ? zoom : 0.85,
  trail: Number.isFinite(trail) && params.has('trail') ? trail : 10,
  trailFade: Number.isFinite(fade) && params.has('fade') ? fade : 0,
  theme,
  downloadName: 'hat-tableevolve-r48-s21.jsonl',
});
