// HOME-016/017: managing the archive from the pages. Photographs chosen on
// the grid as on the desktop — a click once something is chosen, shift for
// a run, Ctrl or ⌘ to start, Select on a phone — and what can be done to
// them; new library folders, renaming one; the trash. HOME-021: libraries
// chosen on their page the same way, and what can be done to them.
'use strict';

const picked = { ids: new Set(), anchor: null, mode: false, album: null };
const libraryOf = id => Math.floor(id / 2 ** 32);

// Whether the click chose (true) rather than opened.
function pick(event, id) {
  // The click a long press leaves behind: the press chose already.
  if (event.currentTarget?.dataset.held) { delete event.currentTarget.dataset.held; return true; }
  // The click that ends a drag across the photographs chose already.
  if (picked.swallow) return true;
  const choosing = picked.mode || picked.ids.size > 0 || event.shiftKey || event.ctrlKey || event.metaKey || event.target.closest?.('.check');
  if (!choosing) return false;
  const order = state.sections.flatMap(section => section.photos || []).map(photo => photo.id);
  if (event.shiftKey && picked.anchor != null && order.includes(picked.anchor)) {
    const [a, b] = [order.indexOf(picked.anchor), order.indexOf(id)].sort((x, y) => x - y);
    order.slice(a, b + 1).forEach(one => picked.ids.add(one));
  } else {
    if (picked.ids.has(id)) picked.ids.delete(id); else picked.ids.add(id);
    picked.anchor = id;
  }
  paintPicked();
  return true;
}

// On a touch screen, a finger held on a photograph chooses it, as in the
// phones' own Photos; a finger that moves is scrolling and lets go. With a
// mouse, the right button opens Numa's menu for it (`chosenMenu`).
function holdToChoose(el, id) {
  let timer = null, from = null, touch = false;
  const cancel = () => { clearTimeout(timer); timer = null; };
  el.addEventListener('pointerdown', event => {
    touch = event.pointerType !== 'mouse';
    if (!touch) return;
    from = { x: event.clientX, y: event.clientY };
    timer = setTimeout(() => {
      timer = null;
      el.dataset.held = '1';
      picked.ids.add(id);
      picked.anchor = id;
      paintPicked();
      navigator.vibrate?.(10);
    }, 450);
  });
  el.addEventListener('pointermove', event => { if (timer && Math.hypot(event.clientX - from.x, event.clientY - from.y) > 10) cancel(); });
  el.addEventListener('pointerup', cancel);
  el.addEventListener('pointercancel', cancel);
  el.addEventListener('contextmenu', event => touch ? event.preventDefault() : chosenMenu(event, el, id));
  return el;
}

// A drag with the mouse across the photographs chooses what it covers, as
// in Numa; with Ctrl, ⌘ or Shift held it adds to what was chosen. Near the
// top or the foot of the window the page scrolls along.
document.addEventListener('pointerdown', down => {
  if (down.pointerType !== 'mouse' || down.button !== 0 || state.viewer || !down.target.closest?.('main .month, main > .grid')) return;
  const page = event => ({ x: event.clientX + scrollX, y: event.clientY + scrollY });
  const from = page(down);
  const kept = down.ctrlKey || down.metaKey || down.shiftKey ? new Set(picked.ids) : new Set();
  let box = null, last = down, frame = 0;
  const cover = () => {
    const to = page(last);
    const [left, right, top, bottom] = [Math.min(from.x, to.x), Math.max(from.x, to.x), Math.min(from.y, to.y), Math.max(from.y, to.y)];
    Object.assign(box.style, { left: left - scrollX + 'px', top: top - scrollY + 'px', width: right - left + 'px', height: bottom - top + 'px' });
    picked.ids = new Set(kept);
    for (const tile of document.querySelectorAll('main .tile[data-id]')) {
      const r = tile.getBoundingClientRect();
      const inside = r.right + scrollX > left && r.left + scrollX < right && r.bottom + scrollY > top && r.top + scrollY < bottom;
      if (inside) picked.ids.add(Number(tile.dataset.id));
      tile.classList.toggle('picked', picked.ids.has(Number(tile.dataset.id)));
    }
  };
  const along = () => {
    const edge = last.clientY < 100 ? -1 : last.clientY > innerHeight - 60 ? 1 : 0;
    if (edge) { scrollBy(0, edge * 16); cover(); }
    frame = requestAnimationFrame(along);
  };
  const move = event => {
    last = event;
    if (!box) {
      if (Math.hypot(event.clientX - down.clientX, event.clientY - down.clientY) < 6) return;
      box = h('div', { class: 'marquee' });
      document.body.append(box);
      document.body.classList.add('dragging', 'picking');
      frame = requestAnimationFrame(along);
    }
    cover();
  };
  const up = () => {
    removeEventListener('pointermove', move);
    removeEventListener('pointerup', up);
    if (!box) return;
    cancelAnimationFrame(frame);
    box.remove();
    document.body.classList.remove('dragging');
    picked.swallow = true;
    setTimeout(() => { picked.swallow = false; });
    paintPicked();
  };
  addEventListener('pointermove', move);
  addEventListener('pointerup', up);
});
// A photograph is not dragged out of the page as a picture: the drag chooses.
document.addEventListener('dragstart', event => { if (event.target.closest?.('main .tile')) event.preventDefault(); });

