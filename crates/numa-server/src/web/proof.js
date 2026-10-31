// HOME-043: a proof gallery, as a client sees it from a link. Every picture
// arrives as a puzzle of shuffled tiles in one JPEG and is put back here on
// a canvas, from the seed its answer carries (X-Proof: seed width height
// tile). The order is the server's (proof.rs): mulberry32 and Fisher–Yates.
// The client chooses favourites with the heart; the photographer sees them.
'use strict';

const token = location.pathname.split('/')[2] || '';
const main = document.querySelector('main');
const ICONS = {
  heart: '<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>',
  close: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  back: '<path d="m15 18-6-6 6-6"/>',
  on: '<path d="m9 18 6-6-6-6"/>',
};

function h(tag, props, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props || {})) {
    if (key.startsWith('on')) el.addEventListener(key.slice(2), value);
    else if (value === true) el.setAttribute(key, '');
    else if (value != null && value !== false) el.setAttribute(key, value);
  }
  el.append(...children.flat().filter(child => child != null && child !== false));
  return el;
}
function icon(name) {
  const box = document.createElement('span');
  box.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${ICONS[name]}</svg>`;
  return box.firstChild;
}

function mulberry(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return (t ^ (t >>> 14)) >>> 0;
  };
}
function order(seed, tiles) {
  const where = Array.from({ length: tiles }, (_, i) => i);
  const next = mulberry(seed);
  for (let i = tiles - 1; i > 0; i--) {
    const j = Math.floor(next() * (i + 1) / 4294967296);
    [where[i], where[j]] = [where[j], where[i]];
  }
  return where;
}

const pause = ms => new Promise(done => setTimeout(done, ms));

// One proof onto `canvas`. Too many at once (429) waits and asks again.
async function draw(canvas, n, edge) {
  let answer;
  for (let tries = 0; tries < 6; tries++) {
    answer = await fetch(`/api/v1/s/${token}/${n}?edge=${edge}`);
    if (answer.status !== 429) break;
    await pause(1500 * (tries + 1));
  }
  if (!answer.ok) throw new Error(answer.status);
  const [seed, width, height, tile] = (answer.headers.get('X-Proof') || '').split(' ').map(Number);
  const puzzle = await createImageBitmap(await answer.blob());
  const columns = Math.ceil(width / tile);
  canvas.width = width;
  canvas.height = height;
  const g = canvas.getContext('2d');
  g.imageSmoothingEnabled = false;
  order(seed, columns * Math.ceil(height / tile)).forEach((to, from) => {
    g.drawImage(puzzle, (to % columns) * tile, Math.floor(to / columns) * tile, tile, tile,
      (from % columns) * tile, Math.floor(from / columns) * tile, tile, tile);
  });
  puzzle.close();
}

// A canvas of the photograph's shape until its picture comes.
const blank = aspect => h('canvas', { width: Math.round(aspect * 100), height: 100 });

let gallery;
const hearts = new Map();

function heart(photo, round) {
  const button = h('button', { class: 'round' + (photo.picked ? ' chosen' : ''), 'aria-pressed': String(photo.picked),
    'aria-label': S.proofFavourite, title: S.proofFavourite, onclick: event => { event.stopPropagation(); choose(photo); } }, icon('heart'));
  if (round) button.classList.add(round);
  if (!hearts.has(photo.n)) hearts.set(photo.n, new Set());
  hearts.get(photo.n).add(button);
  return button;
}

async function choose(photo) {
  photo.picked = !photo.picked;
  paintHearts(photo);
  const answer = await fetch(`/api/v1/s/${token}/${photo.n}/pick`, { method: 'PUT', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ picked: photo.picked }) }).catch(() => null);
  if (!answer?.ok) { photo.picked = !photo.picked; paintHearts(photo); }
  counted();
}
function paintHearts(photo) {
  hearts.get(photo.n)?.forEach(button => {
    if (!button.isConnected) return hearts.get(photo.n).delete(button);
    button.classList.toggle('chosen', photo.picked);
    button.setAttribute('aria-pressed', String(photo.picked));
  });
}

const tally = h('p', { class: 'caption' });
function counted() {
  const chosen = gallery.photos.filter(photo => photo.picked).length;
  const photos = `${gallery.photos.length} ${gallery.photos.length === 1 ? S.photo : S.photos}`;
  const until = new Date(gallery.expires * 1000).toLocaleDateString(S.locale, { day: 'numeric', month: 'long', year: 'numeric' });
  tally.textContent = [photos, phrase('proofChosen', { n: chosen }), phrase('proofUntil', { date: until })].join(' · ');
}

// Grid pictures come as they scroll into sight.
const sight = new IntersectionObserver(entries => entries.forEach(entry => {
  if (!entry.isIntersecting) return;
  sight.unobserve(entry.target);
  const canvas = entry.target.querySelector('canvas');
  draw(canvas, Number(entry.target.dataset.n), 640).catch(() => {});
}), { rootMargin: '400px' });

async function start() {
  const answer = await fetch(`/api/v1/s/${token}`).catch(() => null);
  if (!answer?.ok) {
    document.title = S.proofClosed;
    main.replaceChildren(h('h1', {}, S.proofClosed), h('p', { class: 'body' }, S.proofClosedText));
    return;
  }
  gallery = await answer.json();
  document.title = gallery.title;
  counted();
  const grid = h('div', { class: 'proofs' }, gallery.photos.map(photo => {
    const box = h('div', { class: 'proof', 'data-n': photo.n }, blank(photo.aspect),
      h('button', { class: 'open', 'aria-label': phrase('proofOpen', { n: photo.n + 1 }), onclick: () => loupe(photo.n) }), heart(photo));
    sight.observe(box);
    return box;
  }));
  main.replaceChildren(h('h1', {}, gallery.title), h('div', { class: 'by' }, gallery.by), tally,
    h('p', { class: 'body' }, S.proofHow), grid);
}

// The loupe: one photograph at 2048 px at most, ← and → to the next.
function loupe(at) {
  const box = h('dialog', { class: 'loupe', 'aria-label': gallery.title });
  const stage = h('div', { class: 'stage' });
  const where = h('div', { class: 'where caption' });
  const fav = h('div', { class: 'fav' });
  const show = n => {
    at = (n + gallery.photos.length) % gallery.photos.length;
    const photo = gallery.photos[at];
    const canvas = blank(photo.aspect);
    // Over the canvas, so a right click finds no picture to save.
    stage.replaceChildren(canvas, h('div', { class: 'shield' }));
    fav.replaceChildren(heart(photo));
    where.textContent = `${at + 1} / ${gallery.photos.length}`;
    draw(canvas, photo.n, 2048).catch(() => {});
  };
  const step = by => h('button', { class: 'round step ' + (by < 0 ? 'back' : 'on'), 'aria-label': by < 0 ? S.proofPrevious : S.proofNext,
    onclick: () => show(at + by) }, icon(by < 0 ? 'back' : 'on'));
  box.append(stage, h('button', { class: 'round close', 'aria-label': S.close, onclick: () => box.close() }, icon('close')),
    fav, step(-1), step(1), where);
  box.addEventListener('keydown', event => {
    if (event.key === 'ArrowLeft') show(at - 1);
    if (event.key === 'ArrowRight') show(at + 1);
  });
  box.addEventListener('close', () => box.remove());
  document.body.append(box);
  box.showModal();
  show(at);
}

// Saving a canvas from its menu would hand over the put-back picture; a
// screenshot still can, and the guide says so.
document.addEventListener('contextmenu', event => { if (event.target.closest('canvas, .proof, .stage')) event.preventDefault(); });
start();
