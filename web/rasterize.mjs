// Rasterize SVG figures for PDF inclusion / preview. Run from web/:
//   node rasterize.mjs <file.svg> [more.svg...]
// Playwright is a devDependency here; chromium comes from
// `npx playwright install chromium` (cached in ~/Library/Caches).
import { chromium } from 'playwright';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1900, height: 1400 }, deviceScaleFactor: 2 });
for (const f of process.argv.slice(2)) {
  await page.goto('file://' + (f.startsWith('/') ? f : process.cwd() + '/' + f));
  // A root SVG with only a width attribute gets height 100% of the
  // viewport, letterboxing the content; pin height from the viewBox.
  await page.evaluate(() => {
    const svg = document.querySelector('svg');
    const vb = svg.viewBox.baseVal;
    const w = Number(svg.getAttribute('width') || vb.width);
    svg.setAttribute('height', String((w * vb.height) / vb.width));
  });
  await page.locator('svg').first().screenshot({ path: f.replace(/\.svg$/, '.png') });
  console.log('rasterized', f);
}
await browser.close();
