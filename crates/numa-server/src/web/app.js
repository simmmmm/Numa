// HOME-004: Numa Home's pages. No framework and no build step: the API, and
// the page built with `h`, never from strings of HTML, so nothing a file
// name or a person's name holds can become markup.
'use strict';

const app = document.getElementById('app');
const state = { me: null, sections: [], viewer: null, libraries: null, tints: new Map() };

function h(tag, props, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props || {})) {
    if (value == null || value === false) continue;
    if (key.startsWith('on')) el.addEventListener(key.slice(2), value);
    else if (key === 'class') el.className = value;
    else if (key === 'style') Object.assign(el.style, value);
    else if (typeof value === 'boolean') el[key] = value;
    else el.setAttribute(key, value);
  }
  for (const child of children.flat(Infinity)) if (child != null && child !== false) el.append(child instanceof Node ? child : String(child));
  return el;
}

// The few glyphs the pages draw, stroked as Numa's own icons are.
const ICONS = {
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.6v.2"/>',
  download: '<path d="M12 4v11M7 10l5 5 5-5M5 20h14"/>',
  back: '<path d="M15 5l-7 7 7 7"/>',
  on: '<path d="M9 5l7 7-7 7"/>',
  expand: '<path d="M14 5h5v5M10 19H5v-5M19 5l-6 6M5 19l6-6"/>',
  star: '<path d="M12 3.8l2.5 5.1 5.6.8-4 4 .9 5.6-5-2.6-5 2.6.9-5.6-4-4 5.6-.8z"/>',
  more: '<path d="M5.5 12h.01M12 12h.01M18.5 12h.01" stroke-width="3"/>',
};
function icon(name) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.innerHTML = ICONS[name];
  return svg;
}

// `wait`: seconds before giving up with a sentence, for a page that would
// otherwise stay empty while a busy NAS takes its time.
async function api(path, options = {}) {
  const headers = options.body ? { 'Content-Type': 'application/json' } : {};
  const signal = options.wait ? AbortSignal.timeout(options.wait * 1000) : undefined;
  let response;
  try { response = await fetch('/api/v1/' + path, { credentials: 'same-origin', signal, ...options, headers }); } catch (error) {
    if (error.name === 'TimeoutError') throw new Error('Numa Home is taking too long to answer; it may be busy. Try again in a moment.');
    throw new Error('Numa Home could not be reached. Is the NAS on and on this network?');
  }
  const text = await response.text();
  let body = null;
  try { body = text ? JSON.parse(text) : null; } catch (_) { body = null; }
  if (response.status === 401 && !['login', 'setup'].includes(path)) {
    signIn();
    throw new Error('Sign in first.');
  }
  if (!response.ok) throw new Error((body && body.error) || `The server answered ${response.status}.`);
  return body;
}

const grouped = n => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
const counted = n => `${grouped(n)} photo${n === 1 ? '' : 's'}`;
const thumb = (id, edge) => `/api/v1/photos/${id}/thumb?edge=${edge}`;
function monthName(month) {
  const [year, number] = month.split('-').map(Number);
  return new Date(Date.UTC(year, number - 1, 1)).toLocaleString('en-GB', { month: 'long', year: 'numeric', timeZone: 'UTC' });
}
// The camera's own clock, read as Numa reads it: as if it were UTC.
const when = seconds => new Date(seconds * 1000).toLocaleString('en-GB', {
  day: 'numeric', month: 'long', year: 'numeric', hour: '2-digit', minute: '2-digit', timeZone: 'UTC',
});

// Routes live in the hash: #/ (photos, narrowed by ?library=&person=&album=&q=),
// #/libraries, #/people, #/albums, #/accounts, #/activity, #/cameras,
// #/trash, #/import, #/tidy (?kind=).
function route() {
  const [path, query] = (location.hash.slice(1) || '/').split('?');
  return { path, params: new URLSearchParams(query || '') };
}
function go(path, params) {
  const query = new URLSearchParams(Object.entries(params || {}).filter(([, value]) => value != null && value !== '')).toString();
  location.hash = path + (query ? '?' + query : '');
}

async function start() {
  let status;
  try { status = await api('status'); } catch (error) {
    app.replaceChildren(h('div', { class: 'gate' }, h('p', { class: 'said' }, error.message)));
    return;
  }
  if (status.setup) return setup();
  try { state.me = await api('me'); } catch (_) { return; }
  render();
}

function gate(text, button, submit) {
  const name = h('input', { type: 'text', autocomplete: 'username', placeholder: 'Name', required: true, autocapitalize: 'words' });
  const password = h('input', {
    type: 'password', placeholder: 'Password', required: true,
    autocomplete: button === 'Sign In' ? 'current-password' : 'new-password',
  });
  const said = h('p', { class: 'said', role: 'status' });
  const form = h('form', {
    onsubmit: async event => {
      event.preventDefault();
      said.textContent = '';
      try { await submit(name.value.trim(), password.value); } catch (error) { said.textContent = error.message; }
    },
  }, h('img', { class: 'mark', src: '/mark.png', alt: '' }), h('h1', {}, 'Numa Home'), h('p', { class: 'body' }, text),
  name, password, h('button', { class: 'primary', type: 'submit' }, button), said);
  app.replaceChildren(h('div', { class: 'gate' }, form));
  name.focus();
}

function setup() {
  gate('Make the first account. It can make one for everyone else in the house.', 'Make Account', async (name, password) => {
    await api('setup', { method: 'POST', body: JSON.stringify({ name, password }) });
    await signInAs(name, password);
  });
}

function signIn() {
  state.me = null;
  closeViewer();
  gate("The household's photographs, as Numa shows them.", 'Sign In', signInAs);
}

async function signInAs(name, password) {
  const me = await api('login', { method: 'POST', body: JSON.stringify({ name, password, device: 'Browser' }) });
  state.me = { name: me.name, admin: me.admin };
  state.libraries = null;
  render();
}

async function libraries() {
  state.libraries = state.libraries || await api('libraries');
  return state.libraries;
}
const libraryName = library => library.mine ? 'Sent Home' : library.name;