function unpick() {
  picked.ids.clear();
  picked.anchor = null;
  picked.mode = false;
  picked.album = null;
  paintPicked();
}

function paintPicked() {
  for (const tile of document.querySelectorAll('.tile[data-id]')) tile.classList.toggle('picked', picked.ids.has(Number(tile.dataset.id)));
  document.body.classList.toggle('picking', picked.mode || picked.ids.size > 0);
  document.querySelectorAll('.select-toggle').forEach(chip => chip.classList.toggle('chosen', picked.mode));
  selectionBar();
}

// Every photograph of the page, the months not yet asked for included, which
// is the chip in the bar and Ctrl or ⌘ + A.
async function selectAll() {
  await Promise.allSettled(state.sections.map(section => section.load?.()));
  state.sections.flatMap(section => section.photos || []).forEach(photo => picked.ids.add(photo.id));
  paintPicked();
}

function selectToggle() {
  return h('button', { class: 'chip select-toggle' + (picked.mode ? ' chosen' : ''), onclick: () => {
    if (picked.mode) unpick(); else { picked.mode = true; paintPicked(); }
  } }, S.select);
}

// What may change these: an admin anything, an account its own Sent Home.
async function mayManage(ids) {
  if (state.me.admin || state.me.manage) return true;
  const mine = (await libraries().catch(() => [])).filter(library => library.mine).map(library => library.id);
  return [...ids].every(id => mine.includes(libraryOf(id)));
}

// What can be done to these photographs, as menu items: the bar's star and
// ⋯ menus, and the right-click menu, are made of the same ones.
function chosenActions(ids, manage, own = {}) {
  const n = ids.length;
  const send = async (path, body, after) => {
    try { const answer = await api(path, { method: 'POST', body: JSON.stringify({ ids, ...body }) }); after?.(answer); } catch (error) { toast(error.message); }
  };
  const refresh = () => { unpick(); state.libraries = null; refreshAround(ids); };
  const marks = body => send('photos/marks', body, () => {
    for (const section of state.sections) section.photos?.filter(photo => ids.includes(photo.id)).forEach(photo => { if ('rating' in body) photo.rating = body.rating; });
    remark(ids);
    toast(phrase('saidCount', { said: markSaid(body), photos: counted(n) }));
  });
  // Pick and Reject are toggles, as P and X are: a flag every one chosen has
  // already is taken off, else it is set on all of them. The flags they had
  // are kept for Undo.
  const flag = which => {
    const listed = new Map(state.sections.flatMap(section => section.photos || []).map(photo => [photo.id, photo]));
    const had = ids.map(id => listed.get(id)?.flag);
    const now = which !== 'none' && had.some(one => one !== which) ? which : 'none';
    send('photos/marks', { flag: now }, () => {
      ids.forEach(id => { const photo = listed.get(id); if (photo) photo.flag = now; });
      remark(ids);
      toast(phrase('saidCount', { said: markSaid({ flag: now }), photos: counted(n) }), had.every(Boolean) && { label: S.undo, run: () => putFlagsBack(ids, had) });
    });
  };
  const item = (label, onclick, title) => h('button', { onclick, title }, label);
  // The loupe has its own (`own.erase`): it goes on to the next photograph.
  const erase = own.erase || (async () => {
    if (!await ask(phrase('trashQN', { photos: counted(n) }), S.trashTextN, n === 1 ? S.moveToTrash : phrase('moveN', { n: grouped(n) }))) return;
    const before = await trashIds();
    send('photos/delete', {}, async answer => { toast(phrase('movedN', { photos: counted(answer.trashed) }), await trashed(before)); refresh(); });
  });
  // HOME-033: the raws among them, which may be kept as JPEGs instead.
  const raws = ids.filter(id => state.sections.some(section => section.photos?.some(photo => photo.id === id && photo.raw)));
  const keepJpegs = async () => {
    const m = raws.length;
    if (!await ask(m === 1 ? S.keepRawQ : phrase('keepRawsQ', { n: grouped(m) }),
      S.keepText,
      m === 1 ? S.deleteRaw : S.deleteRaws)) return;
    send('photos/keep-jpeg', { ids: raws }, answer => {
      unpick();
      if (state.viewer) leaveViewer();
      toast(phrase(answer.raws === 1 ? 'keepingOne' : 'keepingMany', { n: grouped(answer.raws) }));
      waitKept(raws);
    });
  };
  return {
    erase: manage ? erase : null,
    album: () => item(S.albumEll, () => albumDialog(ids)),
    downloads: () => downloadItems(`/api/v1/photos/zip?ids=${ids.join(',')}`, `/api/v1/photos/zip?ids=${ids.join(',')}&as=jpeg`),
    marks: () => [
      h('div', { class: 'section-label' }, S.starsLabel),
      h('div', { class: 'star-row' }, [1, 2, 3, 4, 5].map(stars => h('button', { title: `${stars} (${stars})`, 'aria-label': starsOf(stars), onclick: () => marks({ rating: stars }) },
        Array.from({ length: stars }, () => icon('star'))))),
      item(S.noRating, () => marks({ rating: 0 }), '0'),
      h('div', { class: 'section-label' }, S.flagLabel),
      Object.entries(FLAGS).map(([which, label]) => item(label, () => flag(which), { pick: 'P', reject: 'X', none: 'U' }[which])),
    ],
    more: () => [
      item(S.takenBy, () => byDialog(ids)),
      item(S.forABook, () => bookDialog(`ids=${ids.join(',')}`)),
      manage && h('div', { class: 'rule' }),
      manage && item(S.moveEll, () => moveDialog(ids, refresh)),
      manage && item(S.removeFromLibraryEll, async () => {
        if (await ask(phrase('removeQ', { photos: counted(n) }), S.removeText, S.removeFromLibrary))
          send('photos/remove', {}, refresh);
      }),
      manage && item(S.moveToTrashEll, erase, phrase('moveTip', { key: DELETE_KEY })),
      manage && raws.length > 0 && item(raws.length === 1 ? S.deleteRawEll : S.deleteRawsEll, keepJpegs, S.keepJpegTip),
    ].filter(Boolean),
  };
}

