// HOME-009: a card in the NAS. The bar on every page — what is new on it,
// the import's progress and verdict — and the Import page: the card's own
// previews by shoot, the new ones ticked, the ones already in a library
// dimmed, and where each shoot goes.
'use strict';

let cardTimer = null;
async function cardBar() {
  clearTimeout(cardTimer);
  const bar = document.querySelector('.cardbar');
  if (!bar || !state.me) return;
  let card = null;
  try { card = await api('card', { quiet: true }); } catch (_) { card = null; }
  drawCard(bar, card);
  cardTimer = setTimeout(cardBar, card?.importing ? 1000 : 10000);
}

const shortDate = seconds => new Date(seconds * 1000).toLocaleString(S.locale, { day: 'numeric', month: 'short', year: 'numeric', timeZone: 'UTC' });
function days(first, last) {
  const [a, b] = [shortDate(first), shortDate(last)];
  return a === b ? a : `${a.replace(/ \d{4}$/, '')} – ${b}`;
}

function drawCard(bar, card) {
  const said = card?.done?.said;
  const onPage = route().path === '/import';
  if (!card || (said && state.cardDismissed === said) || (!card.importing && !said && (!card.new || onPage))) {
    bar.hidden = true;
    return;
  }
  bar.hidden = false;
  if (card.importing) {
    bar.replaceChildren(progressLine(S.importingCard, card.importing));
    return;
  }
  if (said) {
    bar.replaceChildren(...[h('div', { class: 'line' }, h('span', {}, said),
      h('button', { class: 'chip', onclick: () => { state.cardDismissed = said; bar.hidden = true; } }, S.done)),
    card.done.problems.length > 0 && h('ul', { class: 'problems' }, card.done.problems.map(problem => h('li', {}, problem)))].filter(Boolean));
    return;
  }
  const from = card.camera ? phrase('aCardFrom', { camera: card.camera }) : S.aCard;
  bar.replaceChildren(h('div', { class: 'line' },
    h('span', {}, phrase('cardLine', { from, n: grouped(card.new), unit: card.new === 1 ? S.newPhoto : S.newPhotos, days: days(card.first, card.last) })),
    h('button', { class: 'primary', onclick: () => go('/import') }, S.importEll)));
}