window.addEventListener('hashchange', () => { if (state.me) render(); });

function render() {
  const { path, params } = route();
  closeViewer();
  const main = h('main');
  app.replaceChildren(header(path, params), placesNav(path, params), h('div', { class: 'cardbar', hidden: true }), main);
  cardBar();
  activityBadge();
  if (path === '/activity') return activityPage(main);
  if (path === '/import') return importPage(main);
  if (path === '/trash') return trashPage(main);
  if (path === '/tidy') return tidyPage(main, params);
  if (path === '/people') return people(main);
  if (path === '/albums') return albums(main);
  if (path === '/libraries') return shelves(main);
  if (path === '/accounts' && state.me.admin) return accounts(main);
  if (path === '/cameras' && state.me.admin) return cameras(main);
  if (params.get('q')) return search(main, params.get('q'));
  return timeline(main, params);
}

function header(path, params) {
  const field = h('input', { type: 'search', placeholder: 'Search by names or words', value: params.get('q') || '', enterkeyhint: 'search', 'aria-label': 'Search' });
  const search = h('form', { class: 'search', role: 'search', onsubmit: event => { event.preventDefault(); go('/', { q: field.value.trim() }); } }, field);
  const bar = h('header', {});
  const who = h('button', { class: 'chip who', 'aria-haspopup': 'menu', onclick: event => {
    event.stopPropagation();
    menu(bar, [
      h('div', { class: 'section-label' }, state.me.name),
      h('button', { onclick: async () => { await api('logout', { method: 'POST' }).catch(() => {}); signIn(); } }, 'Sign Out'),
    ]);
  } }, state.me.name);
  bar.append(h('img', { class: 'mark', src: '/mark.png', alt: 'Numa Home', onclick: () => go('/') }), badgeButton(), search, who);
  return bar;
}

// Every place in one list: down the side on a wide screen, along the foot
// on a phone, where the four first are tabs and the rest are under More.
const OWNERS = { library: '/libraries', album: '/albums', person: '/people' };
function placesNav(path, params) {
  const owner = path === '/' && Object.keys(OWNERS).find(key => params.get(key));
  const here = owner ? OWNERS[owner] : path;
  const place = (label, to, extra) => h('button', { class: 'place-link' + (here === to ? ' chosen' : '') + (extra ? ' extra' : ''),
    'aria-current': here === to ? 'page' : null, onclick: () => go(to) }, label);
  const rest = [
    h('div', { class: 'section-label extra' }, 'Archive'),
    place('Tidy Up', '/tidy', true), place('Trash', '/trash', true), place('Activity', '/activity', true),
    state.me.admin && h('div', { class: 'section-label extra' }, 'Household'),
    state.me.admin && place('Accounts', '/accounts', true), state.me.admin && place('Cameras', '/cameras', true),
  ];
  const nav = h('nav', { class: 'places-nav', 'aria-label': 'Places' },
    place('Photos', '/'), place('Libraries', '/libraries'), place('People', '/people'), place('Albums', '/albums'), rest);
  const extras = ['/tidy', '/trash', '/activity', '/accounts', '/cameras'];
  nav.append(h('button', { class: 'place-link more' + (extras.includes(here) ? ' chosen' : ''), 'aria-haspopup': 'menu', onclick: event => {
    event.stopPropagation();
    menu(nav, rest.filter(Boolean).map(one => one.tagName === 'BUTTON'
      ? h('button', { class: one.classList.contains('chosen') ? 'chosen' : null, onclick: () => one.click() }, one.textContent)
      : h('div', { class: 'section-label' }, one.textContent)));
  } }, 'More'));
  return nav;
}

// A menu under its place, gone at the next click anywhere.
function menu(parent, items) {
  parent.querySelector('.menu')?.remove();
  const box = h('div', { class: 'menu', role: 'menu' }, items);
  parent.append(box);
  setTimeout(() => document.addEventListener('click', () => box.remove(), { once: true }));
}

// A menu at a point of the screen, kept inside it: where the pointer is for a
// right-click, or above its button in a selection bar, whose own box would
// clip it.
function menuAt(x, y, items, above) {
  document.querySelector('body > .menu')?.remove();
  const box = h('div', { class: 'menu floating', role: 'menu' }, items);
  document.body.append(box);
  const { width, height } = box.getBoundingClientRect();
  box.style.left = Math.max(8, Math.min(above ? x - width / 2 : x, innerWidth - width - 8)) + 'px';
  box.style.top = Math.max(8, Math.min(above ? y - height - 8 : y, innerHeight - height - 8)) + 'px';
  setTimeout(() => document.addEventListener('click', () => box.remove(), { once: true }));
}
const menuOver = (button, items) => {
  const box = button.getBoundingClientRect();
  menuAt(box.left + box.width / 2, box.top, items, true);
};

function empty(title, text) {
  return h('div', { class: 'empty' }, h('h2', {}, title), h('p', { class: 'body' }, text));
}

const rowHeight = () => innerWidth < 720 ? 110 : 200;
const GAP = 3;

