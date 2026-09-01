// Headless verification of the kiosk flight panel (temporary script).
// Headless Chromium misses the WebGL layer in page.screenshot, so
// frames are captured by copying the canvas into a 2D canvas inside a
// rAF callback and reading toDataURL (the repo's standard technique).
import { chromium } from 'playwright';
import { writeFileSync } from 'node:fs';

const OUT = process.argv[2] ?? '.';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on('console', (m) => {
  if (m.type() === 'error') errors.push(m.text());
});
page.on('pageerror', (e) => errors.push(String(e)));

await page.goto('http://localhost:5199/kiosk.html?speed=60&zoom=0.85');
await page.waitForFunction(
  () => {
    const el = document.querySelector('.ep-loading');
    return el && getComputedStyle(el).display === 'none';
  },
  undefined,
  { timeout: 180000 },
);
console.log('loaded');

const odo = () => page.textContent('.fp-odometer');

const shot = async (name) => {
  const data = await page.evaluate(
    () =>
      new Promise((res) => {
        requestAnimationFrame(() => {
          const src = document.querySelector('.ep-canvas');
          const c = document.createElement('canvas');
          c.width = src.width;
          c.height = src.height;
          c.getContext('2d').drawImage(src, 0, 0);
          res(c.toDataURL('image/png'));
        });
      }),
  );
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(data.split(',')[1], 'base64'));
  console.log(`${name}: ${await odo()}`);
};

// Longest rAF gap over 6 s while running at speed — the hop hitch.
const hitchProbe = page.evaluate(
  () =>
    new Promise((res) => {
      let last = performance.now();
      let worst = 0;
      const t0 = last;
      const probe = (now) => {
        worst = Math.max(worst, now - last);
        last = now;
        if (now - t0 < 6000) requestAnimationFrame(probe);
        else res(worst);
      };
      requestAnimationFrame(probe);
    }),
);

// Wait until at least 2 hops have happened (geometry re-uploads exercised).
await page.waitForFunction(
  () => {
    const m = document.querySelector('.fp-odometer')?.textContent?.match(/hops (\d+)/);
    return m && Number(m[1]) >= 2;
  },
  undefined,
  { timeout: 180000 },
);
await shot('flight-zoomin');
console.log('worst rAF gap (ms):', Math.round(await hitchProbe));

// Zoom out: the whole window plus terrain generation should be visible.
await page.evaluate(() => {
  const z = document.querySelector('.fp-zoom');
  z.value = '0.05';
  z.dispatchEvent(new Event('input'));
});
await page.waitForTimeout(1200);
await shot('flight-zoomout');

// Let it fly across a few more hops zoomed out, then capture again to
// confirm the window visibly moved on (re-rooted ahead of the glider).
const hops0 = Number((await odo()).match(/hops ([\d,]+)/)[1].replace(/,/g, ''));
await page.waitForFunction(
  (h0) => {
    const m = document.querySelector('.fp-odometer')?.textContent?.match(/hops ([\d,]+)/);
    return m && Number(m[1].replace(/,/g, '')) >= h0 + 2;
  },
  hops0,
  { timeout: 180000 },
);
await shot('flight-zoomout-later');

console.log('console errors:', errors.length ? errors : 'none');
await browser.close();
