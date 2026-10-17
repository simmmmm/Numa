// HOME-016/017: managing the archive from the pages. Photographs chosen on
// the grid as on the desktop — a click once something is chosen, shift for
// a run, Ctrl or ⌘ to start, Select on a phone — and what can be done to
// them; new library folders, renaming one; the trash. HOME-021: libraries
// chosen on their page the same way, and what can be done to them.
'use strict';

const picked = { ids: new Set(), anchor: null, mode: false };
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
  paintPicked();
}

function paintPicked() {
  for (const tile of document.querySelectorAll('.tile[data-id]')) tile.classList.toggle('picked', picked.ids.has(Number(tile.dataset.id)));
  document.body.classList.toggle('picking', picked.mode || picked.ids.size > 0);
  document.querySelectorAll('.select-toggle').forEach(chip => chip.classList.toggle('chosen', picked.mode));
  selectionBar();
}

function selectToggle() {
  return h('button', { class: 'chip select-toggle' + (picked.mode ? ' chosen' : ''), onclick: () => {
    if (picked.mode) unpick(); else { picked.mode = true; paintPicked(); }
  } }, 'Select');
}

// What may change these: an admin anything, an account its own Sent Home.
async function mayManage(ids) {
  if (state.me.admin) return true;
  const mine = (await libraries().catch(() => [])).filter(library => library.mine).map(library => library.id);
  return [...ids].every(id => mine.includes(libraryOf(id)));
}

// What can be done to these photographs, as menu items: the bar's star and
// ⋯ menus, and the right-click menu, are made of the same ones.
function chosenActions(ids, manage) {
  const n = ids.length;
  const send = async (path, body, after) => {
    try { const answer = await api(path, { method: 'POST', body: JSON.stringify({ ids, ...body }) }); after?.(answer); } catch (error) { toast(error.message); }
  };
  const refresh = () => { unpick(); state.libraries = null; refreshAround(ids); };
  const marks = (body, label) => send('photos/marks', body, () => {
    for (const section of state.sections) section.photos?.filter(photo => ids.includes(photo.id)).forEach(photo => { if ('rating' in body) photo.rating = body.rating; });
    for (const id of ids) {
      const tile = document.querySelector(`.tile[data-id="${id}"]`);
      if (!tile || !('rating' in body)) continue;
      tile.querySelector('.stars')?.remove();
      if (body.rating > 0) tile.append(h('span', { class: 'stars' }, '★'.repeat(body.rating)));
    }
    toast(`${label} · ${counted(n)}`);
  });
  const item = (label, onclick, title) => h('button', { onclick, title }, label);
  const erase = async () => {
    if (await ask(`Delete ${counted(n)} from the disk?`, "They go to their library's trash, where they can be put back for 30 days; after that they are gone for good.", 'Delete from Disk', true))
      send('photos/delete', {}, answer => { toast(`${counted(answer.trashed)} in the trash`); refresh(); });
  };
  return {
    erase: manage ? erase : null,
    album: () => item('Album…', () => albumDialog(ids)),
    download: () => h('a', { href: `/api/v1/photos/zip?ids=${ids.join(',')}`, download: '', title: 'The originals, as one zip' }, 'Download'),
    marks: () => [
      h('div', { class: 'section-label' }, 'Stars'),
      h('div', { class: 'star-row' }, [1, 2, 3, 4, 5].map(stars => h('button', { title: `${stars} (${stars})`, 'aria-label': `${stars} star${stars > 1 ? 's' : ''}`, onclick: () => marks({ rating: stars }, '★'.repeat(stars)) }, '★'.repeat(stars)))),
      item('No Rating', () => marks({ rating: 0 }, 'No Rating'), '0'),
      h('div', { class: 'section-label' }, 'Flag'),
      Object.entries(FLAGS).map(([flag, label]) => item(label, () => marks({ flag }, label), { pick: 'P', reject: 'X', none: 'U' }[flag])),
    ],
    more: () => [
      item('Taken By…', () => byDialog(ids)),
      item('For a Book…', () => bookDialog(`ids=${ids.join(',')}`)),
      manage && h('div', { class: 'rule' }),
      manage && item('Move…', () => moveDialog(ids, refresh)),
      manage && item('Remove from Library…', async () => {
        if (await ask(`Remove ${counted(n)} from the library?`, 'The files stay in their folder. Numa Home leaves them out from now on, and their stars, flags and edits here are forgotten.', 'Remove from Library'))
          send('photos/remove', {}, refresh);
      }),
      manage && item('Delete from Disk…', erase, 'Delete (Del)'),
    ].filter(Boolean),
  };
}