// The timeline: every month's count at once, a month's photographs when it
// comes near the screen, its rows justified to the width.
async function timeline(main, params) {
  const narrowed = ['library', 'person', 'album', 'thing', 'by'].filter(key => params.get(key)).map(key => [key, params.get(key)]);
  const query = new URLSearchParams(narrowed).toString();
  // The months asked at once, beside the filters, not after them.
  const asked = api('timeline' + (query ? '?' + query : '')).then(months => ({ months }), error => ({ error }));
  const filters = await filterRow(params, narrowed);
  main.append(...[filters.head, filters.row.childElementCount > 0 && filters.row].filter(Boolean));
  const { months, error } = await asked;
  if (error) {
    main.append(empty('Nothing to Show', error.message));
    return;
  }
  if (!main.isConnected) return;
  filters.count.textContent = counted(months.reduce((sum, month) => sum + month.count, 0));
  if (months.length) filters.dates.textContent = ' · ' + spanOf(months);
  if (!months.length) {
    main.append(narrowed.length
      ? empty('Nothing Here', 'No photographs answer these filters.')
      : empty('No Photographs Yet', 'Put folders of photographs in the photos folder, or send them home from Numa. Numa never moves or changes them.'));
    return;
  }
  state.sections = months.map(month => monthSection(month, query));
  main.append(...state.sections.map(section => section.el));
  // HOME-011: the scrubber, once there is more than a screen to scrub.
  if (months.length > 1) {
    main.classList.add('scrubbed');
    app.append(scrubber(months, state.sections));
  }
  const near = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      // Kept watching until it has loaded: a month that failed (the server
      // restarting) is asked again when it next comes near.
      state.sections.find(section => section.el === entry.target)?.load().then(() => near.unobserve(entry.target), () => {});
    }
  }, { rootMargin: '1500px 0px' });
  state.sections.forEach(section => near.observe(section.el));
}

// A library, an album or a person is a page of its own: its name as the
// title, how many and when, what can be done with it, and the way back. The
// filters under it narrow what it holds.
async function filterRow(params, narrowed) {
  const row = h('div', { class: 'filters' });
  const without = key => Object.fromEntries(narrowed.filter(([other]) => other !== key));
  const owner = Object.keys(OWNERS).find(key => params.get(key));
  // Asked at once; the row waits for the slowest, not for their sum.
  const asked = {
    libraries: libraries().catch(() => []), by: api('by' + byQuery(narrowed)).catch(() => []), things: api('things').catch(() => []),
    albums: params.get('album') ? api('albums').catch(() => []) : [],
  };
  const list = await asked.libraries;
  const library = list.find(one => String(one.id) === params.get('library'));
  const names = {
    library: library ? libraryName(library) : 'Library',
    album: (await asked.albums).find(album => album.key === params.get('album'))?.name || 'Album',
    person: params.get('person'),
  };
  if (!owner) {
    const select = h('select', { 'aria-label': 'Libraries', onchange: () => go('/', { ...without('library'), library: select.value }) },
      h('option', { value: '' }, 'All Libraries'), list.map(one => h('option', { value: one.id }, libraryName(one))));
    row.append(select);
  }
  const chip = (text, key) => h('button', { class: 'chip chosen', onclick: () => go('/', without(key)) }, text, h('span', { class: 'x', 'aria-label': 'Remove' }, '×'));
  for (const key of ['library', 'person', 'album']) if (key !== owner && params.get(key)) row.append(chip(names[key], key));
  // HOME-012: whose photographs — a chip per account, and Unknown.
  const by = await asked.by;
  if (by.some(row => row.account) && by.length > 1) {
    row.append(h('span', { class: 'section-label by-label' }, 'By'), ...by.map(({ account }) => {
      const name = account || 'Unknown';
      const chosen = params.get('by') === name;
      return h('button', { class: 'chip' + (chosen ? ' chosen' : ''), onclick: () => go('/', chosen ? without('by') : { ...without('by'), by: name }) }, name);
    }));
  }
  // LIB-029: what Numa saw by itself, in italic, so it reads as Numa's.
  if (params.get('thing')) row.append(h('button', { class: 'chip numa-saw', onclick: () => go('/', without('thing')) }, params.get('thing'), h('span', { class: 'x', 'aria-label': 'Remove' }, '×')));
  else {
    const things = await asked.things;
    if (things.length) {
      const seen = h('select', { class: 'numa-saw', 'aria-label': 'Things Numa Saw', onchange: () => go('/', { ...without('thing'), thing: seen.value }) },
        h('option', { value: '' }, 'Things Numa Saw'), things.map(one => h('option', { value: one.thing }, `${one.thing} · ${grouped(one.count)}`)));
      row.append(seen);
    }
  }
  const count = h('span', { class: 'count' });
  const dates = h('span', { class: 'dates' });
  // HOME-023: a trip is a library or an album; its picks make the book.
  const book = (params.get('library') || params.get('album')) &&
    h('button', { class: 'chip', title: 'The picks here, as a photo book\'s chapter folders', onclick: () => bookDialog(new URLSearchParams(narrowed).toString()) }, 'For a Book…');
  if (!owner) {
    count.className = 'caption count';
    row.append(...[book, selectToggle(), count].filter(Boolean));
    return { row, count, dates };
  }
  const back = { library: 'Libraries', album: 'Albums', person: 'People' }[owner];
  const head = h('div', { class: 'collection-head' },
    h('button', { class: 'round small', 'aria-label': back, title: back, onclick: () => go(OWNERS[owner]) }, icon('back')),
    h('div', { class: 'titles' }, h('h1', {}, names[owner]),
      h('div', { class: 'caption' }, count, dates)),
    h('div', { class: 'act' }, ...[book,
      owner === 'library' && library && state.me.admin && !library.mine && h('button', { class: 'chip', onclick: () => renameLibrary(library) }, 'Rename…'),
      selectToggle()].filter(Boolean)));
  return { head, row, count, dates };
}

// The By chips count within the page's other filters, so a library names
// only the people with photographs in it.
function byQuery(narrowed) {
  const query = new URLSearchParams(narrowed.filter(([key]) => key !== 'by')).toString();
  return query ? '?' + query : '';
}

// When a page's photographs were taken: "Aug 2026", "Mar – Aug 2026",
// "Dec 2025 – Jan 2026".
function spanOf(months) {
  const sorted = months.map(month => month.month).sort();
  const name = (month, year) => {
    const [y, m] = month.split('-').map(Number);
    return new Date(Date.UTC(y, m - 1, 1)).toLocaleString('en-GB', { month: 'short', ...(year && { year: 'numeric' }), timeZone: 'UTC' });
  };
  const [first, last] = [sorted[0], sorted[sorted.length - 1]];
  if (first === last) return name(last, true);
  return `${name(first, first.slice(0, 4) !== last.slice(0, 4))} – ${name(last, true)}`;
}

