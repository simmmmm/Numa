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
  try { card = await api('card'); } catch (_) { card = null; }
  drawCard(bar, card);
  cardTimer = setTimeout(cardBar, card?.importing ? 1000 : 10000);
}

const shortDate = seconds => new Date(seconds * 1000).toLocaleString('en-GB', { day: 'numeric', month: 'short', year: 'numeric', timeZone: 'UTC' });
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
    // A meter of its own width and the numbers beside it in tabular
    // figures, so neither moves while the other changes.
    const { done, total } = card.importing;
    bar.replaceChildren(h('div', { class: 'line progress' }, h('span', {}, 'Importing from the card'),
      h('div', { class: 'meter', role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': total, 'aria-valuenow': done },
        h('div', { class: 'fill', style: { width: (total ? done / total : 0) * 100 + '%' } })),
      h('span', { class: 'numbers' }, `${grouped(done)} of ${grouped(total)}`)));
    return;
  }
  if (said) {
    bar.replaceChildren(...[h('div', { class: 'line' }, h('span', {}, said),
      h('button', { class: 'chip', onclick: () => { state.cardDismissed = said; bar.hidden = true; } }, 'Done')),
    card.done.problems.length > 0 && h('ul', { class: 'problems' }, card.done.problems.map(problem => h('li', {}, problem)))].filter(Boolean));
    return;
  }
  const from = card.camera ? `A card from the ${card.camera}` : 'A card';
  bar.replaceChildren(h('div', { class: 'line' },
    h('span', {}, `${from} · ${grouped(card.new)} new photograph${card.new === 1 ? '' : 's'} · ${days(card.first, card.last)}`),
    h('button', { class: 'primary', onclick: () => go('/import') }, 'Import…')));
}

// The Import page. Ticked: the photographs to copy, by their place on the
// card (`n`).
async function importPage(main) {
  let card, list;
  try { [card, list] = await Promise.all([api('card'), api('card/photos')]); } catch (error) { return main.append(empty('Import', error.message)); }
  if (!main.isConnected) return;
  if (!card || !list) return main.append(empty('No Card', "Put a camera's card in the NAS, or plug in its USB drive; it shows here."));
  if (card.importing) return main.append(empty('Importing', 'The bar above says how far it is.'));
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
    count.textContent = `${grouped(ticked.size)} to import`;
    start.disabled = ticked.size === 0;
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
  } }, 'Import');
  const tile = photo => {
    const image = h('img', { src: `/api/v1/card/thumb/${photo.n}?card=${encodeURIComponent(list.card || '')}&of=${photos.length}`, loading: 'lazy', decoding: 'async', alt: '' });
    image.addEventListener('load', () => image.classList.add('in'));
    const el = h('button', { class: 'tile' + (photo.new ? '' : ' already'), title: photo.name + (photo.new ? '' : ' · already in a library'), 'aria-label': photo.name,
      onclick: event => {
        const order = photos.map(one => one.n);
        const on = !ticked.has(photo.n);
        const span = event.shiftKey && anchor != null
          ? order.slice(Math.min(order.indexOf(anchor), order.indexOf(photo.n)), Math.max(order.indexOf(anchor), order.indexOf(photo.n)) + 1) : [photo.n];
        span.forEach(n => on ? ticked.add(n) : ticked.delete(n));
        anchor = photo.n;
        paint();
      } }, image, h('span', { class: 'check', 'aria-hidden': 'true' }), !photo.new && h('span', { class: 'badge' }, 'Imported'));
    tiles.set(photo.n, el);
    return el;
  };
  const rows = card.shoots.map((shoot, at) => {
    const own = list.shoots[at] || [];
    const suggested = shoot.folder.split('/').pop();
    const name = h('input', { type: 'text', value: suggested, 'aria-label': 'Name of the new folder' });
    const where = h('span', { class: 'caption' });
    const select = h('select', { 'aria-label': 'Where this shoot goes' },
      h('option', { value: '' }, 'A new folder'),
      archive.map(library => h('option', { value: library.id, selected: library.id === shoot.library }, library.name)));
    const update = () => {
      name.hidden = select.value !== '';
      const parent = shoot.folder.split('/').slice(0, -1).join('/');
      where.textContent = select.value ? '' : `into ${parent ? parent + '/' : ''}${name.value.trim() || suggested}`;
    };
    select.addEventListener('change', update);
    name.addEventListener('input', update);
    update();
    const box = h('input', { type: 'checkbox', 'aria-label': 'Every photograph of this shoot', onchange: () => {
      own.forEach(photo => box.checked ? ticked.add(photo.n) : ticked.delete(photo.n));
      paint();
    } });
    boxes.push({ box, shoot: own });
    const caption = shoot.count === shoot.photos ? `${grouped(shoot.count)} new` : `${grouped(shoot.count)} new of ${grouped(shoot.photos)}`;
    return { shoot: own, select, name, el: h('section', { class: 'shoot' },
      h('div', { class: 'head' }, h('label', { class: 'tick' }, box, h('span', { class: 'section-label' }, days(shoot.first, shoot.last))),
        h('span', { class: 'caption' }, caption), h('span', { class: 'to' }, select, name, where)),
      h('div', { class: 'grid' }, own.map(tile))) };
  });
  const from = card.camera ? `From the ${card.camera}` : 'From the card';
  main.append(h('div', { class: 'import-page' },
    h('div', { class: 'top' },
      h('div', {}, h('h1', {}, 'Import'), h('p', { class: 'body' }, `${from} · ${counted(card.photos)}, ${grouped(card.new)} new. Copied, read back and checked against the card; Numa never changes the card.`)),
      h('div', { class: 'act' }, count, start)),
    h('div', { class: 'filters' }, choice('All New', () => true), choice('Only Raws', photo => photo.raw), choice('Only JPEGs', photo => !photo.raw),
      h('button', { class: 'chip', onclick: () => { ticked.clear(); paint(); } }, 'None'), said),
    ...rows.map(row => row.el)));
  paint();
}