async function selectionBar() {
  document.querySelector('.selection')?.remove();
  picked.erase = null;
  if (!picked.ids.size && !picked.mode) return;
  const ids = [...picked.ids];
  const n = ids.length;
  const bar = h('div', { class: 'selection', role: 'toolbar', 'aria-label': 'Chosen photographs' });
  document.body.append(bar);
  // The few that are done most in sight; the rest under More, so the bar
  // holds on a phone's width too.
  const close = h('button', { class: 'round small', 'aria-label': 'Clear the choice', title: 'Clear (Esc)', onclick: unpick }, icon('close'));
  const round = (name, label, items) => h('button', { class: 'round small', 'aria-label': label, title: label, onclick: event => {
    event.stopPropagation();
    menuOver(event.currentTarget, items());
  } }, icon(name));
  if (!n) {
    bar.append(close, h('span', { class: 'count' }, 'Choose photographs'));
    return;
  }
  const manage = await mayManage(ids);
  if (!bar.isConnected) return;
  const actions = chosenActions(ids, manage);
  picked.erase = actions.erase;
  bar.append(close, h('span', { class: 'count' }, `${grouped(n)} chosen`),
    h('button', { class: 'chip', onclick: () => albumDialog(ids) }, 'Album…'),
    round('star', 'Stars and Flag', actions.marks),
    h('a', { class: 'round small', href: `/api/v1/photos/zip?ids=${ids.join(',')}`, download: '', 'aria-label': 'Download', title: 'Download the originals, as one zip' }, icon('download')),
    round('more', 'More', actions.more));
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
    ids.length === 1 ? h('button', { onclick: () => { unpick(); tile.click(); } }, 'Open') : h('div', { class: 'section-label' }, `${grouped(ids.length)} chosen`),
    actions.album(), actions.download(),
    h('div', { class: 'rule' }), actions.marks(),
    h('div', { class: 'rule' }), actions.more(),
  ]);
}

// After a change to these photographs, the months that held them asked
// again in place, so the page stays where it was; the search anew.
async function refreshAround(ids) {
  const touched = state.sections.filter(section => section.el && section.photos?.some(photo => ids.includes(photo.id)));
  if (!touched.length) return render();
  await Promise.allSettled(touched.map(section => { section.loading = null; return section.load(); }));
  const count = document.querySelector('main .count');
  if (count) count.textContent = counted(state.sections.reduce((sum, section) => sum + section.count, 0));
}

document.addEventListener('keydown', event => {
  if (state.viewer || event.target.matches?.('input, select, textarea') || document.querySelector('dialog[open]')) return;
  const open = document.querySelector('.menu');
  if (event.key === 'Escape' && open) open.remove();
  else if (event.key === 'Escape' && (shelved.ids.size || shelved.mode)) unshelve();
  else if (event.key === 'Escape' && (picked.ids.size || picked.mode)) unpick();
  else if ((event.key === 'Delete' || event.key === 'Backspace') && picked.ids.size && picked.erase) picked.erase();
  else if (event.key === 'a' && (event.ctrlKey || event.metaKey) && state.sections.length) {
    event.preventDefault();
    state.sections.flatMap(section => section.photos || []).forEach(photo => picked.ids.add(photo.id));
    paintPicked();
  }
});
window.addEventListener('hashchange', () => { unpick(); unshelve(); });

// A short word at the foot, gone in two seconds; a longer one (a reason
// something failed) stays as long as it takes to read.
function toast(text) {
  document.querySelector('.toast')?.remove();
  const el = h('div', { class: 'toast', role: 'status' }, text);
  document.body.append(el);
  setTimeout(() => el.remove(), Math.max(2200, text.length * 70));
}