function estimate(count, width) {
  const perRow = Math.max(1, Math.floor(width / (1.4 * rowHeight())));
  return Math.ceil(count / perRow) * (rowHeight() + GAP);
}

function monthSection(month, query) {
  const rows = h('div', { class: 'rows' });
  const caption = h('span', { class: 'caption' }, counted(month.count));
  const el = h('section', { class: 'month' },
    h('div', { class: 'head' }, h('span', { class: 'section-label' }, monthName(month.month)), caption),
    rows);
  const section = { el, rows, count: month.count, photos: null, loading: null };
  requestAnimationFrame(() => { if (!section.photos) rows.style.height = estimate(month.count, rows.clientWidth || innerWidth) + 'px'; });
  section.load = () => section.loading = section.loading || api(`timeline/${month.month}` + (query ? '?' + query : '')).then(list => {
    section.photos = list;
    // Asked again after a change (`refreshAround`), the month may have shrunk.
    section.count = list.length;
    caption.textContent = counted(list.length);
    el.hidden = !list.length;
    layout(section);
    return list;
  }, error => { section.loading = null; throw error; });
  return section;
}

// Justified rows: photographs side by side until the row is as wide as the
// page at no more than the row height; the last row keeps the height.
function layout(section) {
  const width = section.rows.clientWidth;
  if (!width || !section.photos) return;
  const tall = rowHeight();
  const tiles = [];
  let row = [], sum = 0, top = 0;
  const place = (photos, height) => {
    let left = 0;
    for (const photo of photos) {
      const wide = aspect(photo) * height;
      tiles.push(tile(section, photo, { left: left + 'px', top: top + 'px', width: wide + 'px', height: height + 'px' }, height));
      left += wide + GAP;
    }
    top += height + GAP;
  };
  for (const photo of section.photos) {
    row.push(photo);
    sum += aspect(photo);
    const height = (width - GAP * (row.length - 1)) / sum;
    if (height <= tall) { place(row, height); row = []; sum = 0; }
  }
  if (row.length) place(row, tall);
  section.rows.style.height = Math.max(0, top - GAP) + 'px';
  section.rows.replaceChildren(...tiles);
}
const aspect = photo => Math.min(4, Math.max(0.3, photo.aspect || 1.5));

function tile(section, photo, style, height) {
  const edge = height * (devicePixelRatio || 1) > 330 ? 512 : 256;
  const image = h('img', { src: thumb(photo.id, edge), loading: 'lazy', decoding: 'async', alt: '' });
  image.addEventListener('load', () => image.classList.add('in'));
  // HOME-017: a click chooses once something is chosen (`pick`).
  return holdToChoose(h('button', { class: 'tile' + (picked.ids.has(photo.id) ? ' picked' : ''), style, 'data-id': photo.id, 'aria-label': 'Photograph',
    onclick: event => pick(event, photo.id) || openViewer(section, section.photos.indexOf(photo)) },
  image, h('span', { class: 'check', 'aria-hidden': 'true' }), photo.rating > 0 && h('span', { class: 'stars' }, '★'.repeat(photo.rating))), photo.id);
}

let resizing;
window.addEventListener('resize', () => {
  clearTimeout(resizing);
  resizing = setTimeout(() => state.sections.forEach(section => section.photos && section.rows && layout(section)), 150);
});

// LIB-028: names narrow, the rest is read by the model; best first.
async function search(main, q) {
  const count = h('span', { class: 'caption count' });
  main.append(h('div', { class: 'filters' },
    h('button', { class: 'chip chosen', onclick: () => go('/') }, q, h('span', { class: 'x', 'aria-label': 'Remove' }, '×')), selectToggle(), count));
  let found;
  try { found = await api('search?q=' + encodeURIComponent(q)); } catch (error) {
    main.append(empty('Search Waits', error.message));
    return;
  }
  if (!main.isConnected) return;
  count.textContent = counted(found.length);
  if (!found.length) return main.append(empty('Nothing Found', 'Try other words, or a name.'));
  const section = { photos: found.map(one => ({ id: one.id })), load: async () => section.photos };
  state.sections = [section];
  main.append(h('div', { class: 'grid' }, section.photos.map((photo, at) => {
    const image = h('img', { src: thumb(photo.id, 256), loading: 'lazy', decoding: 'async', alt: '' });
    image.addEventListener('load', () => image.classList.add('in'));
    return holdToChoose(h('button', { class: 'tile', 'data-id': photo.id, 'aria-label': 'Photograph', onclick: event => pick(event, photo.id) || openViewer(section, at) },
      image, h('span', { class: 'check', 'aria-hidden': 'true' })), photo.id);
  })));
}

async function people(main) {
  let list, unnamed;
  try { [list, unnamed] = await Promise.all([api('people'), api('faces').catch(() => [])]); } catch (error) { return main.append(empty('No People', error.message)); }
  if (!main.isConnected) return;
  if (!list.length && !unnamed.length) {
    return main.append(empty('No People Yet', 'People show here once Numa has found the faces in the photographs. Name them here, or in Numa.'));
  }
  if (list.length) {
    main.append(h('div', { class: 'places' }, list.map(person => h('button', { class: 'place', onclick: () => go('/', { person: person.name }) },
      h('div', { class: 'face' }, h('img', { src: thumb(person.portrait, 256), loading: 'lazy', alt: '' })),
      h('div', {}, h('div', { class: 'name' }, person.name), h('div', { class: 'caption' }, counted(person.count)))))));
  }
  if (unnamed.length) main.append(h('div', { class: 'section-label unnamed-label' }, 'Not named yet'), h('div', { class: 'unnamed' }, unnamed.map(nameRow)));
}