// The Import page. Ticked: the photographs to copy, by their place on the
// card (`n`).
async function importPage(main) {
  let card, list;
  try { [card, list] = await Promise.all([api('card'), api('card/photos')]); } catch (error) { return main.append(empty(S.importPage, error.message, true)); }
  if (!main.isConnected) return;
  if (!card || !list) return main.append(empty(S.noCard, S.noCardText));
  if (card.importing) return main.append(empty(S.importing, S.importingText));
  const archive = (await libraries().catch(() => [])).filter(library => !library.mine);
  const photos = list.shoots.flat();
  const ticked = new Set(photos.filter(photo => photo.new).map(photo => photo.n));
  const tiles = new Map();
  let anchor = null;
  const count = h('span', { class: 'caption count' });
  const said = h('span', { class: 'said' });
  const boxes = [];
  const paint = () => {
    for (const [n, tile] of tiles) tile.classList.toggle('ticked', ticked.has(n));
    for (const { box, shoot } of boxes) {
      const on = shoot.filter(photo => ticked.has(photo.n)).length;
      box.checked = on === shoot.length;
      box.indeterminate = on > 0 && on < shoot.length;
    }
    count.textContent = phrase('toImport', { n: grouped(ticked.size) });
    start.disabled = ticked.size === 0;
    start.textContent = ticked.size ? (ticked.size === 1 ? S.import1 : phrase('importN', { n: grouped(ticked.size) })) : S.importPage;
  };
  const only = keep => { ticked.clear(); photos.filter(photo => photo.new && keep(photo)).forEach(photo => ticked.add(photo.n)); paint(); };
  const choice = (label, keep) => h('button', { class: 'chip', onclick: () => only(keep) }, label);
  const start = h('button', { class: 'primary', onclick: async () => {
    const shoots = rows.map(row => ({
      ...(row.select.value ? { library: Number(row.select.value) } : { name: row.name.value.trim() }),
      photos: row.shoot.map(photo => photo.n).filter(n => ticked.has(n)),
    }));
    start.disabled = true;
    try {
      await api('card/import', { method: 'POST', body: JSON.stringify({ shoots }) });
      go('/');
    } catch (error) { said.textContent = error.message; start.disabled = false; }
  } }, S.importPage);
  const tile = photo => {
    const image = h('img', { src: `/api/v1/card/thumb/${photo.n}?card=${encodeURIComponent(list.card || '')}&of=${photos.length}`, loading: 'lazy', decoding: 'async', alt: '' });
    image.addEventListener('load', () => image.classList.add('in'));
    const el = h('button', { class: 'tile' + (photo.new ? '' : ' already'), title: photo.name + (photo.new ? '' : S.alreadyTip), 'aria-label': photo.name,
      onclick: event => {
        const order = photos.map(one => one.n);
        const on = !ticked.has(photo.n);
        const span = event.shiftKey && anchor != null
          ? order.slice(Math.min(order.indexOf(anchor), order.indexOf(photo.n)), Math.max(order.indexOf(anchor), order.indexOf(photo.n)) + 1) : [photo.n];
        span.forEach(n => on ? ticked.add(n) : ticked.delete(n));
        anchor = photo.n;
        paint();
      } }, image, h('span', { class: 'check', 'aria-hidden': 'true' }), !photo.new && h('span', { class: 'badge' }, S.imported));
    tiles.set(photo.n, el);
    return el;
  };
  // Asked once for every shoot: reading the card's GPS takes a moment, and
  // the page does not wait for it.
  const named = api('card/names', { wait: 180, quiet: true }).catch(() => null);
  const rows = card.shoots.map((shoot, at) => {
    const own = list.shoots[at] || [];
    const suggested = shoot.folder.split('/').pop();
    const name = h('input', { type: 'text', value: suggested, 'aria-label': S.newFolderName });
    const where = h('span', { class: 'caption' });
    const select = h('select', { 'aria-label': S.whereShoot },
      h('option', { value: '' }, S.newFolderOption),
      archive.map(library => h('option', { value: library.id, selected: library.id === shoot.library }, library.name)));
    const update = () => {
      name.hidden = select.value !== '';
      const parent = shoot.folder.split('/').slice(0, -1).join('/');
      where.textContent = select.value ? '' : phrase('intoWhere', { where: `${parent ? parent + '/' : ''}${name.value.trim() || suggested}` });
    };
    select.addEventListener('change', update);
    // A name chosen (typed, or one of the ideas below) is a new folder's.
    name.addEventListener('input', () => { select.value = ''; update(); });
    update();
    // HOME-029: a name from where it was taken, by its GPS: the first is
    // filled in while the date still stands, the others offered.
    const ideas = nameIdeas(named.then(all => {
      const answer = all?.[at];
      if (answer?.names?.length && name.value === suggested) {
        name.value = answer.names[0];
        update();
      }
      return answer;
    }), name);
    const box = h('input', { type: 'checkbox', 'aria-label': S.everyPhoto, onchange: () => {
      own.forEach(photo => box.checked ? ticked.add(photo.n) : ticked.delete(photo.n));
      paint();
    } });
    boxes.push({ box, shoot: own });
    const caption = shoot.count === shoot.photos ? phrase('newCount', { n: grouped(shoot.count) }) : phrase('newOf', { n: grouped(shoot.count), of: grouped(shoot.photos) });
    return { shoot: own, select, name, el: h('section', { class: 'shoot' },
      h('div', { class: 'head' }, h('label', { class: 'tick' }, box, h('span', { class: 'section-label' }, days(shoot.first, shoot.last))),
        h('span', { class: 'caption' }, caption), h('span', { class: 'to' }, select, name, where)),
      ideas,
      h('div', { class: 'grid' }, own.map(tile))) };
  });
  const from = card.camera ? phrase('fromCamera', { camera: card.camera }) : S.fromCard;
  main.append(h('div', { class: 'import-page' },
    h('div', { class: 'top' },
      h('div', {}, h('h1', {}, S.importPage), h('p', { class: 'body' }, phrase('importSummary', { from, photos: counted(card.photos), n: grouped(card.new) }))),
      h('div', { class: 'act' }, count, start)),
    h('div', { class: 'filters' }, choice(S.allNew, () => true), choice(S.onlyRaws, photo => photo.raw), choice(S.onlyJpegs, photo => !photo.raw),
      h('button', { class: 'chip', onclick: () => { ticked.clear(); paint(); } }, S.none), said),
    ...rows.map(row => row.el)));
  paint();
}

