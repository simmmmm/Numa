// HOME-019: Tidy Up — cards of decisions, each a small set side by side,
// Numa's suggestion marked: exact copies, runs of the same frame, frames
// past use, and photographs nobody is known to have taken. One tap keeps
// the best (the rest to their library's trash, 30 days), keeps all (never
// asked again), or tap the frames to keep by hand.
'use strict';

const KINDS = [
  ['copies', 'Exact Copies', 'The same file more than once, byte for byte.'],
  ['bursts', 'The Same Frame', 'Three frames or more of one moment.'],
  ['unusable', 'Unusable', 'Blurred past use, nearly black or white, or blown out.'],
  ['who', 'Who Took These?', 'Photographs nobody in the house is known to have taken.'],
];

async function tidyPage(main, params) {
  const kind = params.get('kind');
  const top = h('div', { class: 'top' }, h('div', {}, h('h1', {}, 'Tidy Up'),
    h('p', { class: 'body' }, 'Photographs you may not need, in small sets. Nothing goes without your choice, and what goes waits in the trash for 30 days.')));
  const page = h('div', { class: 'import-page tidy-page' }, top, h('p', { class: 'body' }, 'Loading…'));
  main.append(page);
  let answer;
  try { answer = await api('tidy', { wait: 60 }); } catch (error) { return page.replaceChildren(top, empty('Tidy Up', error.message)); }
  if (!main.isConnected) return;
  const all = answer.cards.length;
  const chip = (key, label, count) => h('button', { class: 'chip' + (kind === key ? ' chosen' : ''), onclick: () => go('/tidy', { kind: key }) },
    label, h('span', { class: 'n' }, grouped(count)));
  const counts = h('div', { class: 'filters' }, chip(null, 'All', all),
    KINDS.filter(([key]) => answer.counts[key] > 0 || kind === key).map(([key, label]) => chip(key, label, answer.counts[key])));
  const shown = answer.cards.filter(card => !kind || card.kind === kind);
  const list = h('div', { class: 'sets' }, shown.map(card => tidyCard(card, answer.accounts)));
  page.replaceChildren(...[top, counts,
    answer.pending > 0 && h('p', { class: 'caption' }, `${grouped(answer.pending)} files of the same size are compared byte for byte tonight; exact copies among them show here tomorrow.`),
    shown.length ? list : empty('Nothing to Tidy', kind ? 'Nothing of this kind is waiting.' : 'Numa found nothing to choose. It looks again every night.')].filter(Boolean));
}

function tidyCard(card, accounts) {
  const [, title, about] = KINDS.find(([key]) => key === card.kind);
  const keep = new Set(card.keep);
  const said = h('span', { class: 'said' });
  const box = h('section', { class: 'set ' + card.kind });
  const frames = h('div', { class: 'frames' });
  const section = { photos: card.photos.map(photo => ({ id: photo.id, rating: 0 })) };
  const act = h('div', { class: 'act' });
  const done = text => { toast(text); box.remove(); if (!document.querySelector('.set')) render(); };
  const send = async body => {
    try {
      const answer = await api('tidy/keep', { method: 'POST', body: JSON.stringify({ key: card.key, ...body }) });
      done(answer.trashed ? `${counted(answer.trashed)} in the trash` : 'Kept; not asked again');
    } catch (error) { said.textContent = error.message; }
  };
  const paint = () => {
    for (const frame of frames.children) frame.classList.toggle('kept', keep.has(Number(frame.dataset.id)));
    if (card.kind === 'who') return;
    const going = card.photos.length - keep.size;
    const suggested = keep.size === card.keep.length && card.keep.every(id => keep.has(id));
    act.replaceChildren(...[
      going > 0 && h('button', { class: 'chip go', onclick: () => send({ keep: [...keep] }) },
        going === card.photos.length ? `Delete ${going === 1 ? 'It' : 'All ' + going}`
          : suggested && card.keep.length ? 'Keep Best' : `Keep ${keep.size}, Delete ${going}`),
      h('button', { class: 'chip', onclick: () => send({ keep: card.photos.map(photo => photo.id) }) }, 'Keep All')].filter(Boolean));
  };
  card.photos.forEach((photo, at) => {
    const image = h('img', { src: thumb(photo.id, 512), loading: 'lazy', decoding: 'async', alt: '' });
    image.addEventListener('load', () => image.classList.add('in'));
    const best = card.keep.includes(photo.id);
    const notes = [...(photo.notes || []), photo.suggested != null && card.kind === 'bursts' && `~${photo.suggested.toFixed(1)}★`].filter(Boolean);
    frames.append(h('figure', { class: 'frame', 'data-id': photo.id, style: { width: `calc(var(--h) * ${Math.min(2.5, Math.max(0.5, photo.aspect || 1.5))})` } },
      h('button', { class: 'pick', 'aria-label': card.kind === 'who' ? 'Photograph' : 'Keep this one', 'aria-pressed': String(keep.has(photo.id)), onclick: event => {
        if (card.kind === 'who') return openFrame(section, at);
        if (keep.has(photo.id)) keep.delete(photo.id); else keep.add(photo.id);
        event.currentTarget.setAttribute('aria-pressed', String(keep.has(photo.id)));
        paint();
      } }, image, card.kind !== 'who' && h('span', { class: 'check', 'aria-hidden': 'true' }), best && h('span', { class: 'best' }, "Numa's Pick")),
      h('button', { class: 'round small look', 'aria-label': 'Look closer', title: 'Look closer', onclick: () => openFrame(section, at) }, icon('expand')),
      card.kind !== 'who' && h('figcaption', { class: 'caption' }, notes.length ? notes.join(' · ') : ' ', photo.place && h('span', { class: 'where' }, photo.place))));
  });
  if (card.kind === 'who') {
    act.append(...accounts.map(name => h('button', { class: 'chip go', onclick: async () => {
      try { await api('photos/by', { method: 'POST', body: JSON.stringify({ ids: card.ids, account: name }) }); done(`By ${name}`); } catch (error) { said.textContent = error.message; }
    } }, name)), h('button', { class: 'chip', onclick: () => send({ keep: [] }) }, 'Not Sure'));
  }
  box.append(
    h('div', { class: 'head' }, h('div', { class: 'section-label' }, card.kind === 'who' ? `${title} · ${card.title}` : `${title} · ${card.photos.length}`),
      h('p', { class: 'body' }, card.why || about)),
    frames, h('div', { class: 'foot' }, act, said));
  paint();
  return box;
}

// The viewer over the set, as on the timeline: arrows, zoom, Info.
function openFrame(section, at) {
  state.sections = [section];
  openViewer(section, at);
}
