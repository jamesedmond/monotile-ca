// Census object viewer: one live flight panel shared by a row of tabs,
// one tab per object, with the selected object's certificate figures
// beneath. A page holds one viewer per kind of object, so the number of
// WebGL contexts is the number of kinds, not the number of objects.
// The selected object is mirrored in the URL fragment, so the index
// page can link to any object directly.
import { mountFlightPanel, type FlightHandle } from '../flight-panel';

export interface CensusObject {
  /** URL fragment and DOM id, e.g. 'hatA-441'. */
  key: string;
  /** Short tab label, e.g. '441'. */
  tab: string;
  /** Tab group label: tabs are grouped under their rule. */
  rule: string;
  title: string;
  /** One line under the title: what kind of object this is. */
  kind: string;
  /** One JSONL ResultRecord line; omit while the record is not prepared. */
  record?: string;
  downloadName?: string;
  reproduce?: string;
  flight?: { launchGens?: number; selectHeading?: number };
  /** Label/value pairs for the figures table (values may hold HTML). */
  facts: [string, string][];
  /** Paragraph of HTML under the figures. */
  note?: string;
}

export function mountCensusViewer(container: HTMLElement, objects: CensusObject[]): void {
  container.classList.add('cv');
  const rules = [...new Set(objects.map((o) => o.rule))];
  container.innerHTML = `
    <div class="cv-tabs" role="tablist">
      ${rules
        .map(
          (rule) => `<div class="cv-group"><span class="cv-rule">${rule}</span>${objects
            .filter((o) => o.rule === rule)
            .map(
              (o) =>
                `<button role="tab" id="${o.key}" data-key="${o.key}"${
                  o.record ? '' : ' disabled title="record not yet prepared"'
                }>${o.tab}</button>`,
            )
            .join('')}</div>`,
        )
        .join('')}
    </div>
    <div class="cv-flight"></div>
    <div class="cv-detail" role="tabpanel"></div>
  `;
  const detail = container.querySelector('.cv-detail') as HTMLElement;
  const flightBox = container.querySelector('.cv-flight') as HTMLElement;
  const tabs = [...container.querySelectorAll<HTMLButtonElement>('.cv-tabs button')];
  const flyable = objects.filter((o) => o.record);
  let handle: FlightHandle | null = null;
  let current = '';

  const flightOpts = (o: CensusObject) => ({
    record: o.record!,
    launchGens: o.flight?.launchGens,
    selectHeading: o.flight?.selectHeading,
    downloadName: o.downloadName,
    reproduce: o.reproduce,
  });

  const select = (key: string, fromUser: boolean): void => {
    const o = flyable.find((x) => x.key === key);
    if (!o || o.key === current) return;
    current = o.key;
    for (const t of tabs) t.setAttribute('aria-selected', String(t.dataset.key === o.key));
    detail.innerHTML =
      `<h3>${o.title}</h3><p class="cv-kind">${o.kind}</p>` +
      `<dl class="cv-facts">${o.facts.map(([k, v]) => `<div><dt>${k}</dt><dd>${v}</dd></div>`).join('')}</dl>` +
      (o.note ? `<p class="cv-note">${o.note}</p>` : '');
    if (handle) {
      handle.load(flightOpts(o));
    } else {
      handle = mountFlightPanel(flightBox, {
        ...flightOpts(o),
        speed: 8,
        autoplay: true,
        zoomLevel: 0.85,
        trail: 10,
      });
    }
    if (fromUser) history.replaceState(null, '', `#${o.key}`);
  };

  for (const t of tabs) t.addEventListener('click', () => select(t.dataset.key!, true));

  const fromHash = (): boolean => {
    const key = decodeURIComponent(location.hash.slice(1));
    if (!flyable.some((o) => o.key === key)) return false;
    select(key, false);
    container.scrollIntoView({ block: 'start' });
    return true;
  };
  window.addEventListener('hashchange', fromHash);
  if (!fromHash() && flyable.length > 0) select(flyable[0].key, false);
}
