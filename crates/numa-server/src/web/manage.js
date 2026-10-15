// HOME-016/017: managing the archive from the pages. Photographs chosen on
// the grid as on the desktop — a click once something is chosen, shift for
// a run, Ctrl or ⌘ to start, Select on a phone — and what can be done to
// them; new library folders, renaming one; the trash.
'use strict';

const picked = { ids: new Set(), anchor: null, mode: false };
const libraryOf = id => Math.floor(id / 2 ** 32);

// Whether the click chose (true) rather than opened.
function pick(event, id) {
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

async function selectionBar() {
  document.querySelector('.selection')?.remove();
  if (!picked.ids.size && !picked.mode) return;
  const ids = [...picked.ids];
  const n = ids.length;
  const bar = h('div', { class: 'selection', role: 'toolbar', 'aria-label': 'Chosen photographs' });
  document.body.append(bar);
  const send = async (path, body, after) => {
    try { const answer = await api(path, { method: 'POST', body: JSON.stringify({ ids, ...body }) }); after?.(answer); } catch (error) { alert(error.message); }
  };
  const refresh = () => { unpick(); state.libraries = null; render(); };
  const marks = (body, label) => send('photos/marks', body, () => {
    for (const section of state.sections) section.photos?.filter(photo => picked.ids.has(photo.id)).forEach(photo => { if ('rating' in body) photo.rating = body.rating; });
    for (const id of ids) {
      const tile = document.querySelector(`.tile[data-id="${id}"]`);
      if (!tile || !('rating' in body)) continue;
      tile.querySelector('.stars')?.remove();
      if (body.rating > 0) tile.append(h('span', { class: 'stars' }, '★'.repeat(body.rating)));
    }
    toast(`${label} · ${counted(n)}`);
  });
  const button = (label, onclick, extra) => h('button', { class: 'chip', onclick, ...extra }, label);
  const choose = (label, items) => button(label, event => { event.stopPropagation(); menu(bar, items); });
  if (!n) {
    bar.append(h('span', { class: 'count' }, 'Choose photographs'), button('Cancel', unpick));
    return;
  }
  const manage = await mayManage(ids);
  if (!bar.isConnected) return;
  bar.append(
    h('span', { class: 'count' }, `${grouped(n)} chosen`),
    choose('Stars', [1, 2, 3, 4, 5].map(stars => h('button', { onclick: () => marks({ rating: stars }, '★'.repeat(stars)) }, '★'.repeat(stars)))
      .concat(h('button', { onclick: () => marks({ rating: 0 }, 'No Rating') }, 'No Rating'))),
    choose('Flag', Object.entries(FLAGS).map(([flag, label]) => h('button', { onclick: () => marks({ flag }, label) }, label))),
    button('Album…', () => albumDialog(ids)),
    button('By…', () => byDialog(ids)),
    h('a', { class: 'chip', href: `/api/v1/photos/zip?ids=${ids.join(',')}`, download: '', title: 'The originals, as one zip' }, 'Download'),
    manage && button('Move…', () => moveDialog(ids, refresh)),
    manage && button('Remove…', async () => {
      if (await ask(`Remove ${counted(n)} from the library?`, 'The files stay in their folder. Numa Home leaves them out from now on, and their stars, flags and edits here are forgotten.', 'Remove from Library'))
        send('photos/remove', {}, refresh);
    }),
    manage && button('Delete…', async () => {
      if (await ask(`Delete ${counted(n)} from the disk?`, "They go to their library's trash, where they can be put back for 30 days; after that they are gone for good.", 'Delete from Disk', true))
        send('photos/delete', {}, answer => { toast(`${counted(answer.trashed)} in the trash`); refresh(); });
    }),
    h('button', { class: 'round small', 'aria-label': 'Clear the choice', title: 'Clear (Esc)', onclick: unpick }, icon('close')));
}

document.addEventListener('keydown', event => {
  if (state.viewer || event.target.matches?.('input, select, textarea') || document.querySelector('dialog[open]')) return;
  if (event.key === 'Escape' && (picked.ids.size || picked.mode)) unpick();
  else if (event.key === 'a' && (event.ctrlKey || event.metaKey) && state.sections.length) {
    event.preventDefault();
    state.sections.flatMap(section => section.photos || []).forEach(photo => picked.ids.add(photo.id));
    paintPicked();
  }
});
window.addEventListener('hashchange', unpick);

// A short word at the foot, gone in two seconds.
function toast(text) {
  document.querySelector('.toast')?.remove();
  const el = h('div', { class: 'toast', role: 'status' }, text);
  document.body.append(el);
  setTimeout(() => el.remove(), 2200);
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
  try { await api('photos/album', { method: 'POST', body: JSON.stringify(body) }); toast(asked.out ? 'Out of the album' : 'In the album'); } catch (error) { alert(error.message); }
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
  try { await setBy(ids, asked.account); toast(asked.account ? `By ${asked.account}` : 'As the camera says'); } catch (error) { alert(error.message); }
}

// The info panel's By: a choice of the household, or the camera's say.
async function byPicker(photo) {
  const users = await api('users').catch(() => []);
  const select = h('select', { class: 'by-pick', 'aria-label': 'Photographer', onchange: async () => {
    try { await setBy([photo.id], select.value); photo.by = select.value || null; photo.chosen = !!select.value; } catch (error) { alert(error.message); }
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
  } catch (error) { alert(error.message); }
}

// HOME-016: the Libraries page's own commands.
function libraryCommands(list) {
  const row = h('div', { class: 'filters' });
  if (state.me.admin) {
    row.append(h('button', { class: 'chip', onclick: newLibrary }, 'New Library…'), h('button', { class: 'chip', onclick: addFolder }, 'Add Folder…'));
  }
  if (state.me.admin || list.some(library => library.mine)) {
    row.append(h('button', { class: 'chip', onclick: () => go('/tidy') }, 'Tidy Up'), h('button', { class: 'chip', onclick: () => go('/trash') }, 'Trash'));
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
  } catch (error) { alert(error.message); }
}

async function addFolder() {
  let folders;
  try { folders = await api('libraries/folders'); } catch (error) { return alert(error.message); }
  if (!folders.length) return ask('Add Folder', 'Every folder in the photos folder is a library already. A folder copied in shows up here before the next look round, every quarter of an hour.', 'OK');
  const asked = await dialog('Add Folder', 'A folder put in the photos folder since Numa Home last looked, taken in now.',
    [h('select', { name: 'folder', 'aria-label': 'Folder' }, folders.map(one => h('option', { value: one.folder }, one.folder)))], 'Add');
  if (!asked) return;
  try { await api('libraries', { method: 'POST', body: JSON.stringify({ folder: asked.folder }) }); state.libraries = null; render(); } catch (error) { alert(error.message); }
}

function renameButton(library) {
  if (!state.me.admin || library.mine) return null;
  return h('button', { class: 'chip small', onclick: async () => {
    const asked = await dialog('Rename Library', 'The folder itself is renamed, in its place.', [h('input', { type: 'text', name: 'name', value: library.name, required: true, autocomplete: 'off' })], 'Rename');
    if (!asked) return;
    try { await api('libraries/' + library.id, { method: 'PUT', body: JSON.stringify({ name: asked.name.trim() }) }); state.libraries = null; render(); } catch (error) { alert(error.message); }
  } }, 'Rename…');
}

// HOME-017: the trash — what was deleted from the disk, for 30 days.
async function trashPage(main) {
  let list;
  try { list = await api('trash'); } catch (error) { return main.append(empty('Trash', error.message)); }
  if (!main.isConnected) return;
  if (!list.length) return main.append(empty('The Trash Is Empty', 'Photographs deleted from the disk wait here for 30 days, to be put back.'));
  const chosen = new Set();
  const restore = h('button', { class: 'primary', disabled: true, onclick: async () => {
    try { const answer = await api('trash/restore', { method: 'POST', body: JSON.stringify({ ids: [...chosen] }) }); toast(`${counted(answer.restored)} put back`); render(); } catch (error) { alert(error.message); }
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