// A meter of its own width and the numbers beside it in tabular figures, so
// neither moves while the other changes.
function progressLine(word, { done, total }) {
  return h('div', { class: 'line progress' }, h('span', {}, word),
    h('div', { class: 'meter', role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': total, 'aria-valuenow': done },
      h('div', { class: 'fill', style: filled(total ? done / total : 0) })),
    h('span', { class: 'numbers' }, phrase('ofTotal', { done: grouped(done), total: grouped(total) })));
}

// HOME-052: Send Home from the browser. Files chosen with Add Photos… or
// dropped anywhere on the page go into the account's own Sent Home, by the
// day each was taken, one at a time; a bar under the header counts them and
// a toast says how it went. The same bytes sent twice are kept once (the
// server's), so sending a folder again is safe.
const sending = { queue: [], done: 0, total: 0, created: 0, already: 0, failed: [] };
// Made on first use: `h` is app.js's, which loads after this file.
const sendBar = () => sending.bar ??= h('div', { class: 'cardbar', hidden: true });

function addPhotos() {
  sending.picker ??= document.body.appendChild(h('input', { type: 'file', multiple: true, hidden: true, onchange: () => {
    sendHome([...sending.picker.files]);
    sending.picker.value = '';
  } }));
  sending.picker.click();
}

async function sendHome(files) {
  if (!files.length) return;
  sending.queue.push(...files);
  sending.total += files.length;
  // A round already going takes these along.
  if (sending.total > files.length) return drawSending();
  while (sending.queue.length) {
    drawSending();
    const file = sending.queue.shift();
    try {
      const answer = await api('send/' + encodeURIComponent(file.name), { method: 'PUT', body: file, quiet: true,
        headers: { 'Content-Type': 'application/octet-stream', 'X-Numa-Taken': String(Math.round((file.lastModified || Date.now()) / 1000)) } });
      sending[answer.duplicate ? 'already' : 'created'] += 1;
    } catch (error) { sending.failed.push(error.message); }
    sending.done += 1;
  }
  const { created, already, failed } = sending;
  Object.assign(sending, { done: 0, total: 0, created: 0, already: 0, failed: [] });
  drawSending();
  state.libraries = null;
  const said = [created && phrase('sentHomeN', { photos: counted(created) }), already && phrase('thereAlready', { photos: counted(already) }),
    failed.length && phrase('notSent', { n: grouped(failed.length), error: failed[0] })].filter(Boolean).join(' ');
  const mine = created ? (await libraries().catch(() => [])).find(library => library.mine) : null;
  const here = () => route().path === '/' && route().params.get('library') === String(mine.id);
  toast(said, mine ? { label: S.open, run: () => here() ? render() : go('/', { library: mine.id }) } : null);
}

function drawSending() {
  const bar = sendBar();
  bar.hidden = !sending.total;
  if (sending.total) bar.replaceChildren(progressLine(S.sendingHome, sending));
}

// Files dragged in from outside show where they go over the whole window. A
// drag that starts on the page (a stack's print) is not one: Chrome gives it
// a file too. Listened to on the window, after manage.js has said no to a tile.
let dragFromPage = false;
window.addEventListener('dragstart', event => { dragFromPage = !event.defaultPrevented; });
window.addEventListener('dragend', () => { dragFromPage = false; });
const dropZone = () => sending.zone ??= document.body.appendChild(h('div', { class: 'dropzone', hidden: true,
  ondragleave: () => { sending.zone.hidden = true; } },
h('div', { class: 'empty' }, h('h2', {}, S.sendHome), h('p', { class: 'body' }, S.dropText))));
const carriesFiles = event => state.me && !dragFromPage && event.dataTransfer?.types.includes('Files');
document.addEventListener('dragover', event => {
  if (!carriesFiles(event)) return;
  event.preventDefault();
  event.dataTransfer.dropEffect = 'copy';
  dropZone().hidden = false;
});
document.addEventListener('drop', event => {
  if (!carriesFiles(event)) return;
  event.preventDefault();
  dropZone().hidden = true;
  // Asked for now: the drop's items are gone once this returns.
  const entries = [...event.dataTransfer.items].map(item => item.webkitGetAsEntry?.()).filter(Boolean);
  const files = [...event.dataTransfer.files];
  (entries.length ? filesIn(entries) : Promise.resolve(files)).then(sendHome, error => toast(error.message));
});
window.addEventListener('beforeunload', event => { if (sending.total) event.preventDefault(); });

// A dropped folder is walked: every file in it and in its folders, nothing
// hidden (a Mac's .DS_Store, a library's own .numa).
async function filesIn(entries) {
  const files = [];
  const asked = (call, ...on) => new Promise((ok, no) => call.call(...on, ok, no));
  async function walk(entry) {
    if (entry.name.startsWith('.')) return;
    if (entry.isFile) return files.push(await asked(entry.file, entry));
    const reader = entry.createReader();
    for (let batch; (batch = await asked(reader.readEntries, reader)).length;) for (const one of batch) await walk(one);
  }
  for (const entry of entries) await walk(entry);
  return files;
}
