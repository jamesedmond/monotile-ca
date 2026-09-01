// Shared essay navigation: a fixed side rail on wide screens, a
// collapsible contents block on narrow ones, and prev/next links at the
// foot of <main>. Every essay page calls mountNav(); the current
// section is inferred from the URL, and all links are relative so the
// site works unchanged under any host prefix.
import { SECTIONS } from './sections';
import './essay.css';

function currentSlug(): string {
  const m = location.pathname.match(/\/essay\/([^/]*)\/?/);
  return m?.[1] ?? '';
}

function href(fromSlug: string, toSlug: string): string {
  if (toSlug === '') return fromSlug === '' ? './' : '../';
  return (fromSlug === '' ? '' : '../') + `${toSlug}/`;
}

export function mountNav(): void {
  const slug = currentSlug();
  const index = SECTIONS.findIndex((s) => s.slug === slug);
  const main = document.querySelector('main');
  if (!main) return;

  const links = (): string =>
    SECTIONS.map((s, i) => {
      const current = s.slug === slug ? ' class="current"' : '';
      const soon = s.ready ? '' : ' <span class="soon">soon</span>';
      return `<a${current} href="${href(slug, s.slug)}" title="${s.blurb}">${i + 1}. ${s.title}${soon}</a>`;
    }).join('');

  const rail = document.createElement('nav');
  rail.className = 'rail';
  rail.innerHTML = `<div class="rail-head">Gliders on the<br/>Hat &amp; Spectre</div>${links()}`;
  document.body.prepend(rail);

  const toc = document.createElement('details');
  toc.className = 'toc';
  toc.innerHTML = `<summary>Contents</summary><div>${links()}</div>`;
  main.prepend(toc);

  const pager = document.createElement('div');
  pager.className = 'pager';
  const prev = index > 0 ? SECTIONS[index - 1] : null;
  const next = index >= 0 && index < SECTIONS.length - 1 ? SECTIONS[index + 1] : null;
  pager.innerHTML =
    (prev ? `<a class="prev" href="${href(slug, prev.slug)}">← ${prev.title}</a>` : '<span></span>') +
    (next ? `<a class="next" href="${href(slug, next.slug)}">${next.title} →</a>` : '<span></span>');
  main.append(pager);
}