// The browser's own dialog: a title, a sentence, what goes in it, Cancel
// and one button. Resolves to the form's fields by name, or null.
function dialog(title, text, fields, button, destructive) {
  return new Promise(resolve => {
    const done = answer => { box.close(); box.remove(); resolve(answer); };
    // Answered on submit, not on the dialog's close event, which a browser
    // may hold back while its tab is in the background.
    const form = h('form', { onsubmit: event => { event.preventDefault(); done(Object.fromEntries(new FormData(form))); } },
      h('h2', {}, title), text && h('p', { class: 'body' }, text), ...fields,
      h('div', { class: 'buttons' },
        h('button', { class: 'chip', type: 'button', onclick: () => done(null) }, 'Cancel'),
        h('button', { class: destructive ? 'primary destructive' : 'primary', type: 'submit' }, button)));
    const box = h('dialog', { class: 'ask', oncancel: event => { event.preventDefault(); done(null); } }, form);
    document.body.append(box);
    box.showModal();
  });
}
const ask = async (title, text, button, destructive) => (await dialog(title, text, [], button, destructive)) !== null;

async function albumDialog(ids) {
  const list = await api('albums').catch(() => []);
  const select = h('select', { name: 'album', 'aria-label': 'Album' }, list.map(album => h('option', { value: album.key }, album.name)), h('option', { value: '' }, 'A new album…'));
  const name = h('input', { type: 'text', name: 'name', placeholder: 'Name of the new album', hidden: list.length > 0 });
  select.addEventListener('change', () => { name.hidden = select.value !== ''; });
  const out = h('label', { class: 'check' }, h('input', { type: 'checkbox', name: 'out' }), 'Take them out of it instead');
  const asked = await dialog(`${counted(ids.length)} and an album`, null, [select, name, list.length > 0 && out].filter(Boolean), 'Done');
  if (!asked) return;
  const body = { ids, inside: !asked.out, ...(asked.album ? { album: asked.album } : { name: (asked.name || '').trim() }) };
  try { await api('photos/album', { method: 'POST', body: JSON.stringify(body) }); toast(asked.out ? 'Out of the album' : 'In the album'); } catch (error) { toast(error.message); }
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
  const product = h('select', { name: 'product', 'aria-label': 'Photo Book' },
    plan.products.map((one, at) => h('option', { value: at, selected: at === kept }, one.name)));
  const list = h('div', { class: 'chapters' });
  const fill = () => list.replaceChildren(...plan.chapters.map((chapter, at) => h('label', { class: 'check' },
    h('input', { type: 'checkbox', name: `c${at}`, checked: true }), `${String(at + 1).padStart(2, '0')} · ${chapter.name}`,
    h('span', { class: 'caption' }, grouped(chapter.count)))));
  const by = h('select', { name: 'by', 'aria-label': 'Chapters By', hidden: !plan.albums, onchange: async () => {
    try { plan = await api(`book?${query}&by=${by.value}`); fill(); } catch (error) { toast(error.message); }
  } }, [['album', 'Chapters by Album'], ['day', 'Chapters by Day']].map(([value, label]) => h('option', { value, selected: plan.by === value }, label)));
  fill();
  const asked = await dialog(`${plan.name} · ${counted(plan.count)}`,
    'In the order they happened, a numbered folder per chapter, at the page\'s size. Unzip it and open the folder in CEWE, Albelli or Popsa: the book is half laid out.',
    [by, list, product], 'Make the Book');
  if (!asked) return;
  try { localStorage.setItem('numa-book-product', asked.product); } catch (_) { /* a private window */ }
  const leave = plan.chapters.map((_, at) => at).filter(at => !asked[`c${at}`]);
  if (leave.length === plan.chapters.length) return toast('Leave at least one chapter in the book.');
  const link = h('a', { href: `/api/v1/book/zip?${query}&by=${plan.by}&mm=${plan.products[asked.product].mm}&leave=${leave.join(',')}`, download: '' });
  document.body.append(link);
  link.click();
  link.remove();
  toast('The book downloads as Numa develops each photograph');
}

// HOME-012: whose these are, said by hand; it wins over the camera's say.
async function setBy(ids, account) {
  await api('photos/by', { method: 'POST', body: JSON.stringify({ ids, account: account || null }) });
}

