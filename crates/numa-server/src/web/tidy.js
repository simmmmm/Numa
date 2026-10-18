// HOME-019: Tidy Up — library by library, and over your own photographs
// unless Everyone's is chosen. The page first lists the libraries that have
// something to tidy, as stacks; a library opens its cards of decisions, each
// a small set side by side, Numa's suggestion marked: exact copies, runs of
// the same frame, frames past use, and (Everyone's) photographs nobody is
// known to have taken. One tap keeps the best (the rest to their library's
// trash, 30 days), keeps all (never asked again), or tap the frames to keep
// by hand.
'use strict';

const KINDS = [
  ['copies', S.copiesTitle, S.copiesText],
  ['bursts', S.burstsTitle, S.burstsText],
  ['unusable', S.unusableTitle, S.unusableText],
  ['who', S.whoTitle, S.whoText],
];

// Whose photographs the page is over: the account's own, every time Tidy Up
// is opened. Everyone's rides along in the address while one moves about in
// Tidy Up (a library and back), and is gone when Tidy Up is opened afresh.
const everyoneIn = params => params.get('everyone') === '1';
const around = (params, more) => ({ everyone: everyoneIn(params) ? '1' : null, ...more });
const everyonesChip = (params, single) => !single && h('button', { class: 'chip everyones' + (everyoneIn(params) ? ' chosen' : ''),
  'aria-pressed': String(everyoneIn(params)), title: S.everyonesTip,
  onclick: () => go('/tidy', { library: params.get('library'), everyone: everyoneIn(params) ? null : '1' }) }, S.everyones);

// The entry on a library's page, beside For a Book…: an admin's, an
// account's with the right to delete, or its own Sent Home. For the rest it
// stays in sight, greyed, and says who can (P14), in a tooltip and, so that a
// touch screen and the keys reach it too, as a toast.
const tidyEntry = library => state.me.admin || state.me.manage || library.mine
  ? h('button', { class: 'chip', title: S.tidyTip, onclick: () => go('/tidy', { library: library.id }) }, S.tidyUp)
  : h('button', { class: 'chip', 'aria-disabled': 'true', title: S.tidyRefused, onclick: () => toast(S.tidyRefused) }, S.tidyUp);

async function tidyPage(main, params) {
  const library = params.get('library');
  const top = h('div', { class: 'top' }, h('div', {}, h('h1', {}, S.tidyUp),
    h('p', { class: 'body' }, S.tidyText)));
  const page = h('div', { class: 'import-page tidy-page' }, top, h('p', { class: 'body' }, S.loading));
  main.append(page);
  const query = new URLSearchParams(Object.entries({ library, everyone: everyoneIn(params) ? '1' : null }).filter(([, value]) => value)).toString();
  let answer;
  try { answer = await api('tidy' + (query && '?' + query), { wait: 60 }); } catch (error) { return page.replaceChildren(top, empty(S.tidyUp, error.message, true)); }
  if (!main.isConnected) return;
  const pending = answer.pending > 0 && h('p', { class: 'caption' }, phrase('pending', { n: grouped(answer.pending) }));
  page.replaceChildren(...(library ? tidyLibrary(pending, params, answer) : tidyLibraries(top, pending, params, answer)).filter(Boolean));
}

// The libraries with something to tidy, as stacks of the photographs in the
// sets, and how many of them.
function tidyLibraries(top, pending, params, answer) {
  const chips = everyonesChip(params, answer.single);
  const places = h('div', { class: 'places' }, answer.libraries.map(library => h('button', { class: 'place', onclick: () => go('/tidy', around(params, { library: library.id })) },
    prints(library.covers),
    h('div', {}, h('div', { class: 'name' }, libraryName(library)), h('div', { class: 'caption' }, phrase('toTidy', { photos: counted(library.photos) }))))));
  return [top, chips && h('div', { class: 'filters' }, chips), pending,
    answer.libraries.length ? places : empty(S.nothingToTidy, answer.everyone || answer.single ? S.nothingFoundTidy : S.nothingOfYours)];
}

