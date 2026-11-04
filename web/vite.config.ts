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
        // The glider census (results/) is unpublished certificate-paper
        // material: it is served by the dev server but only enters a
        // build when MONOTILE_RESULTS_PAGE is set, so a routine essay
        // deploy cannot publish it by accident.
        ...(process.env.MONOTILE_RESULTS_PAGE && existsSync(resolve(__dirname, 'results/index.html'))
          ? { results: resolve(__dirname, 'results/index.html') }
          : {}),
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