async function byDialog(ids) {
  const users = await api('users').catch(() => []);
  const select = h('select', { name: 'account', 'aria-label': 'Photographer' },
    users.map(user => h('option', { value: user.name }, user.name)), h('option', { value: '' }, 'As the camera says'));
  const asked = await dialog(`Who took ${ids.length === 1 ? 'it' : 'them'}?`, 'Said here, it wins over the camera linked to an account and its Artist.', [select], 'Done');
  if (!asked) return;
  try { await setBy(ids, asked.account); toast(asked.account ? `By ${asked.account}` : 'As the camera says'); } catch (error) { toast(error.message); }
}

// The info panel's By: a choice of the household, or the camera's say.
async function byPicker(photo) {
  const users = await api('users').catch(() => []);
  const select = h('select', { class: 'by-pick', 'aria-label': 'Photographer', onchange: async () => {
    try { await setBy([photo.id], select.value); photo.by = select.value || null; photo.chosen = !!select.value; } catch (error) { toast(error.message); }
  } }, h('option', { value: '' }, photo.chosen || !photo.by ? 'As the camera says' : `${photo.by}, as the camera says`),
  users.map(user => h('option', { value: user.name, selected: photo.chosen && user.name === photo.by }, user.name)));
  return select;
}

async function moveDialog(ids, after) {
  const list = (await libraries().catch(() => [])).filter(library => state.me.admin || library.mine);
  const select = h('select', { name: 'library', 'aria-label': 'Library' }, list.map(library => h('option', { value: library.id, selected: library.id === libraryOf(ids[0]) }, libraryName(library))));
  const dir = h('input', { type: 'text', name: 'dir', placeholder: 'A folder inside it (optional)', autocomplete: 'off' });
  const asked = await dialog(`Move ${counted(ids.length)}`, 'The files move, with their stars, flags, edits and albums.', [select, dir], 'Move');
  if (!asked) return;
  try {
    const answer = await api('photos/move', { method: 'POST', body: JSON.stringify({ ids, library: Number(asked.library), dir: asked.dir.trim() }) });
    toast(`${counted(answer.moved)} moved`);
    after();
  } catch (error) { toast(error.message); }
}

// HOME-016: the Libraries page's own commands.
function libraryCommands(list) {
  const row = h('div', { class: 'filters' });
  if (state.me.admin) {
    row.append(h('button', { class: 'chip', onclick: newLibrary }, 'New Library…'), h('button', { class: 'chip', onclick: addFolder }, 'Add Folder…'), librarySelectToggle());
  }
  return row;
}

async function newLibrary() {
  const year = h('input', { type: 'text', name: 'year', value: String(new Date().getFullYear()), inputmode: 'numeric', pattern: '\\d{4}', maxlength: 4, 'aria-label': 'Year', class: 'year' });
  const inYear = h('label', { class: 'check' }, h('input', { type: 'checkbox', name: 'inyear', checked: true, onchange: event => { year.disabled = !event.target.checked; } }), 'In a year folder', year);
  const asked = await dialog('New Library', 'A new folder in the photos folder, as Import makes them: photos/2026/Japan.',
    [h('input', { type: 'text', name: 'name', placeholder: 'Name', required: true, autocomplete: 'off' }), inYear], 'Make Library');
  if (!asked) return;
  try {
    const made = await api('libraries', { method: 'POST', body: JSON.stringify({ name: asked.name.trim(), year: asked.inyear ? asked.year : null }) });
    state.libraries = null;
    toast(`${made.name} is a library`);
    render();
  } catch (error) { toast(error.message); }
}

async function addFolder() {
  let folders;
  try { folders = await api('libraries/folders'); } catch (error) { return toast(error.message); }
  if (!folders.length) return ask('Add Folder', 'Every folder in the photos folder is a library already. A folder copied in shows up here before the next look round, every quarter of an hour.', 'OK');
  const asked = await dialog('Add Folder', 'A folder put in the photos folder since Numa Home last looked, taken in now.',
    [h('select', { name: 'folder', 'aria-label': 'Folder' }, folders.map(one => h('option', { value: one.folder }, one.folder)))], 'Add');
  if (!asked) return;
  try { await api('libraries', { method: 'POST', body: JSON.stringify({ folder: asked.folder }) }); state.libraries = null; render(); } catch (error) { toast(error.message); }
}