// What Undo does to a flag chosen on several: each goes back to the flag it had.
async function putFlagsBack(ids, had) {
  try {
    for (const value of new Set(had)) await api('photos/marks', { method: 'POST', body: JSON.stringify({ ids: ids.filter((_, at) => had[at] === value), flag: value }) });
  } catch (error) { return toast(error.message); }
  const back = new Map(ids.map((id, at) => [id, had[at]]));
  for (const section of state.sections) section.photos?.forEach(photo => { if (back.has(photo.id)) photo.flag = back.get(photo.id); });
  remark(ids);
  toast(S.flagsBack);
}

// HOME-033: the raws being kept as JPEGs, followed until the last is done,
// then their months shown again.
async function waitKept(ids) {
  for (;;) {
    await new Promise(done => setTimeout(done, 3000));
    const { left } = await api('photos/keep-jpeg', { quiet: true }).catch(() => ({ left: 0 }));
    if (!left) break;
  }
  state.libraries = null;
  await refreshAround(ids);
  toast(phrase('keptAs', { photos: counted(ids.length) }));
}

async function selectionBar() {
  document.querySelector('.selection')?.remove();
  picked.erase = null;
  if (!picked.ids.size && !picked.mode) return;
  const ids = [...picked.ids];
  const n = ids.length;
  const bar = h('div', { class: 'selection', role: 'toolbar', 'aria-label': S.chosenPhotos });
  document.body.append(bar);
  // The few that are done most in sight; the rest under More, so the bar
  // holds on a phone's width too.
  const close = h('button', { class: 'round small', 'aria-label': S.clearChoice, title: S.clearTip, onclick: unpick }, icon('close'));
  const round = (name, label, items) => h('button', { class: 'round small', 'aria-label': label, title: label, onclick: event => {
    event.stopPropagation();
    menuOver(event.currentTarget, items());
  } }, icon(name));
  // Select All is in the bar, where it can be seen, not only under a key (P15).
  const everything = h('button', { class: 'chip', onclick: selectAll }, S.selectAll);
  if (!n) {
    bar.append(h('div', { class: 'lead' }, close, h('span', { class: 'count' }, picked.album ? phrase('choosePhotosFor', { album: picked.album }) : S.choosePhotos), everything));
    return;
  }
  const manage = await mayManage(ids);
  if (!bar.isConnected) return;
  const actions = chosenActions(ids, manage);
  picked.erase = actions.erase;
  // Two groups, so a phone's bar breaks between them and not in the middle of one.
  bar.append(h('div', { class: 'lead' }, close, h('span', { class: 'count' }, phrase('chosenN', { n: grouped(n) })), everything),
    h('div', { class: 'acts' },
      picked.album ? h('button', { class: 'chip add', onclick: () => intoNewAlbum(ids, picked.album) }, phrase('addTo', { album: picked.album }))
        : h('button', { class: 'chip', onclick: () => albumDialog(ids) }, S.albumEll),
      round('star', S.starsAndFlag, actions.marks),
      round('download', S.downloadTip, actions.downloads),
      round('more', S.more, actions.more)));
}

