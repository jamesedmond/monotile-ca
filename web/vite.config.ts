import { defineConfig } from 'vite';
import { resolve } from 'node:path';

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