// LIB-014: a group of faces nobody has named — who is this, or nobody.
function nameRow(group) {
  const field = h('input', { type: 'text', placeholder: 'Who is this?', 'aria-label': 'Name', autocapitalize: 'words' });
  const said = h('span', { class: 'caption' });
  const send = async (path, body) => {
    try { await api(path, { method: 'POST', body: JSON.stringify(body) }); render(); } catch (error) { said.textContent = error.message; }
  };
  const portraits = group.portraits.map(({ face, photo }) => {
    const image = h('img', { src: `/api/v1/faces/${face}/portrait`, loading: 'lazy', alt: '' });
    image.addEventListener('error', () => { image.src = thumb(photo, 256); }, { once: true });
    return h('div', { class: 'face small' }, image);
  });
  return h('form', { class: 'name-row', onsubmit: event => { event.preventDefault(); if (field.value.trim()) send('faces/name', { faces: group.faces, name: field.value.trim() }); } },
    h('div', { class: 'faces' }, portraits),
    h('div', { class: 'ask' }, h('div', { class: 'caption' }, `In ${grouped(group.photos)} photographs`), field, said),
    h('button', { class: 'chip', type: 'submit' }, 'Name'),
    h('button', { class: 'chip', type: 'button', title: 'Nobody to name — stop asking about these faces', onclick: () => send('faces/ignore', { faces: group.faces }) }, 'Nobody'));
}

// A stack of three prints, from the first photographs the filter finds.
function stack(query) {
  const el = h('div', { class: 'stack' }, h('div', { class: 'print' }), h('div', { class: 'print' }), h('div', { class: 'print' }));
  (async () => {
    const ids = [];
    for (const month of await api('timeline?' + query)) {
      if (ids.length >= 3) break;
      ids.push(...(await api(`timeline/${month.month}?` + query)).slice(0, 3 - ids.length).map(photo => photo.id));
    }
    // The front print is the first photograph; the two behind it the next.
    ids.forEach((id, at) => el.children[2 - at].append(h('img', { src: thumb(id, 256), loading: 'lazy', alt: '' })));
  })().catch(() => {});
  return el;
}

function collection(name, caption, query, open) {
  return h('button', { class: 'place', onclick: open }, stack(query),
    h('div', {}, h('div', { class: 'name' }, name), h('div', { class: 'caption' }, caption)));
}

async function shelves(main) {
  const list = await libraries().catch(error => (main.append(empty('No Libraries', error.message)), null));
  if (!list) return;
  if (!list.length) return main.append(empty('No Libraries Yet', 'Every folder in the photos folder is a library; so is what each person sends home.'));
  // HOME-011: each library's tint, as the scrubber has it.
  state.tints = tintsOf(await api('timeline').catch(() => []));
  // An empty Sent Home is only a grey stack; it shows once something is sent.
  const shown = list.filter(library => library.count || !library.mine);
  main.append(libraryCommands(list), h('div', { class: 'places' }, shown.map(library => libraryShelf(library, list))));
}

async function albums(main) {
  let list;
  try { list = await api('albums'); } catch (error) { return main.append(empty('No Albums', error.message)); }
  if (!list.length) return main.append(empty('No Albums Yet', 'Choose photographs and Album… to start one. Albums made in Numa show here too.'));
  main.append(h('div', { class: 'places' }, list.map(album =>
    collection(album.name, counted(album.count), 'album=' + encodeURIComponent(album.key), () => go('/', { album: album.key })))));
}

async function accounts(main) {
  const users = await api('users').catch(error => (main.append(empty('Accounts', error.message)), null));
  if (!users) return;
  const name = h('input', { type: 'text', placeholder: 'Name', required: true, autocomplete: 'off' });
  const password = h('input', { type: 'password', placeholder: 'Password, 8 characters or more', required: true, autocomplete: 'new-password' });
  const admin = h('input', { type: 'checkbox' });
  const said = h('p', { class: 'said', role: 'status', style: { textAlign: 'left' } });
  const form = h('form', { onsubmit: async event => {
    event.preventDefault();
    try {
      await api('users', { method: 'POST', body: JSON.stringify({ name: name.value.trim(), password: password.value, admin: admin.checked }) });
      render();
    } catch (error) { said.textContent = error.message; }
  } }, h('div', { class: 'section-label' }, 'New account'), name, password,
  h('label', { class: 'check' }, admin, 'Can make accounts too'), h('button', { class: 'primary', type: 'submit' }, 'Make Account'), said);
  main.append(h('div', { class: 'accounts' },
    h('div', {}, h('div', { class: 'section-label' }, 'Accounts'),
      h('ul', {}, users.map(user => h('li', {}, h('span', {}, user.name), h('span', { class: 'caption' }, user.admin ? 'Admin' : ''))))),
    form,
    h('p', { class: 'body' }, 'Everyone sees the libraries in the photos folder. What a person sends home is theirs alone.')));
}

// The photograph large: arrow keys, a swipe on a phone, its information, and
// the file to take away.
function openViewer(section, at) {
  closeViewer();
  const image = h('img', { alt: '', draggable: 'false' });
  const info = h('aside', { class: 'info', hidden: true });
  const hud = h('div', { class: 'hud', hidden: true });
  const infoButton = h('button', { class: 'round', 'aria-label': 'Info', title: 'Info (I)', onclick: () => toggleInfo() }, icon('info'));
  // HOME-013: the photograph in Numa on this computer or device, through
  // the link both clients take (docs/PLAN_HOME.md). No Numa on Android.
  const inNuma = !/Android/i.test(navigator.userAgent) && h('a', {
    class: 'chip open-in-numa', title: 'Open this photograph in Numa, signed in to this Numa Home',
    onclick: event => { event.currentTarget.href = `numa://home/open?server=${encodeURIComponent(location.origin)}&photo=${current().id}`; },
  }, 'Open in Numa');
  const corner = h('div', { class: 'corner right' }, inNuma, infoButton,
    h('button', { class: 'round', 'aria-label': 'Download', onclick: event => {
      event.stopPropagation();
      const id = current().id;
      menu(corner, [
        h('a', { href: `/api/v1/photos/${id}/original`, download: '' }, 'Original'),
        h('a', { href: `/api/v1/photos/${id}/jpeg`, download: '' }, 'As JPEG'),
      ]);
    } }, icon('download')));
  const stage = h('div', { class: 'stage' }, image, hud,
    h('div', { class: 'corner left' }, h('button', { class: 'round', 'aria-label': 'Close', title: 'Close (Esc)', onclick: () => history.back() }, icon('close'))),
    corner,
    h('button', { class: 'round step back', 'aria-label': 'Previous', title: 'Previous (←)', onclick: () => step(-1) }, icon('back')),
    h('button', { class: 'round step on', 'aria-label': 'Next', title: 'Next (→)', onclick: () => step(1) }, icon('on')));
  const el = h('div', { class: 'viewer', role: 'dialog', 'aria-label': 'Photograph' }, stage, info);
  document.body.append(el);
  document.body.style.overflow = 'hidden';
  // A history entry of its own, so Back closes it; marked, so an entry left
  // by a viewer before a reload is not taken for this one.
  const mark = Date.now();
  state.viewer = { sections: state.sections, si: state.sections.indexOf(section), pi: at, el, stage, image, info, hud, infoButton, mark };
  history.pushState({ viewer: mark }, '');
  handle(stage, image);
  image.addEventListener('load', () => { if (!state.viewer.zoom.full) fit(); });
  show();
}