// Right-click on a photograph: it is chosen (alone, unless it already was),
// and Numa's own menu opens where the pointer is.
async function chosenMenu(event, tile, id) {
  event.preventDefault();
  if (!picked.ids.has(id)) {
    picked.ids.clear();
    picked.ids.add(id);
    picked.anchor = id;
    paintPicked();
  }
  const ids = [...picked.ids];
  const actions = chosenActions(ids, await mayManage(ids));
  menuAt(event.clientX, event.clientY, [
    ids.length === 1 ? h('button', { onclick: () => { unpick(); tile.click(); } }, S.open) : h('div', { class: 'section-label' }, phrase('chosenN', { n: grouped(ids.length) })),
    actions.album(),
    h('div', { class: 'rule' }), h('div', { class: 'section-label' }, S.download), ...actions.downloads(),
    h('div', { class: 'rule' }), actions.marks(),
    h('div', { class: 'rule' }), actions.more(),
  ]);
}

// After a change to these photographs, the months that held them asked
// again in place, so the page stays where it was; the search anew.
async function refreshAround(ids) {
  const touched = state.sections.filter(section => section.el && section.photos?.some(photo => ids.includes(photo.id)));
  if (!touched.length) {
    // A page without months of its own (a search, Today) is asked again whole;
    // with a photograph open on it that waits until the loupe closes.
    const viewer = state.viewer;
    if (!viewer) return render();
    viewer.stale = true;
    for (const section of viewer.sections) section.photos = section.photos?.filter(photo => !ids.includes(photo.id));
    return keepShown(viewer);
  }
  await Promise.allSettled(touched.map(section => { section.loading = null; return section.load(); }));
  const count = document.querySelector('main .count');
  if (count) count.textContent = counted(state.sections.reduce((sum, section) => sum + section.count, 0));
  if (state.viewer) await keepShown(state.viewer);
}

document.addEventListener('keydown', event => {
  if (state.viewer || event.target.matches?.('input, select, textarea') || document.querySelector('dialog[open]')) return;
  if (event.key === 'Escape' && openMenu) closeMenu();
  else if (event.key === 'Escape' && (shelved.ids.size || shelved.mode)) unshelve();
  else if (event.key === 'Escape' && (picked.ids.size || picked.mode)) unpick();
  else if ((event.key === 'Delete' || event.key === 'Backspace') && picked.ids.size && picked.erase) picked.erase();
  else if (event.key === 'a' && (event.ctrlKey || event.metaKey) && state.sections.length) {
    event.preventDefault();
    selectAll();
  }
});
window.addEventListener('hashchange', () => { unpick(); unshelve(); });

// An event, as a toast at the bottom centre: a short word, gone in two
// seconds; a longer one (a reason something failed) stays as long as it takes
// to read; with one action at most, Undo, which stays a little longer.
// `toasts` is the live region app.js keeps in the page.
function toast(text, action) {
  clearTimeout(toasts.timer);
  const el = h('div', { class: 'toast' }, h('span', {}, text),
    action && h('button', { class: 'undo', onclick: () => { toasts.replaceChildren(); action.run(); } }, action.label));
  toasts.replaceChildren(el);
  toasts.timer = setTimeout(() => { el.classList.add('leaving'); setTimeout(() => el.remove(), 160); }, Math.max(action ? 6000 : 2200, text.length * 70));
}

// Moved to the Trash can be undone: the rows the Trash gained since `before`
// are put back. (The delete answers with how many, not which.)
const trashIds = () => api('trash', { quiet: true }).then(list => new Set(list.map(item => item.id)), () => null);
async function trashed(before) {
  const now = await trashIds();
  const ids = before && now ? [...now].filter(id => !before.has(id)) : [];
  return ids.length > 0 && { label: S.undo, run: async () => {
    try { await api('trash/restore', { method: 'POST', body: JSON.stringify({ ids }) }); state.libraries = null; render(); toast(S.putBack); } catch (error) { toast(error.message); }
  } };
}

// What a change of stars or of the flag says.
const markSaid = change => 'rating' in change
  ? (change.rating ? phrase('rated', { stars: starsOf(change.rating) }) : S.ratingCleared)
  : { pick: S.picked, reject: S.rejected, none: S.flagCleared }[change.flag];

// The browser's own dialog: a title, a sentence, what goes in it, Cancel
// and one button. Resolves to the form's fields by name, or null.
function dialog(title, text, fields, button) {
  return new Promise(resolve => {
    const done = answer => { box.close(); box.remove(); holdLoupe(); resolve(answer); };
    // Answered on submit, not on the dialog's close event, which a browser
    // may hold back while its tab is in the background.
    const form = h('form', { onsubmit: event => { event.preventDefault(); done(Object.fromEntries(new FormData(form))); } },
      h('h2', {}, title), text && h('p', { class: 'body' }, text), ...fields,
      h('div', { class: 'buttons' },
        h('button', { class: 'chip', type: 'button', onclick: () => done(null) }, S.cancel),
        h('button', { class: 'primary', type: 'submit' }, button)));
    const box = h('dialog', { class: 'ask', oncancel: event => { event.preventDefault(); done(null); } }, form);
    document.body.append(box);
    box.showModal();
  });
}
const ask = async (title, text, button) => (await dialog(title, text, [], button)) !== null;