// HOME-021: libraries chosen as photographs are — a click once one is
// chosen, shift for a run, Ctrl or ⌘ to start, Select — and the bar of what
// can be done to them. An admin's, for the libraries in the photos folder.
const shelved = { ids: new Set(), anchor: null, mode: false };
const librariesCounted = n => `${grouped(n)} librar${n === 1 ? 'y' : 'ies'}`;

function libraryShelf(library, list) {
  const place = collection([h('i', { class: 'tint', style: { background: tint(library.id) } }), libraryName(library)], counted(library.count), 'library=' + library.id, event => {
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
  } }, 'Select');
}

async function libraryBar(list) {
  document.querySelector('.selection')?.remove();
  if (!shelved.ids.size && !shelved.mode) return;
  list = list || await libraries().catch(() => []);
  const chosen = list.filter(library => shelved.ids.has(library.id));
  const ids = chosen.map(library => library.id);
  const n = ids.length;
  const bar = h('div', { class: 'selection', role: 'toolbar', 'aria-label': 'Chosen libraries' });
  document.body.append(bar);
  const close = h('button', { class: 'round small', 'aria-label': 'Clear the choice', title: 'Clear (Esc)', onclick: unshelve }, icon('close'));
  const button = (label, onclick) => h('button', { class: 'chip', onclick }, label);
  const item = (label, onclick) => h('button', { onclick }, label);
  if (!n) {
    bar.append(close, h('span', { class: 'count' }, 'Choose libraries'));
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
  bar.append(...[
    close,
    h('span', { class: 'count' }, `${librariesCounted(n)} chosen`),
    n === 1 && button('Open', () => go('/', { library: ids[0] })),
    n === 1 && button('Rename…', () => renameLibrary(chosen[0])),
    h('button', { class: 'round small', 'aria-label': 'More', title: 'More', onclick: event => { event.stopPropagation(); menuOver(event.currentTarget, [
      item('Combine…', () => combineDialog(chosen, list.filter(library => !library.mine), send)),
      h('div', { class: 'rule' }),
      item('Remove from Numa Home…', async () => {
        if (await ask(`Remove ${names} from Numa Home?`, 'The folders stay in the photos folder, untouched. Add Folder takes one in again, with all that was done in it.', 'Remove from Numa Home'))
          send('libraries/remove', { ids }, answer => `${librariesCounted(answer.removed)} removed`);
      }),
      item('Delete from Disk…', async () => {
        if (await ask(`Delete ${names} from the disk?`, 'The folders go to the trash, photographs, edits and all, where they can be put back for 30 days; after that they are gone for good.', 'Delete from Disk', true))
          send('libraries/delete', { ids }, answer => `${librariesCounted(answer.trashed)} in the trash`);
      }),
    ]); } }, icon('more'))].filter(Boolean));
}

// HOME-029: names from where the photographs were taken, by their GPS or
// that of the household's photos of the same hours, as chips that fill
// `field`. Gone again when there is nothing to offer.
function nameIdeas(asked, field) {
  const box = h('div', { class: 'ideas' }, h('span', { class: 'caption' }, 'Looking where these were taken…'));
  asked.then(answer => {
    if (!answer?.names?.length) return box.remove();
    box.replaceChildren(h('span', { class: 'caption' }, answer.from === 'borrowed' ? 'From photos taken in the same hours' : 'Where they were taken'),
      ...answer.names.map(name => h('button', { type: 'button', class: 'chip small', onclick: () => {
        field.value = name;
        field.dispatchEvent(new Event('input'));
      } }, name)));
  }, () => box.remove());
  return box;
}

async function renameLibrary(library) {
  const field = h('input', { type: 'text', name: 'name', value: library.name, required: true, autocomplete: 'off' });
  const ideas = nameIdeas(api(`libraries/${library.id}/names`, { wait: 120 }), field);
  const asked = await dialog('Rename Library', 'The folder itself is renamed, in its place.', [field, ideas], 'Rename');
  if (!asked) return;
  try { await api('libraries/' + library.id, { method: 'PUT', body: JSON.stringify({ name: asked.name.trim() }) }); unshelve(); state.libraries = null; render(); } catch (error) { toast(error.message); }
}

// One library or more into another: the one chosen in the dialog keeps its
// folder, the others' photographs move into it.
async function combineDialog(chosen, archive, send) {
  const choices = chosen.length > 1 ? chosen : archive.filter(library => library.id !== chosen[0].id);
  if (!choices.length) return ask('Combine', 'There is no other library to combine it with.', 'OK');
  const select = h('select', { name: 'into', 'aria-label': 'Into' }, choices.map(library => h('option', { value: library.id }, library.name)));
  const asked = await dialog(chosen.length > 1 ? `Combine ${librariesCounted(chosen.length)}` : `Combine ${chosen[0].name} with…`,
    "The photographs move into the library chosen here, each in the folder it had, with their stars, flags, edits and albums. The emptied folders go to the trash.", [select], 'Combine');
  if (!asked) return;
  send('libraries/combine', { ids: chosen.map(library => library.id), into: Number(asked.into) }, answer => `${counted(answer.moved)} combined`);
}

// HOME-017: the trash — what was deleted from the disk, for 30 days.
async function trashPage(main) {
  let list;
  try { list = await api('trash'); } catch (error) { return main.append(empty('Trash', error.message)); }
  const dropped = state.me.admin ? await api('trash/libraries').catch(() => []) : [];
  if (!main.isConnected) return;
  if (!list.length && !dropped.length) return main.append(empty('The Trash Is Empty', 'Photographs and libraries deleted from the disk wait here for 30 days, to be put back.'));
  // HOME-021: whole libraries, each put back on its own.
  if (dropped.length) main.append(h('div', { class: 'import-page dropped' }, h('h1', {}, 'Libraries in the Trash'), dropped.map(row => h('div', { class: 'shoot' }, h('div', { class: 'head' },
    h('span', { class: 'name' }, row.name), h('span', { class: 'caption' }, `${row.folder} · ${row.left} day${row.left === 1 ? '' : 's'}`),
    h('button', { class: 'chip', onclick: async () => {
      try { const answer = await api('trash/libraries/restore', { method: 'POST', body: JSON.stringify({ ids: [row.id] }) }); if (!answer.restored) throw new Error(`A folder ${row.folder} is in the photos folder again; rename it first.`); toast(`${row.name} put back`); state.libraries = null; render(); } catch (error) { toast(error.message); }
    } }, 'Put Back'))))));
  if (!list.length) return;
  const chosen = new Set();
  const restore = h('button', { class: 'primary', disabled: true, onclick: async () => {
    try { const answer = await api('trash/restore', { method: 'POST', body: JSON.stringify({ ids: [...chosen] }) }); toast(`${counted(answer.restored)} put back`); render(); } catch (error) { toast(error.message); }
  } }, 'Put Back');
  const count = h('span', { class: 'caption count' }, counted(list.length));
  const names = new Map((await libraries().catch(() => [])).map(library => [library.id, libraryName(library)]));
  main.append(h('div', { class: 'import-page' },
    h('div', { class: 'top' }, h('div', {}, h('h1', {}, 'Trash'), h('p', { class: 'body' }, 'Deleted from the disk; each is gone for good when its days are up. Choose what to put back.')),
      h('div', { class: 'act' }, count, restore)),
    h('div', { class: 'grid' }, list.map(item => {
      const image = h('img', { src: `/api/v1/trash/${item.id}/thumb`, loading: 'lazy', alt: '' });
      image.addEventListener('load', () => image.classList.add('in'));
      return h('button', { class: 'tile', title: `${names.get(item.library) || ''}/${item.path}`, onclick: event => {
        if (chosen.has(item.id)) chosen.delete(item.id); else chosen.add(item.id);
        event.currentTarget.classList.toggle('ticked', chosen.has(item.id));
        restore.disabled = !chosen.size;
        count.textContent = chosen.size ? `${grouped(chosen.size)} chosen` : counted(list.length);
      } }, image, h('span', { class: 'check', 'aria-hidden': 'true' }), h('span', { class: 'badge' }, `${item.left} day${item.left === 1 ? '' : 's'}`));
    }))));
}