// One library's cards: its name with the way back, the kinds as chips and
// Everyone's after them.
function tidyLibrary(pending, params, answer) {
  const kind = params.get('kind');
  const { library } = answer;
  titled(`${S.tidyUp} · ${libraryName(library)}`);
  const head = h('div', { class: 'collection-head' },
    h('button', { class: 'round small', 'aria-label': S.tidyUp, title: S.tidyUp, onclick: () => go('/tidy', around(params)) }, icon('back')),
    h('div', { class: 'titles' }, h('h1', {}, libraryName(library)), h('div', { class: 'caption' }, S.tidyUp)));
  const all = answer.cards.length;
  const chip = (key, label, count) => h('button', { class: 'chip' + (kind === key ? ' chosen' : ''), onclick: () => go('/tidy', around(params, { library: library.id, kind: key })) },
    label, h('span', { class: 'n' }, grouped(count)));
  const counts = h('div', { class: 'filters' }, chip(null, S.all, all),
    KINDS.filter(([key]) => answer.counts[key] > 0 || kind === key).map(([key, label]) => chip(key, label, answer.counts[key])), everyonesChip(params, answer.single));
  const shown = answer.cards.filter(card => !kind || card.kind === kind);
  const scope = { library: library.id, everyone: answer.everyone };
  const nothing = kind ? S.nothingKind : !library.may ? S.tidyRefused : answer.everyone || answer.single ? S.nothingFoundTidy : S.nothingOfYours;
  return [head, h('p', { class: 'body' }, S.tidyText), counts, pending,
    shown.length ? h('div', { class: 'sets' }, shown.map(card => tidyCard(card, answer.accounts, scope))) : empty(S.nothingToTidy, nothing)];
}

function tidyCard(card, accounts, scope) {
  const [, title, about] = KINDS.find(([key]) => key === card.kind);
  const keep = new Set(card.keep);
  const said = h('span', { class: 'said' });
  const box = h('section', { class: 'set ' + card.kind });
  const frames = h('div', { class: 'frames' });
  const section = { photos: card.photos.map(photo => ({ id: photo.id, rating: 0 })) };
  const act = h('div', { class: 'act' });
  const done = (text, action) => { toast(text, action); box.remove(); if (!document.querySelector('.set')) render(); };
  const send = async body => {
    const before = await trashIds();
    try {
      const answer = await api('tidy/keep', { method: 'POST', body: JSON.stringify({ key: card.key, ...scope, ...body }) });
      done(answer.trashed ? phrase('movedN', { photos: counted(answer.trashed) }) : S.kept, answer.trashed > 0 && await trashed(before));
    } catch (error) { said.textContent = error.message; }
  };
  const paint = () => {
    for (const frame of frames.children) frame.classList.toggle('kept', keep.has(Number(frame.dataset.id)));
    if (card.kind === 'who') return;
    const going = card.photos.length - keep.size;
    const suggested = keep.size === card.keep.length && card.keep.every(id => keep.has(id));
    act.replaceChildren(...[
      going > 0 && h('button', { class: 'chip go', onclick: () => send({ keep: [...keep] }) },
        going === card.photos.length ? (going === 1 ? S.moveToTrash : phrase('moveN', { n: going }))
          : suggested && card.keep.length ? S.keepBest : phrase('keepSome', { keep: keep.size, going })),
      h('button', { class: 'chip', onclick: () => send({ keep: card.photos.map(photo => photo.id) }) }, S.keepAll)].filter(Boolean));
  };
  card.photos.forEach((photo, at) => {
    const image = h('img', { src: thumb(photo.id, 512), loading: 'lazy', decoding: 'async', alt: '' });
    image.addEventListener('load', () => image.classList.add('in'));
    const best = card.keep.includes(photo.id);
    const notes = [...(photo.notes || []), photo.suggested != null && card.kind === 'bursts' && phrase('suggestedStars', { n: photo.suggested.toFixed(1) })].filter(Boolean);
    frames.append(h('figure', { class: 'frame', 'data-id': photo.id, style: { width: `calc(var(--h) * ${Math.min(2.5, Math.max(0.5, photo.aspect || 1.5))})` } },
      h('button', { class: 'pick', 'aria-label': card.kind === 'who' ? phrase('frameN', { n: at + 1, of: card.photos.length }) : phrase('keepOneN', { n: at + 1, of: card.photos.length }), 'aria-pressed': String(keep.has(photo.id)), onclick: event => {
        if (card.kind === 'who') return openFrame(section, at);
        if (keep.has(photo.id)) keep.delete(photo.id); else keep.add(photo.id);
        event.currentTarget.setAttribute('aria-pressed', String(keep.has(photo.id)));
        paint();
      } }, image, card.kind !== 'who' && h('span', { class: 'check', 'aria-hidden': 'true' }), best && h('span', { class: 'best' }, S.numasPick)),
      h('button', { class: 'round small look', 'aria-label': S.lookCloser, title: S.lookCloser, onclick: () => openFrame(section, at) }, icon('expand')),
      card.kind !== 'who' && h('figcaption', { class: 'caption' }, notes.length ? notes.join(' · ') : ' ', photo.place && h('span', { class: 'where' }, photo.place))));
  });
  if (card.kind === 'who') {
    act.append(...accounts.map(name => h('button', { class: 'chip go', onclick: async () => {
      try { await api('photos/by', { method: 'POST', body: JSON.stringify({ ids: card.ids, account: name }) }); done(phrase('byName', { name })); } catch (error) { said.textContent = error.message; }
    } }, name)), h('button', { class: 'chip', onclick: () => send({ keep: [] }) }, S.notSure));
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