async function albumDialog(ids) {
  const list = await api('albums').catch(() => []);
  const select = h('select', { name: 'album', 'aria-label': S.album }, list.map(album => h('option', { value: album.key }, album.name)), h('option', { value: '' }, S.albumOption));
  const name = h('input', { type: 'text', name: 'name', placeholder: S.newAlbumName, hidden: list.length > 0 });
  select.addEventListener('change', () => { name.hidden = select.value !== ''; });
  const out = h('label', { class: 'check' }, h('input', { type: 'checkbox', name: 'out' }), S.takeOut);
  const asked = await dialog(phrase('albumAnd', { photos: counted(ids.length) }), null, [select, name, list.length > 0 && out].filter(Boolean), S.done);
  if (!asked) return;
  const body = { ids, inside: !asked.out, ...(asked.album ? { album: asked.album } : { name: (asked.name || '').trim() }) };
  try { await api('photos/album', { method: 'POST', body: JSON.stringify(body) }); toast(asked.out ? S.outOfAlbum : S.inAlbum); } catch (error) { toast(error.message); }
}

// New Album…: its name first, then the photographs for it, chosen on
// Photos, and Add to puts them in and opens it. An album is never empty.
async function newAlbum() {
  const asked = await dialog(S.newAlbum, null, [h('input', { type: 'text', name: 'name', placeholder: S.albumName, required: true, autocomplete: 'off' })], S.choosePhotosBtn);
  const name = (asked?.name || '').trim();
  if (!name) return;
  const start = () => { picked.mode = true; picked.album = name; paintPicked(); };
  if (location.hash === '#/') return start();
  // After the page has changed, which lets go of any choice.
  addEventListener('hashchange', start, { once: true });
  go('/');
}
async function intoNewAlbum(ids, name) {
  try {
    const { album } = await api('photos/album', { method: 'POST', body: JSON.stringify({ ids, inside: true, name }) });
    go('/', { album });
  } catch (error) { toast(error.message); }
}

// HOME-023: For a Book — the picks of a trip, or the photographs chosen,
// as the desktop's numbered chapter folders, developed on the NAS into one
// zip that starts downloading with the first photograph. `query` is
// `ids=…` or the timeline's filters.
async function bookDialog(query) {
  let plan;
  try { plan = await api('book?' + query); } catch (error) { toast(error.message); return; }
  let kept = 0;
  try { kept = Number(localStorage.getItem('numa-book-product')) || 0; } catch (_) { /* a private window */ }
  const product = h('select', { name: 'product', 'aria-label': S.photoBook },
    plan.products.map((one, at) => h('option', { value: at, selected: at === kept }, one.name)));
  const list = h('div', { class: 'chapters' });
  const fill = () => list.replaceChildren(...plan.chapters.map((chapter, at) => h('label', { class: 'check' },
    h('input', { type: 'checkbox', name: `c${at}`, checked: true }), `${String(at + 1).padStart(2, '0')} · ${chapter.name}`,
    h('span', { class: 'caption' }, grouped(chapter.count)))));
  const by = h('select', { name: 'by', 'aria-label': S.chaptersBy, hidden: !plan.albums, onchange: async () => {
    try { plan = await api(`book?${query}&by=${by.value}`); fill(); } catch (error) { toast(error.message); }
  } }, [['album', S.byAlbum], ['day', S.byDay]].map(([value, label]) => h('option', { value, selected: plan.by === value }, label)));
  fill();
  const asked = await dialog(`${plan.name} · ${counted(plan.count)}`,
    S.bookText,
    [by, list, product], S.makeBook);
  if (!asked) return;
  try { localStorage.setItem('numa-book-product', asked.product); } catch (_) { /* a private window */ }
  const leave = plan.chapters.map((_, at) => at).filter(at => !asked[`c${at}`]);
  if (leave.length === plan.chapters.length) return toast(S.leaveOne);
  const link = h('a', { href: `/api/v1/book/zip?${query}&by=${plan.by}&mm=${plan.products[asked.product].mm}&leave=${leave.join(',')}`, download: '' });
  document.body.append(link);
  link.click();
  link.remove();
  toast(S.bookDownloads);
}

// HOME-012: whose these are, said by hand; it wins over the camera's say.
async function setBy(ids, account) {
  await api('photos/by', { method: 'POST', body: JSON.stringify({ ids, account: account || null }) });
}

