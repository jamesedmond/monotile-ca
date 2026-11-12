import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import { existsSync } from 'node:fs';

export default defineConfig({
  // The essay's permanent home is offlattice.org/monotile/ — every asset
  // URL is prefixed for that sub-path. Dev serves at localhost:5173/monotile/.
  base: '/monotile/',
  build: {
    rollupOptions: {
      input: {
        playground: resolve(__dirname, 'index.html'),
        essayLegacyRedirect: resolve(__dirname, 'essay.html'),
        kiosk: resolve(__dirname, 'kiosk.html'),
        essay: resolve(__dirname, 'essay/index.html'),
        // The old results gallery (results/) is private and superseded by
        // the census pages: it is served by the dev server but only
        // enters a build when MONOTILE_RESULTS_PAGE is set, so a routine
        // deploy cannot publish it by accident.
        ...(process.env.MONOTILE_RESULTS_PAGE && existsSync(resolve(__dirname, 'results/index.html'))
          ? { results: resolve(__dirname, 'results/index.html') }
          : {}),
        // The census pages (census/), published 2026-11-12 with the
        // certificate paper, which links them.
        ...Object.fromEntries(
          ['', 'convoys/', 'singles/', 'second-lane/', 'showers/'].map((p) => [
            `census-${p.replace('/', '') || 'index'}`,
            resolve(__dirname, `census/${p}index.html`),
          ]),
        ),
        ...Object.fromEntries(
          [
            'ca',
            'tilings',
            'first-dynamics',
            'first-searches',
            'evolution',
            'penrose',
            'hunt',
            'controls',
            'gliders',
            'flight',
            'compass',
            'certificate',
            'playground-section',
            'sources',
          ].map((slug) => [
            `essay-${slug}`,
            resolve(
              __dirname,
              `essay/${slug === 'playground-section' ? 'playground' : slug}/index.html`,
            ),
          ]),
        ),
      },
    },
  },
  server: {
    fs: {
      // Essay panels import ResultRecords verbatim from ../results.
      allow: ['..'],
    },
  },
});
