// Main-thread smoothness probe for the kiosk flight (temporary tool):
// waits for 3 hops (warm-up), then records the worst rAF gap, all
// longtasks, and the top applyWindow durations over 10 s. Note that
// headless Chromium software-rasterizes WebGL, so draw-bound numbers
// here are far worse than any real GPU; applyWindow and longtask
// attribution are the meaningful signals.
import { chromium } from 'playwright';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto('http://localhost:5199/kiosk.html?speed=60&zoom=0.85');
await page.waitForFunction(
  () => document.querySelector('.fp-odometer')?.textContent?.match(/hops (\d+)/) &&
        Number(document.querySelector('.fp-odometer').textContent.match(/hops (\d+)/)[1]) >= 3,
  undefined, { timeout: 180000 },
);
// Warmed up. Measure worst rAF gap + longtasks over 10 s.
const result = await page.evaluate(
  () =>
    new Promise((res) => {
      const longtasks = [];
      new PerformanceObserver((l) => {
        for (const e of l.getEntries()) longtasks.push(Math.round(e.duration));
      }).observe({ entryTypes: ['longtask'] });
      let last = performance.now();
      let worst = 0;
      const t0 = last;
      const probe = (now) => {
        worst = Math.max(worst, now - last);
        last = now;
        if (now - t0 < 10000) requestAnimationFrame(probe);
        else {
          const applies = performance
            .getEntriesByName('applyWindow')
            .map((m) => Math.round(m.duration))
            .sort((a, b) => b - a)
            .slice(0, 5);
          res({ worst: Math.round(worst), longtasks, applies });
        }
      };
      requestAnimationFrame(probe);
    }),
);
console.log(JSON.stringify(result));
await browser.close();
