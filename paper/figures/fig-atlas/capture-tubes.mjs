// Atlas oblique worldtube panels: one per monotile glider, light
// palette, above view (auto-orient), fade 120, frozen mid-cruise;
// cropped to the occupied bottom-left 2/3. Requires the dev server
// (localhost:5173) and playwright (installed in web/). Run via
// generate.sh from the repository root.
import { chromium } from 'playwright';
import { writeFileSync } from 'node:fs';

const OUT = process.env.FIG_OUT ?? '.';
const SHOTS = [
  // (name, url params, freeze generation, crop anchor). Crops are
  // square, side = canvas height; 'bl' anchors left, 'center' centers.
  ['s21', 'tuberecord=s21', 150, 'bl'],
  ['s22', 'tuberecord=s22', 150, 'bl'],
  ['s23', 'tuberecord=s23&tubefollow=one', 175, 'center'],
  ['s33', 'tuberecord=s33&tubefollow=one', 150, 'bl'],
];

const browser = await chromium.launch();
for (const [name, params, freeze, crop] of SHOTS) {
  const page = await browser.newPage({ viewport: { width: 1100, height: 850 }, deviceScaleFactor: 2 });
  page.on('pageerror', (e) => console.log('PAGE ERROR:', e.message));
  await page.goto(`http://localhost:5173/essay/?tubetheme=light&tubefade=120&${params}`);
  await page.locator('#worldtube').scrollIntoViewIfNeeded();
  await page.waitForFunction(() => !document.querySelector('#worldtube .ep-loading'), null, { timeout: 120000 });
  await page.evaluate(() => {
    const s = document.querySelector('#worldtube .ep-speed');
    s.value = '30';
    s.dispatchEvent(new Event('input'));
  });
  await page.waitForFunction(
    (g) => Number(document.querySelector('#worldtube .ep-gen')?.textContent) >= g,
    freeze, { timeout: 120000 },
  );
  await page.evaluate(() => document.querySelector('#worldtube .ep-play').click());
  await page.waitForTimeout(9000); // settle auto-orient (multi-cluster records swing far)
  const gen = await page.textContent('#worldtube .ep-gen');
  const data = await page.evaluate(
    (cropMode) => new Promise((resolve) => {
      requestAnimationFrame(() => {
        const gl = document.querySelector('#worldtube .st-canvas');
        const c = document.createElement('canvas');
        const side = gl.height;
        c.width = side;
        c.height = side;
        if (cropMode === 'center') {
          c.getContext('2d').drawImage(gl, Math.round((gl.width - side) / 2), 0, side, side, 0, 0, side, side);
        } else {
          c.getContext('2d').drawImage(gl, 0, 0, side, side, 0, 0, side, side);
        }
        resolve(c.toDataURL('image/png'));
      });
    }), crop,
  );
  writeFileSync(`${OUT}/${name}-tube.png`, Buffer.from(data.split(',')[1], 'base64'));
  console.log(`captured ${name}-tube.png at generation ${gen}`);
  await page.close();
}
await browser.close();