async function byDialog(ids) {
  const users = await api('users').catch(() => []);
  const select = h('select', { name: 'account', 'aria-label': S.photographer },
    users.map(user => h('option', { value: user.name }, user.name)), h('option', { value: '' }, S.asCamera));
  const asked = await dialog((ids.length === 1 ? S.whoTookOne : S.whoTookMany), S.whoTookText, [select], S.done);
  if (!asked) return;
  try { await setBy(ids, asked.account); toast(asked.account ? phrase('byName', { name: asked.account }) : S.asCamera); } catch (error) { toast(error.message); }
}

// The info panel's By: a choice of the household, or the camera's say.
async function byPicker(photo) {
  const users = await api('users').catch(() => []);
  const select = h('select', { class: 'by-pick', 'aria-label': S.photographer, onchange: async () => {
    try { await setBy([photo.id], select.value); photo.by = select.value || null; photo.chosen = !!select.value; } catch (error) { toast(error.message); }
  } }, h('option', { value: '' }, photo.chosen || !photo.by ? S.asCamera : phrase('byAsCamera', { name: photo.by })),
  users.map(user => h('option', { value: user.name, selected: photo.chosen && user.name === photo.by }, user.name)));
  return select;
}

async function moveDialog(ids, after) {
  const list = (await libraries().catch(() => [])).filter(library => state.me.admin || state.me.manage || library.mine);
  const select = h('select', { name: 'library', 'aria-label': S.library }, list.map(library => h('option', { value: library.id, selected: library.id === libraryOf(ids[0]) }, libraryName(library))));
  const dir = h('input', { type: 'text', name: 'dir', placeholder: S.folderOption, autocomplete: 'off' });
  const asked = await dialog(phrase('moveTitle', { photos: counted(ids.length) }), S.moveText, [select, dir], S.moveBtn);
  if (!asked) return;
  try {
    const answer = await api('photos/move', { method: 'POST', body: JSON.stringify({ ids, library: Number(asked.library), dir: asked.dir.trim() }) });
    toast(phrase('moved', { photos: counted(answer.moved) }));
    after();
  } catch (error) { toast(error.message); }
}

// HOME-016: the Libraries page's own commands.
function libraryCommands(list) {
  const row = h('div', { class: 'filters' });
  if (state.me.admin) {
    row.append(h('button', { class: 'chip', onclick: newLibrary }, S.newLibraryEll), h('button', { class: 'chip', onclick: addFolder }, S.addFolderEll), librarySelectToggle());
  }
  return row;
}

async function newLibrary() {
  const year = h('input', { type: 'text', name: 'year', value: String(new Date().getFullYear()), inputmode: 'numeric', pattern: '\\d{4}', maxlength: 4, 'aria-label': S.year, class: 'year' });
  const inYear = h('label', { class: 'check' }, h('input', { type: 'checkbox', name: 'inyear', checked: true, onchange: event => { year.disabled = !event.target.checked; } }), S.inYear, year);
  const asked = await dialog(S.newLibrary, S.newLibraryText,
    [h('input', { type: 'text', name: 'name', placeholder: S.name, required: true, autocomplete: 'off' }), inYear], S.makeLibrary);
  if (!asked) return;
  try {
    const made = await api('libraries', { method: 'POST', body: JSON.stringify({ name: asked.name.trim(), year: asked.inyear ? asked.year : null }) });
    state.libraries = null;
    toast(phrase('isLibrary', { name: made.name }));
    render();
  } catch (error) { toast(error.message); }
}

async function addFolder() {
  let folders;
  try { folders = await api('libraries/folders'); } catch (error) { return toast(error.message); }
  if (!folders.length) return ask(S.addFolder, S.addFolderNone, S.ok);
  const asked = await dialog(S.addFolder, S.addFolderText,
    [h('select', { name: 'folder', 'aria-label': S.folder }, folders.map(one => h('option', { value: one.folder }, one.folder)))], S.add);
  if (!asked) return;
  try { await api('libraries', { method: 'POST', body: JSON.stringify({ folder: asked.folder }) }); state.libraries = null; render(); } catch (error) { toast(error.message); }
}

// HOME-021: libraries chosen as photographs are — a click once one is
// chosen, shift for a run, Ctrl or ⌘ to start, Select — and the bar of what
// can be done to them. An admin's, for the libraries in the photos folder.
const shelved = { ids: new Set(), anchor: null, mode: false };
const librariesCounted = n => `${grouped(n)} ${n === 1 ? S.libraryOne : S.librariesMany}`;

function libraryShelf(library, list) {
  const place = collection(libraryName(library), counted(library.count), 'library=' + library.id, event => {
    if (!chooseLibrary(event, library, list)) go('/', { library: library.id });
  });
  place.dataset.library = library.id;
  place.classList.toggle('picked', shelved.ids.has(library.id));
  if (state.me.admin && !library.mine) place.querySelector('.stack').append(h('span', { class: 'check', 'aria-hidden': 'true' }));
  return h('div', { class: 'shelf' }, place);
}

