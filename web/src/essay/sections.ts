// The essay's section manifest — the single runtime source of truth for
// order, titles, and the citable slugs (WRITEUP-PLAN holds the same
// list as prose). Slugs are permanent: the paper cites them.

export interface Section {
  /** URL path segment under essay/ ('' = the intro landing page). */
  slug: string;
  title: string;
  /** One line for the rail tooltip / stub page. */
  blurb: string;
  /** False renders as "in preparation" in the rail. */
  ready: boolean;
}

export const SECTIONS: Section[] = [
  {
    slug: '',
    title: 'A glider on the hat tiling',
    blurb: 'The object itself, flying live, and its history as a worldtube.',
    ready: true,
  },
  {
    slug: 'ca',
    title: 'The game',
    blurb: 'Cellular automata from Life to any graph: neighbourhoods, Generations, rule tables.',
    ready: true,
  },
  {
    slug: 'tilings',
    title: 'The board',
    blurb: 'The hat and spectre tilings, patches by address, tile classes and rings.',
    ready: true,
  },
  {
    slug: 'first-dynamics',
    title: 'The game on the board',
    blurb: 'Life on the hat, a census of every rule, and the first thing that moved.',
    ready: true,
  },
  {
    slug: 'first-searches',
    title: 'First searches',
    blurb: 'The mortal era: seed sweeps, traveller anatomy, richer rules, and a ceiling.',
    ready: true,
  },
  {
    slug: 'evolution',
    title: 'Genetic search',
    blurb: 'Evolving rule tables; how the fitness function was shaped.',
    ready: true,
  },
  {
    slug: 'penrose',
    title: 'Validation on Penrose',
    blurb: 'Reproducing the 2012 glider — including the printed table failing, live.',
    ready: true,
  },
  {
    slug: 'hunt',
    title: 'The hunt',
    blurb: 'The same pipeline pointed at the monotiles: four gliders in six runs.',
    ready: true,
  },
  {
    slug: 'flight',
    title: 'Tiling on the fly',
    blurb: 'The sliding window that follows a glider forever — the substrate as fog of war.',
    ready: true,
  },
  {
    slug: 'gliders',
    title: 'The gliders',
    blurb: 'The atlas: every glider flying without end, and how far they have flown.',
    ready: true,
  },
  {
    slug: 'compass',
    title: 'The survey',
    blurb: 'The million-ring measurements: the clock, the lanes, and an exact compass law.',
    ready: true,
  },
  {
    slug: 'controls',
    title: 'Structural controls',
    blurb: 'Every objective breeds its parasite; boundary sniffing, live.',
    ready: true,
  },
  {
    slug: 'certificate',
    title: 'Towards a proof',
    blurb: 'Situations, saturation, and closure: from a million rings to immortality.',
    ready: true,
  },
  {
    slug: 'playground',
    title: 'Playground',
    blurb: 'The full interactive UI — your licence to deviate.',
    ready: true,
  },
];