const current = () => state.viewer.sections[state.viewer.si].photos[state.viewer.pi];

function show() {
  const viewer = state.viewer;
  const photo = current();
  viewer.zoom = { s: 1, tx: 0, ty: 0, full: false, aspect: null };
  viewer.detail = null;
  apply();
  // The thumbnail at once, the 2048 preview over it when it has come.
  viewer.image.src = thumb(photo.id, 512);
  const large = new Image();
  large.onload = () => { if (state.viewer === viewer && current() === photo && !viewer.zoom.full) viewer.image.src = large.src; };
  large.src = `/api/v1/photos/${photo.id}/preview`;
  api('photos/' + photo.id).then(detail => {
    if (state.viewer !== viewer || current() !== photo) return;
    viewer.detail = detail;
    if (!viewer.info.hidden) fillInfo();
  }).catch(() => {});
}

// The photograph at its fitted size, laid out by hand so zooming and
// panning are a transform of a box that is exactly the picture.
function fit() {
  const viewer = state.viewer;
  const { image, stage } = viewer;
  if (!image.naturalWidth) return;
  viewer.zoom.aspect = viewer.zoom.aspect || image.naturalWidth / image.naturalHeight;
  const W = stage.clientWidth, H = stage.clientHeight, aspect = viewer.zoom.aspect;
  const width = Math.min(W, H * aspect);
  image.style.width = width + 'px';
  image.style.height = width / aspect + 'px';
  clamp();
  apply();
}

function apply() {
  const { image, zoom } = state.viewer;
  image.style.transform = `translate(${zoom.tx}px, ${zoom.ty}px) scale(${zoom.s})`;
  state.viewer.stage.classList.toggle('zoomed', zoom.s > 1.001);
}

// Zoomed, the picture covers the stage where it is larger than it, and is
// centred where it is not.
function clamp() {
  const { image, stage, zoom } = state.viewer;
  const sides = [[stage.clientWidth, image.offsetLeft, image.offsetWidth, 'tx'], [stage.clientHeight, image.offsetTop, image.offsetHeight, 'ty']];
  for (const [room, at, base, key] of sides) {
    const size = base * zoom.s;
    zoom[key] = size <= room ? (base - size) / 2 : Math.min(-at, Math.max(room - size - at, zoom[key]));
  }
}

// 1:1: one pixel of the photograph to one of the screen.
function actual() {
  const { image, detail, zoom } = state.viewer;
  const sides = detail?.size?.match(/\d+/g)?.map(Number);
  const long = zoom.full ? Math.max(image.naturalWidth, image.naturalHeight) : sides ? Math.max(...sides) : 2 * Math.max(image.naturalWidth, image.naturalHeight);
  return Math.max(1.5, long / (devicePixelRatio || 1) / Math.max(image.offsetWidth, image.offsetHeight));
}

// Around a point of the stage (clientX, clientY): that point stays put.
function zoomTo(s, clientX, clientY) {
  const { image, stage, zoom } = state.viewer;
  const box = stage.getBoundingClientRect();
  const x = clientX - box.left - image.offsetLeft, y = clientY - box.top - image.offsetTop;
  const scale = Math.min(Math.max(1, s), actual() * 2);
  const k = scale / zoom.s;
  zoom.tx = x - k * (x - zoom.tx);
  zoom.ty = y - k * (y - zoom.ty);
  zoom.s = scale;
  clamp();
  apply();
  if (scale > 1.001 && !zoom.full) wholePicture();
}

function toggleZoom(clientX, clientY) {
  if (state.viewer.zoom.s > 1.001) zoomTo(1, clientX, clientY);
  else zoomTo(actual(), clientX, clientY);
}

// Zoomed in, the whole photograph as a JPEG with its edits, swapped in when
// it has come; the preview stands in, enlarged, meanwhile.
function wholePicture() {
  const viewer = state.viewer;
  const photo = current();
  viewer.zoom.full = true;
  const whole = new Image();
  whole.onload = () => { if (state.viewer === viewer && current() === photo) viewer.image.src = whole.src; };
  whole.src = `/api/v1/photos/${photo.id}/jpeg`;
}