function chooseLibrary(event, library, list) {
  const choosing = shelved.mode || shelved.ids.size > 0 || event.shiftKey || event.ctrlKey || event.metaKey || event.target.closest?.('.check');
  if (!choosing || !state.me.admin || library.mine) return false;
  const order = list.filter(one => !one.mine).map(one => one.id);
  if (event.shiftKey && shelved.anchor != null && order.includes(shelved.anchor)) {
    const [a, b] = [order.indexOf(shelved.anchor), order.indexOf(library.id)].sort((x, y) => x - y);
    order.slice(a, b + 1).forEach(one => shelved.ids.add(one));
  } else {
    if (shelved.ids.has(library.id)) shelved.ids.delete(library.id); else shelved.ids.add(library.id);
    shelved.anchor = library.id;
  }
  paintShelved(list);
  return true;
}

function unshelve() {
  if (!shelved.ids.size && !shelved.mode) return;
  shelved.ids.clear();
  shelved.anchor = null;
  shelved.mode = false;
  paintShelved();
}

function paintShelved(list) {
  for (const place of document.querySelectorAll('.place[data-library]')) place.classList.toggle('picked', shelved.ids.has(Number(place.dataset.library)));
  document.body.classList.toggle('picking', shelved.mode || shelved.ids.size > 0);
  document.querySelectorAll('.library-select').forEach(chip => chip.classList.toggle('chosen', shelved.mode));
  libraryBar(list);
}

function librarySelectToggle() {
  return h('button', { class: 'chip library-select' + (shelved.mode ? ' chosen' : ''), onclick: () => {
    if (shelved.mode) unshelve(); else { shelved.mode = true; paintShelved(); }
  } }, S.select);
}

async function libraryBar(list) {
  document.querySelector('.selection')?.remove();
  if (!shelved.ids.size && !shelved.mode) return;
  list = list || await libraries().catch(() => []);
  const chosen = list.filter(library => shelved.ids.has(library.id));
  const ids = chosen.map(library => library.id);
  const n = ids.length;
  const bar = h('div', { class: 'selection', role: 'toolbar', 'aria-label': S.chosenLibraries });
  document.body.append(bar);
  const close = h('button', { class: 'round small', 'aria-label': S.clearChoice, title: S.clearTip, onclick: unshelve }, icon('close'));
  const button = (label, onclick) => h('button', { class: 'chip', onclick }, label);
  const item = (label, onclick) => h('button', { onclick }, label);
  const everything = button(S.selectAll, () => { list.filter(library => !library.mine).forEach(library => shelved.ids.add(library.id)); paintShelved(list); });
  if (!n) {
    bar.append(h('div', { class: 'lead' }, close, h('span', { class: 'count' }, S.chooseLibraries), everything));
    return;
  }
  const send = async (path, body, said) => {
    try {
      const answer = await api(path, { method: 'POST', body: JSON.stringify(body) });
      toast(said(answer));
      unshelve();
      state.libraries = null;
      render();
    } catch (error) { toast(error.message); }
  };
  const names = chosen.length === 1 ? chosen[0].name : librariesCounted(n);
  bar.append(h('div', { class: 'lead' }, close, h('span', { class: 'count' }, phrase('librariesChosen', { libraries: librariesCounted(n) })), everything), h('div', { class: 'acts' }, ...[
    n === 1 && button(S.open, () => go('/', { library: ids[0] })),
    n === 1 && button(S.renameEll, () => renameLibrary(chosen[0])),
    h('button', { class: 'round small', 'aria-label': S.more, title: S.more, onclick: event => { event.stopPropagation(); menuOver(event.currentTarget, [
      item(S.combineEll, () => combineDialog(chosen, list.filter(library => !library.mine), send)),
      h('div', { class: 'rule' }),
      item(S.removeFromNumaEll, async () => {
        if (await ask(phrase('removeNumaQ', { names }), S.removeNumaText, S.removeFromNuma))
          send('libraries/remove', { ids }, answer => phrase('removed', { libraries: librariesCounted(answer.removed) }));
      }),
      item(S.moveToTrashEll, async () => {
        if (await ask(phrase('trashLibQ', { names }), S.trashLibText, S.moveToTrash))
          send('libraries/delete', { ids }, answer => phrase('movedLibs', { libraries: librariesCounted(answer.trashed) }));
      }),
    ]); } }, icon('more'))].filter(Boolean)));
}

// HOME-029: names from where the photographs were taken, by their GPS or
// that of the household's photos of the same hours, as chips that fill
// `field`. Gone again when there is nothing to offer.
function nameIdeas(asked, field) {
  const box = h('div', { class: 'ideas' }, h('span', { class: 'caption' }, S.lookingWhere));
  asked.then(answer => {
    if (!answer?.names?.length) return box.remove();
    box.replaceChildren(h('span', { class: 'caption' }, answer.from === 'borrowed' ? S.sameHours : S.whereTaken),
      ...answer.names.map(name => h('button', { type: 'button', class: 'chip small', onclick: () => {
        field.value = name;
        field.dispatchEvent(new Event('input'));
      } }, name)));
  }, () => box.remove());
  return box;
}

async function renameLibrary(library) {
  const field = h('input', { type: 'text', name: 'name', value: library.name, required: true, autocomplete: 'off' });
  const ideas = nameIdeas(api(`libraries/${library.id}/names`, { wait: 120, quiet: true }), field);
  const asked = await dialog(S.renameLibrary, S.renameText, [field, ideas], S.rename);
  if (!asked) return;
  try { await api('libraries/' + library.id, { method: 'PUT', body: JSON.stringify({ name: asked.name.trim() }) }); unshelve(); state.libraries = null; render(); } catch (error) { toast(error.message); }
}

// One library or more into another: the one chosen in the dialog keeps its
// folder, the others' photographs move into it.
async function combineDialog(chosen, archive, send) {
  const choices = chosen.length > 1 ? chosen : archive.filter(library => library.id !== chosen[0].id);
  if (!choices.length) return ask(S.combine, S.combineNone, S.ok);
  const select = h('select', { name: 'into', 'aria-label': S.into }, choices.map(library => h('option', { value: library.id }, library.name)));
  const asked = await dialog(chosen.length > 1 ? phrase('combineN', { libraries: librariesCounted(chosen.length) }) : phrase('combineWith', { name: chosen[0].name }),
    S.combineText, [select], S.combine);
  if (!asked) return;
  send('libraries/combine', { ids: chosen.map(library => library.id), into: Number(asked.into) }, answer => phrase('combined', { photos: counted(answer.moved) }));
}

// HOME-017: the Trash — what was moved to it, for 30 days.
async function trashPage(main) {
  let list;
  try { list = await api('trash'); } catch (error) { return main.append(empty(S.trash, error.message, true)); }
  const dropped = state.me.admin ? await api('trash/libraries').catch(() => []) : [];
  if (!main.isConnected) return;
  if (!list.length && !dropped.length) return main.append(empty(S.trashEmpty, S.trashEmptyText));
  // HOME-021: whole libraries, each put back on its own.
  if (dropped.length) main.append(h('div', { class: 'import-page dropped' }, h('h1', {}, S.librariesInTrash), dropped.map(row => h('div', { class: 'shoot' }, h('div', { class: 'head' },
    h('span', { class: 'name' }, row.name), h('span', { class: 'caption' }, phrase('folderDays', { folder: row.folder, days: `${row.left} ${row.left === 1 ? S.day : S.days}` })),
    h('button', { class: 'chip', onclick: async () => {
      try { const answer = await api('trash/libraries/restore', { method: 'POST', body: JSON.stringify({ ids: [row.id] }) }); if (!answer.restored) throw new Error(phrase('folderBack', { folder: row.folder })); toast(phrase('libPutBack', { name: row.name })); state.libraries = null; render(); } catch (error) { toast(error.message); }
    } }, S.putBackBtn))))));
  if (!list.length) return;
  const chosen = new Set();
  const restore = h('button', { class: 'primary', disabled: true, onclick: async () => {
    try { const answer = await api('trash/restore', { method: 'POST', body: JSON.stringify({ ids: [...chosen] }) }); toast(phrase('photosPutBack', { photos: counted(answer.restored) })); render(); } catch (error) { toast(error.message); }
  } }, S.putBackBtn);
  const count = h('span', { class: 'caption count' }, counted(list.length));
  const names = new Map((await libraries().catch(() => [])).map(library => [library.id, libraryName(library)]));
  main.append(h('div', { class: 'import-page' },
    h('div', { class: 'top' }, h('div', {}, h('h1', {}, S.trash), h('p', { class: 'body' }, S.trashText)),
      h('div', { class: 'act' }, count, restore)),
    h('div', { class: 'grid' }, list.map(item => {
      const image = h('img', { src: `/api/v1/trash/${item.id}/thumb`, loading: 'lazy', alt: '' });
      image.addEventListener('load', () => image.classList.add('in'));
      return h('button', { class: 'tile', title: `${names.get(item.library) || ''}/${item.path}`, onclick: event => {
        if (chosen.has(item.id)) chosen.delete(item.id); else chosen.add(item.id);
        event.currentTarget.classList.toggle('ticked', chosen.has(item.id));
        restore.disabled = !chosen.size;
        count.textContent = chosen.size ? phrase('chosenN', { n: grouped(chosen.size) }) : counted(list.length);
      } }, image, h('span', { class: 'check', 'aria-hidden': 'true' }), h('span', { class: 'badge' }, `${item.left} ${item.left === 1 ? S.day : S.days}`));
    }))));
}