// Double-click or double-tap for 1:1 where it was, the wheel or a pinch to
// zoom, a drag to pan; at fit, a swipe to the next. The fingers are taken
// anywhere on the stage, not only on the picture: a second finger on the
// dark band beside a tall photograph is still a pinch.
function handle(stage, image) {
  const pointers = new Map();
  let pinch = null, swipe = null, tapped = null, touched = 0;
  // A touch screen's double-tap is the taps' to handle; the browser's own
  // dblclick after it would zoom straight back out.
  stage.addEventListener('dblclick', event => { if (event.target === image && Date.now() - touched > 800) toggleZoom(event.clientX, event.clientY); });
  stage.addEventListener('wheel', event => {
    event.preventDefault();
    const zoom = state.viewer.zoom;
    // Lines (Firefox) and pages, as pixels.
    const delta = event.deltaY * [1, 16, innerHeight][event.deltaMode || 0];
    zoomTo(zoom.s * Math.exp(-delta * 0.0015), event.clientX, event.clientY);
  }, { passive: false });
  // Safari's own page zoom, which touch-action alone does not always stop.
  stage.addEventListener('gesturestart', event => event.preventDefault());
  const mid = () => {
    const [a, b] = [...pointers.values()];
    return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2, distance: Math.hypot(a.x - b.x, a.y - b.y) };
  };
  stage.addEventListener('pointerdown', event => {
    if (event.target.closest('button, a, .menu, .info') || event.button > 0) return;
    if (event.pointerType !== 'mouse') touched = Date.now();
    try { stage.setPointerCapture(event.pointerId); } catch (_) { /* a synthetic pointer */ }
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pointers.size === 2) pinch = { ...mid(), s: state.viewer.zoom.s };
    else if (pointers.size === 1) swipe = { x: event.clientX, y: event.clientY, on: event.target === image };
  });
  stage.addEventListener('pointermove', event => {
    const was = pointers.get(event.pointerId);
    if (!was) return;
    const zoom = state.viewer.zoom;
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pinch && pointers.size === 2) {
      // The picture follows the fingers' middle, and scales around it.
      const now = mid();
      zoom.tx += now.x - pinch.x;
      zoom.ty += now.y - pinch.y;
      Object.assign(pinch, { x: now.x, y: now.y });
      zoomTo(pinch.s * now.distance / pinch.distance, now.x, now.y);
    } else if (pointers.size === 1 && !pinch && zoom.s > 1.001) {
      zoom.tx += event.clientX - was.x;
      zoom.ty += event.clientY - was.y;
      clamp();
      apply();
    }
  });
  const up = event => {
    if (!pointers.delete(event.pointerId)) return;
    if (pointers.size) return;
    const zoom = state.viewer.zoom;
    const moved = swipe ? Math.hypot(event.clientX - swipe.x, event.clientY - swipe.y) : Infinity;
    if (swipe && !pinch && event.pointerType !== 'mouse') {
      const dx = event.clientX - swipe.x, dy = event.clientY - swipe.y;
      if (zoom.s <= 1.001 && Math.abs(dx) > 50 && Math.abs(dx) > 1.5 * Math.abs(dy)) step(dx < 0 ? 1 : -1);
      else if (moved < 10 && swipe.on) {
        // A second tap soon after the first, near it, is a double-tap.
        const now = Date.now();
        if (tapped && now - tapped.at < 350 && Math.hypot(event.clientX - tapped.x, event.clientY - tapped.y) < 40) {
          toggleZoom(event.clientX, event.clientY);
          tapped = null;
        } else tapped = { at: now, x: event.clientX, y: event.clientY };
      }
    }
    if (event.pointerType !== 'mouse') touched = Date.now();
    pinch = null;
    swipe = null;
  };
  stage.addEventListener('pointerup', up);
  stage.addEventListener('pointercancel', up);
  // The info panel opening, a phone turned, the window resized: fitted again.
  new ResizeObserver(() => { if (state.viewer?.stage === stage) fit(); }).observe(stage);
}

async function step(by) {
  const viewer = state.viewer;
  if (!viewer || viewer.busy) return;
  viewer.busy = true;
  try {
    let si = viewer.si, pi = viewer.pi + by;
    for (;;) {
      const section = viewer.sections[si];
      if (!section) return;
      if (!section.photos) await section.load();
      if (pi < 0) {
        if (--si < 0) return;
        const before = viewer.sections[si];
        if (!before.photos) await before.load();
        pi = before.photos.length - 1;
        continue;
      }
      if (pi >= section.photos.length) {
        if (++si >= viewer.sections.length) return;
        pi = 0;
        continue;
      }
      break;
    }
    if (state.viewer !== viewer) return;
    viewer.si = si;
    viewer.pi = pi;
    show();
  } finally { viewer.busy = false; }
}

const FLAGS = { pick: 'Pick', reject: 'Reject', none: 'No Flag' };

// Stars and the flag, the household's as the catalog's are: on the info
// panel, the tiles, and for a moment in the HUD.
async function setMarks(change) {
  const viewer = state.viewer;
  const detail = viewer.detail;
  if (!detail) return;
  const marks = { rating: detail.rating, flag: detail.flag, ...change };
  try {
    await api(`photos/${detail.id}/marks`, { method: 'PUT', body: JSON.stringify(marks) });
  } catch (error) { return flash('Not Saved', error.message); }
  Object.assign(detail, marks);
  for (const section of viewer.sections) section.photos?.filter(photo => photo.id === detail.id).forEach(photo => { photo.rating = marks.rating; });
  for (const tile of document.querySelectorAll(`.tile[data-id="${detail.id}"]`)) {
    tile.querySelector('.stars')?.remove();
    if (marks.rating > 0) tile.append(h('span', { class: 'stars' }, '★'.repeat(marks.rating)));
  }
  if ('rating' in change) flash('Rating', marks.rating ? '★'.repeat(marks.rating) : 'No Rating');
  else flash('Flag', FLAGS[marks.flag]);
  if (!viewer.info.hidden) fillInfo();
}

// The floating value: one name, one value, gone 0.9 s after.
function flash(name, value) {
  const hud = state.viewer.hud;
  hud.replaceChildren(h('span', { class: 'section-label' }, name), h('span', { class: 'value' }, value));
  hud.hidden = false;
  clearTimeout(hud.timer);
  hud.timer = setTimeout(() => { hud.hidden = true; }, 900);
}

function toggleInfo() {
  const viewer = state.viewer;
  viewer.info.hidden = !viewer.info.hidden;
  viewer.infoButton.classList.toggle('chosen', !viewer.info.hidden);
  if (!viewer.info.hidden) fillInfo();
}

async function fillInfo() {
  const viewer = state.viewer;
  const photo = viewer.detail;
  if (!photo) return;
  const library = (await libraries().catch(() => [])).find(one => one.id === photo.library);
  const albums = photo.albums.length ? await api('albums').catch(() => []) : [];
  const by = await byPicker(photo);
  if (state.viewer !== viewer || viewer.detail !== photo) return;
  const label = text => h('div', { class: 'section-label' }, text);
  const chips = (names, open) => h('div', { class: 'chips' }, names.map(name => h('button', { class: 'chip', onclick: () => open(name) }, name)));
  const stars = h('div', { class: 'marks' }, [1, 2, 3, 4, 5].map(n => h('button', {
    class: 'star' + (n <= photo.rating ? ' set' : ''), 'aria-label': `${n} star${n > 1 ? 's' : ''}`, title: `${n} (${n})`,
    onclick: () => setMarks({ rating: photo.rating === n ? 0 : n }),
  }, '★')));
  const flags = h('div', { class: 'chips' }, ['pick', 'reject'].map(flag => h('button', {
    class: 'chip' + (photo.flag === flag ? ' chosen' : ''), title: `${FLAGS[flag]} (${flag === 'pick' ? 'P' : 'X'}); no flag (U)`,
    onclick: () => setMarks({ flag: photo.flag === flag ? 'none' : flag }),
  }, FLAGS[flag])));
  // `replaceChildren` would write a `false` out as text; `h` leaves it out.
  viewer.info.replaceChildren(...[
    h('div', { class: 'title' }, photo.name),
    h('div', { class: 'meta' }, when(photo.taken)),
    stars, flags,
    (photo.camera || photo.lens || photo.exposure) && label('Camera'),
    photo.camera && h('div', {}, photo.camera),
    photo.lens && h('div', { class: 'meta' }, photo.lens),
    photo.exposure && h('div', { class: 'meta' }, photo.exposure),
    photo.size && label('Size'),
    photo.size && h('div', { class: 'meta' }, photo.size),
    photo.people.length > 0 && label('People'),
    photo.people.length > 0 && chips(photo.people, person => go('/', { person })),
    photo.albums.length > 0 && label('Albums'),
    photo.albums.length > 0 && chips(photo.albums, name => go('/', { album: albums.find(album => album.name === name)?.key })),
    label('By'), by,
    library && label('Library'),
    library && h('div', {}, libraryName(library)),
    photo.edited && h('div', { class: 'caption', style: { marginTop: '14px' } }, 'Shown with its edits in Numa.'),
  ].filter(Boolean));
}

function closeViewer() {
  if (!state.viewer) return;
  state.viewer.el.remove();
  state.viewer = null;
  document.body.style.overflow = '';
}

window.addEventListener('popstate', () => { if (state.viewer && history.state?.viewer !== state.viewer.mark) closeViewer(); });
// The desktop's keys, where they mean the same here.
document.addEventListener('keydown', event => {
  if (!state.viewer || event.target.matches?.('input') || event.ctrlKey || event.metaKey || event.altKey) return;
  const key = event.key.toLowerCase();
  const zoom = state.viewer.zoom;
  if (key === 'arrowright') step(1);
  else if (key === 'arrowleft') step(-1);
  else if (key === 'escape') {
    const open = state.viewer.el.querySelector('.menu');
    if (open) open.remove();
    else if (zoom.s > 1.001) { zoom.s = 1; clamp(); apply(); } else history.back();
  } else if (key === 'i') toggleInfo();
  else if (/^[0-5]$/.test(key)) setMarks({ rating: Number(key) });
  else if (key === 'p') setMarks({ flag: 'pick' });
  else if (key === 'x') setMarks({ flag: 'reject' });
  else if (key === 'u') setMarks({ flag: 'none' });
  else return;
  event.preventDefault();
});

// HOME-012: whose camera is whose — every camera in the archive, its count,
// the account it is linked to, and what its EXIF Artist says.
async function cameras(main) {
  // The frame at once, the cameras when they come.
  const waiting = h('div', { class: 'accounts cameras' }, h('div', {}, h('div', { class: 'section-label' }, 'Cameras'), h('p', { class: 'body' }, 'Loading…')));
  main.append(waiting);
  const [list, users] = await Promise.all([api('cameras', { wait: 15 }), api('users', { wait: 15 })])
    .catch(error => (waiting.replaceWith(empty('Cameras', error.message)), [null, null]));
  if (!list) return;
  waiting.remove();
  const said = h('p', { class: 'said', role: 'status', style: { textAlign: 'left' } });
  const link = async (camera, account) => {
    try { await api('cameras', { method: 'PUT', body: JSON.stringify({ camera, account }) }); render(); } catch (error) { said.textContent = error.message; }
  };
  const rows = list.map(camera => {
    const select = h('select', { 'aria-label': 'Whose camera', onchange: () => link(camera.camera, select.value || null) },
      h('option', { value: '' }, 'Nobody'), users.map(user => h('option', { value: user.name, selected: user.name === camera.account }, user.name)));
    const hint = camera.hint && camera.hint !== camera.account
      ? h('button', { class: 'chip', title: 'Link this camera to ' + camera.hint, onclick: () => link(camera.camera, camera.hint) }, `Artist says ${camera.hint}`)
      : camera.artist && !camera.hint && h('span', { class: 'caption' }, `Artist: ${camera.artist}`);
    return h('li', {}, h('div', {}, h('div', {}, [camera.make, camera.model].filter(Boolean).join(' ')),
      h('div', { class: 'caption' }, [camera.serial && `Serial ${camera.serial}`, counted(camera.count)].filter(Boolean).join(' · '))),
    h('div', { class: 'link' }, hint, select));
  });
  main.append(h('div', { class: 'accounts cameras' },
    h('div', {}, h('div', { class: 'section-label' }, 'Cameras'),
      list.length ? h('ul', {}, rows) : h('p', { class: 'body' }, 'Cameras show here once the archive has been looked through.')),
    said,
    h('p', { class: 'body' }, 'A photograph is by whoever sent it home; otherwise by the person its camera is linked to here, or whose name its camera writes as the Artist. The rest are Unknown.')));
}

start();
