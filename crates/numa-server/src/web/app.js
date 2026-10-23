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

// Lucide (ISC, licensed on /licences): the icons the pages draw, on its
// 24-unit box and stroked 1.7 with round caps and joins, the hand of Numa's
// own tab icons. Stars, flags and the cross are icons here, never glyphs.
const ICONS = {
  close: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  info: '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
  download: '<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>',
  back: '<path d="m15 18-6-6 6-6"/>',
  on: '<path d="m9 18 6-6-6-6"/>',
  expand: '<path d="M15 3h6v6"/><path d="m21 3-7 7"/><path d="m3 21 7-7"/><path d="M9 21H3v-6"/>',
  star: '<path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"/>',
  more: '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
  trash: '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" x2="10" y1="11" y2="17"/><line x1="14" x2="14" y1="11" y2="17"/>',
  edit: '<line x1="21" x2="14" y1="4" y2="4"/><line x1="10" x2="3" y1="4" y2="4"/><line x1="21" x2="12" y1="12" y2="12"/><line x1="8" x2="3" y1="12" y2="12"/><line x1="21" x2="16" y1="20" y2="20"/><line x1="12" x2="3" y1="20" y2="20"/><line x1="14" x2="14" y1="2" y2="6"/><line x1="8" x2="8" y1="10" y2="14"/><line x1="16" x2="16" y1="18" y2="22"/>',
  turn: '<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>',
  flag: '<path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><line x1="4" x2="4" y1="22" y2="15"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  minus: '<path d="M5 12h14"/>',
  upload: '<path d="M12 3v12"/><path d="m17 8-5-5-5 5"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>',
  filter: '<path d="M3 6h18"/><path d="M7 12h10"/><path d="M10 18h4"/>',
  image: '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  // A burst's sign: three stacked cameras.
  burst: '<path d="M9 3h11a2 2 0 0 1 2 2v9"/><path d="M6 6h11a2 2 0 0 1 2 2v9"/><path d="M4 10h2l1.5-2h3l1.5 2H14a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z"/><circle cx="9" cy="15.5" r="2.5"/>',
};
// The key that deletes: a Mac's is marked delete and is the backspace.
const DELETE_KEY = /Mac|iPhone|iPad/.test(navigator.userAgent) ? S.delKeyMac : S.delKey;
function icon(name) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  for (const [key, value] of Object.entries({
    viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.7', 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true',
  })) svg.setAttribute(key, value);
  svg.innerHTML = ICONS[name];
  return svg;
}

// While Numa Home is asked something: the ring turning under the header
// after a moment, as the apps show one, saying Working… while something
// changes and Loading… while it is only read.
const pending = { reads: 0, writes: 0, timer: null };
const loader = h('div', { class: 'loader', role: 'status', hidden: true }, ring(0.25), h('span'));
const toasts = h('div', { class: 'toasts', 'aria-live': 'polite' });
document.body.append(loader, toasts);
function waiting(write, by) {
  pending[write ? 'writes' : 'reads'] += by;
  loader.lastChild.textContent = pending.writes ? S.working : S.loading;
  if (pending.reads + pending.writes) pending.timer ??= setTimeout(() => { loader.hidden = false; }, 300);
  else { clearTimeout(pending.timer); pending.timer = null; loader.hidden = true; }
}

// A global state is a banner at the top, in plain ink (P7): Numa Home cannot
// be reached. It is one element the page keeps; any answer from the server
// takes it away again.
const unreachable = h('div', { class: 'banner', role: 'status', hidden: true });
function reach(ok) {
  unreachable.hidden = ok;
  if (!ok) unreachable.textContent = phrase('unreachable', { host: location.host });
}

// `wait`: seconds before giving up with a sentence, for a page that would
// otherwise stay empty while a busy NAS takes its time. `quiet`: asked in
// the background, or shown waiting in its own place, so without the loader.
async function api(path, options = {}) {
  if (options.quiet) return fetched(path, options);
  const write = (options.method || 'GET') !== 'GET';
  waiting(write, 1);
  try { return await fetched(path, options); } finally { waiting(write, -1); }
}
async function fetched(path, options) {
  const headers = { ...(options.body ? { 'Content-Type': 'application/json' } : {}), ...options.headers };
  const signal = options.wait ? AbortSignal.timeout(options.wait * 1000) : undefined;
  let response;
  try { response = await fetch('/api/v1/' + path, { credentials: 'same-origin', signal, ...options, headers }); } catch (error) {
    if (error.name === 'TimeoutError') throw new Error(S.tooSlow);
    reach(false);
    throw new Error(phrase('unreachable', { host: location.host }));
  }
  reach(true);
  const text = await response.text();
  let body = null;
  try { body = text ? JSON.parse(text) : null; } catch (_) { body = null; }
  if (response.status === 401 && !['login', 'setup'].includes(path)) {
    signIn();
    throw new Error(S.signInFirst);
  }
  if (!response.ok) throw new Error((body && body.error) || phrase('serverAnswered', { status: response.status }));
  return body;
}

const grouped = n => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
// A meter's fill, scaled from its start: how far, 0 to 1.
const filled = fraction => ({ transform: `scaleX(${Math.min(1, Math.max(0, fraction || 0))})` });
const counted = n => `${grouped(n)} ${n === 1 ? S.photo : S.photos}`;
const starsOf = n => `${n} ${n === 1 ? S.star : S.stars}`;
// A photograph edited in this visit is asked for by a number of its own: a
// picture the page has loaded once is used again for the same address,
// whatever its headers say.
const versions = new Map();
const versioned = (path, id) => path + (versions.has(id) ? (path.includes('?') ? '&' : '?') + 'v=' + versions.get(id) : '');
const thumb = (id, edge) => versioned(`/api/v1/photos/${id}/thumb?edge=${edge}`, id);
function monthName(month) {
  const [year, number] = month.split('-').map(Number);
  return new Date(Date.UTC(year, number - 1, 1)).toLocaleString(S.locale, { month: 'long', year: 'numeric', timeZone: 'UTC' });
}
// What a screen reader says of a tile: the file and the day (P15). The
// camera's own clock, read as Numa reads it: as if it were UTC.
const dayOf = new Intl.DateTimeFormat(S.locale, { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' });
const named = photo => photo.name ? `${photo.name}, ${dayOf.format(photo.taken * 1000)}` : S.photograph;
const when = seconds => new Date(seconds * 1000).toLocaleString(S.locale, {
  day: 'numeric', month: 'long', year: 'numeric', hour: '2-digit', minute: '2-digit', timeZone: 'UTC',
});

// Routes live in the hash: #/ (photos, narrowed by ?library=&person=&album=&q=),
// #/today (HOME-039), #/libraries, #/people, #/albums, #/accounts, #/activity,
// #/cameras, #/trash, #/import, #/tidy (?kind=), #/places (HOME-041, ?at=lat,lon
// and the timeline's filters). No route at all opens the
// account's start page.
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
    app.replaceChildren(h('div', { class: 'gate' }, unreachable.hidden ? h('p', { class: 'said' }, error.message) : unreachable));
    // A state that mends itself: asked again until it answers.
    setTimeout(start, 5000);
    return;
  }
  // HOME-034: no account yet, so the first start begins.
  if (status.setup) return firstStart('account');
  try { state.me = await api('me'); } catch (_) { return; }
  if (state.me.must_change) return renewGate();
  if (!location.hash) history.replaceState(null, '', '#' + startPage());
  render();
}

function gate(text, button, submit) {
  const name = h('input', { type: 'text', autocomplete: 'username', placeholder: S.name, required: true, autocapitalize: 'words' });
  const password = h('input', {
    type: 'password', placeholder: S.password, required: true,
    autocomplete: button === S.signIn ? 'current-password' : 'new-password',
  });
  gateOf(S.numaHome, text, [name, password], button, () => submit(name.value.trim(), password.value));
}

// The page before any other: the mark, a heading, a sentence, the fields and
// one button; what went wrong under it.
function gateOf(heading, text, fields, button, submit) {
  titled(button);
  const said = h('p', { class: 'said', role: 'status' });
  const form = h('form', {
    onsubmit: async event => {
      event.preventDefault();
      said.textContent = '';
      try { await submit(); } catch (error) { said.textContent = error.message; }
    },
  }, h('img', { class: 'mark', src: '/mark.png', alt: '' }), h('h1', {}, heading), h('p', { class: 'body' }, text),
  fields, h('button', { class: 'primary', type: 'submit' }, button), said);
  app.replaceChildren(h('div', { class: 'gate' }, form));
  fields[0].focus();
}

// HOME-037: an admin gave this account its password, so it chooses its own
// before anything else.
const newPasswordField = (name, placeholder) => h('input', { type: 'password', name, placeholder, required: true, minlength: 8, autocomplete: 'new-password' });
function renewGate() {
  const fresh = newPasswordField('new', S.newPasswordHint), again = newPasswordField('again', S.passwordAgain);
  gateOf(S.newPassword, S.newPasswordText, [fresh, again], S.savePassword, async () => {
    if (fresh.value !== again.value) throw new Error(S.passwordsDiffer);
    await api('me/password', { method: 'PUT', body: JSON.stringify({ new: fresh.value }) });
    state.me.must_change = false;
    if (!location.hash) history.replaceState(null, '', '#' + startPage());
    render();
  });
}

// HOME-037: the account's own password, from the menu under its name. Its
// other devices are signed out.
async function changePassword() {
  const old = h('input', { type: 'password', name: 'old', placeholder: S.currentPassword, required: true, autocomplete: 'current-password' });
  const asked = await dialog(S.changePassword, S.changePasswordText, [old, newPasswordField('new', S.newPasswordHint), newPasswordField('again', S.passwordAgain)], S.change);
  if (!asked) return;
  if (asked.new !== asked.again) return toast(S.passwordsDiffer);
  try { await api('me/password', { method: 'PUT', body: JSON.stringify({ old: asked.old, new: asked.new }) }); toast(S.passwordChanged); } catch (error) { toast(error.message); }
}

// HOME-037: what Safe at Home › Devices calls this browser, "Firefox on Mac".
function browserName() {
  const agent = navigator.userAgent;
  const browser = [['Edg/', 'Edge'], ['Firefox/', 'Firefox'], ['Chrome/', 'Chrome'], ['Safari/', 'Safari']].find(([mark]) => agent.includes(mark))?.[1] || S.browser;
  const system = [['iPhone', 'iPhone'], ['iPad', 'iPad'], ['Android', 'Android'], ['Mac OS X', 'Mac'], ['Windows', 'Windows'], ['Linux', 'Linux']].find(([mark]) => agent.includes(mark))?.[1];
  return system ? phrase('browserOn', { browser, system }) : browser;
}

function signIn() {
  state.me = null;
  closeViewer(false);
  gate(S.signInText, S.signIn, signInAs);
}

async function signInAs(name, password) {
  const me = await api('login', { method: 'POST', body: JSON.stringify({ name, password, device: browserName() }) });
  state.me = await api('me').catch(() => ({ name: me.name, admin: me.admin, manage: false, must_change: me.must_change }));
  state.libraries = null;
  if (state.me.must_change) return renewGate();
  if (!location.hash) history.replaceState(null, '', '#' + startPage());
  render();
}

async function libraries() {
  state.libraries = state.libraries || await api('libraries');
  return state.libraries;
}
const libraryName = library => library.mine ? S.sentHome : library.name;

window.addEventListener('hashchange', () => { if (state.me) render(); });

// One title per page, so a tab, the history and a screen reader say where you
// are: "Photos · Numa Home".
const TITLES = { '/today': S.today, '/libraries': S.libraries, '/people': S.people, '/albums': S.albums, '/places': S.places, '/import': S.importPage, '/tidy': S.tidyUp, '/trash': S.trash,
  '/safe': S.safeAtHome, '/activity': S.safeAtHome, '/accounts': S.safeAtHome, '/devices': S.safeAtHome, '/cameras': S.safeAtHome };
const titled = page => { document.title = page ? `${page} · ${S.numaHome}` : S.numaHome; };
function pageTitle(path, params) {
  if (['/accounts', '/cameras'].includes(path) && !state.me.admin) path = '/';
  if (TITLES[path]) return TITLES[path];
  if (params.get('q')) return S.search;
  // A library, an album or a person: its name, which the page says once it knows it.
  return params.get('person') || (params.get('library') && S.library) || (params.get('album') && S.album) || S.photosPage;
}

function render() {
  if (state.me.first_start) return firstStart(state.me.first_start);
  const { path, params } = route();
  closeViewer(false);
  titled(pageTitle(path, params));
  const main = h('main');
  app.replaceChildren(header(path, params), placesNav(path, params), unreachable, h('div', { class: 'cardbar', hidden: true }), sendBar(), main);
  cardBar();
  activityBadge();
  const head = SAFE.includes(path) && safeHead(path);
  if (head) main.append(head);
  if (path === '/safe') return safePage(main);
  if (path === '/today') return today(main, params);
  if (path === '/activity') return activityPage(main);
  if (path === '/import') return importPage(main);
  if (path === '/trash') return trashPage(main);
  if (path === '/tidy') return tidyPage(main, params);
  if (path === '/people') return people(main);
  if (path === '/albums') return albums(main);
  if (path === '/places') return placesPage(main, params);
  if (path === '/libraries') return shelves(main);
  if (path === '/accounts' && state.me.admin) return accounts(main);
  if (path === '/devices') return devicesPage(main);
  if (path === '/cameras' && state.me.admin) return cameras(main);
  if (params.get('q')) return search(main, params);
  return timeline(main, params);
}

function header(path, params) {
  const field = h('input', { type: 'search', placeholder: S.searchHint, value: params.get('q') || '', enterkeyhint: 'search', 'aria-label': S.search });
  const search = h('form', { class: 'search', role: 'search', onsubmit: event => { event.preventDefault(); go('/', { q: field.value.trim() }); } }, field);
  const bar = h('header', {});
  const who = h('button', { class: 'chip who', 'aria-haspopup': 'menu', onclick: event => {
    event.stopPropagation();
    menu(bar, [
      h('div', { class: 'section-label' }, state.me.name),
      h('button', { onclick: async () => {
        const start = startPage() === '/today' ? 'photos' : 'today';
        try { await api('me', { method: 'PUT', body: JSON.stringify({ start }) }); state.me.start = start; } catch (error) { toast(error.message); }
      } }, startPage() === '/today' ? S.startOnPhotos : S.startOnToday),
      h('button', { onclick: changePassword }, S.changePasswordEll),
      h('button', { onclick: async () => { await api('logout', { method: 'POST' }).catch(() => {}); signIn(); } }, S.signOut),
    ]);
  } }, state.me.name);
  // The mark is a link to where Numa Home opens, never a button (patronen §1).
  bar.append(h('a', { class: 'mark-link', href: '#' + startPage(), 'aria-label': S.numaHome }, h('img', { class: 'mark', src: '/mark.png', alt: '' })),
    badgeButton(), search, h('button', { class: 'chip add', title: S.addPhotosEll, 'aria-label': S.addPhotosEll, onclick: addPhotos }, icon('upload'), h('span', { class: 'what' }, S.addPhotosEll)), who);
  return bar;
}

// Every place in one list: down the side on a wide screen, along the foot
// on a phone, where the four first are tabs and the rest are under More.
const OWNERS = { library: '/libraries', album: '/albums', person: '/people' };
function placesNav(path, params) {
  const owner = path === '/' && Object.keys(OWNERS).find(key => params.get(key));
  // HOME-035: Accounts, Cameras and Activity are Safe at Home's tabs.
  const here = owner ? OWNERS[owner] : SAFE.includes(path) ? '/safe' : path;
  const place = (label, to, extra) => h('button', { class: 'place-link' + (here === to ? ' chosen' : '') + (extra ? ' extra' : ''),
    'aria-current': here === to ? 'page' : null, onclick: () => go(to) }, label);
  const rest = [
    h('div', { class: 'section-label extra' }, S.archive),
    place(S.importPage, '/import', true), place(S.tidyUp, '/tidy', true), place(S.trash, '/trash', true),
    h('div', { class: 'section-label extra' }, S.household),
    place(S.safeAtHome, '/safe', true),
  ];
  // HOME-031: on a wide screen Libraries, People and Albums fold open to
  // every one of them: a library with its tint, a person with their face,
  // and the count; a phone keeps its tabs.
  const group = (label, to, key) => {
    const open = navOpen().includes(key);
    const list = h('div', { class: 'rail' });
    const entries = h('div', { class: 'place-entries extra', inert: !open }, list);
    const fill = items => list.replaceChildren(...items.map(([value, name, count, look]) => {
      const chosen = owner === key && params.get(key) === String(value);
      look = key === 'person' ? h('img', { class: 'face', src: thumb(look, 256), loading: 'lazy', alt: '' })
        : h('i', { class: 'tint', style: { background: key === 'library' ? `var(--library-tint-${look})` : null } });
      const entry = h('button', { class: 'place-entry' + (chosen ? ' chosen' : ''), 'aria-current': chosen ? 'page' : null,
        onclick: () => go('/', { [key]: value }) }, look, h('span', { class: 'name' }, name), h('span', { class: 'caption' }, grouped(count)));
      if (key === 'album') entry.addEventListener('contextmenu', event => { event.preventDefault(); menuAt(event.clientX, event.clientY, albumItems({ key: value, name })); });
      peekOn(entry, `${key}=${encodeURIComponent(value)}`, name, count);
      return entry;
    }), ...(key === 'album' ? [h('button', { class: 'place-entry new', onclick: newAlbum }, h('i', { class: 'plus' }, icon('plus')), h('span', { class: 'name' }, S.newAlbumEll))] : []));
    if (open) navEntries(key, fill);
    const link = place(label, to);
    // An open list shows which one is chosen; its heading need not. A
    // phone shows no list, so its tab is.
    const mark = open => {
      const chosen = here === to && !(open && owner === key && matchMedia('(min-width: 900px)').matches);
      link.classList.toggle('chosen', chosen);
      if (chosen) link.setAttribute('aria-current', 'page'); else link.removeAttribute('aria-current');
    };
    mark(open);
    const fold = h('button', { class: 'place-fold extra', 'aria-expanded': String(open), 'aria-label': phrase('everyOne', { label }),
      onclick: () => {
        const now = !box.classList.contains('open');
        navOpen(key, now);
        box.classList.toggle('open', now);
        entries.inert = !now;
        fold.setAttribute('aria-expanded', String(now));
        mark(now);
        if (now) navEntries(key, fill);
      } }, icon('on'));
    const box = h('div', { class: 'place-group' + (open ? ' open' : '') }, h('div', { class: 'place-head' }, link, fold), entries);
    return box;
  };
  // HOME-039: a phone's tabs are Today, Photos, Libraries and People;
  // Albums goes under More.
  const albums = group(S.albums, '/albums', 'album');
  albums.classList.add('extra');
  // HOME-041: Places, with how many towns; under More on a phone.
  const places = place(S.places, '/places', true);
  const towns = h('span', { class: 'caption' }, navLists.places != null ? grouped(navLists.places) : '');
  places.append(towns);
  places.classList.add('counted');
  if (navLists.places == null) api('places?count=1', { quiet: true }).then(found => {
    towns.textContent = grouped(found.count);
    if (!found.looking) navLists.places = found.count;
  }, () => {});
  const nav = h('nav', { class: 'places-nav', 'aria-label': S.places },
    place(S.today, '/today'), place(S.photosPage, '/'), group(S.libraries, '/libraries', 'library'), group(S.people, '/people', 'person'), albums, places, rest);
  const extras = ['/albums', '/places', '/import', '/tidy', '/trash', '/safe'];
  nav.append(h('button', { class: 'place-link more' + (extras.includes(here) ? ' chosen' : ''), 'aria-haspopup': 'menu', onclick: event => {
    event.stopPropagation();
    menu(nav, [place(S.albums, '/albums'), place(S.places, '/places'), ...rest].filter(Boolean).map(one => one.tagName === 'BUTTON'
      ? h('button', { class: one.classList.contains('chosen') ? 'chosen' : null, onclick: () => one.click() }, one.textContent)
      : h('div', { class: 'section-label' }, one.textContent)));
  } }, S.more));
  return nav;
}

// Which of the side's lists are open, kept by the browser.
function navOpen(key, open) {
  let keys = [];
  try { keys = JSON.parse(localStorage.getItem('numa-nav-open')) || []; } catch (_) { /* a private window */ }
  if (key === undefined) return keys;
  keys = keys.filter(one => one !== key).concat(open ? [key] : []);
  try { localStorage.setItem('numa-nav-open', JSON.stringify(keys)); } catch (_) { /* a private window */ }
}

// A list's entries as [value, name, count, look]: the last ones seen at once,
// then the server's. A library's look is its tint, from the whole archive's
// timeline as on the Libraries page; a person's is their portrait.
const navLists = {};
function navEntries(key, fill) {
  if (navLists[key]) fill(navLists[key]);
  const asked = {
    library: () => Promise.all([libraries(), api('timeline', { quiet: true }).then(tintsOf)]).then(([list, tints]) =>
      list.filter(one => one.count || !one.mine).map(one => [one.id, libraryName(one), one.count, tints.get(one.id) ?? 0])),
    person: () => api('people').then(list => list.map(one => [one.name, one.name, one.count, one.portrait])),
    album: () => api('albums').then(list => list.map(one => [one.key, one.name, one.count])),
  }[key];
  asked().then(list => { navLists[key] = list; fill(list); }).catch(() => {});
}

// An entry held under the pointer a moment shows its stack of prints beside
// the list, with how many and when: which of two Italies is which.
const peeks = new Map();
function peekOn(entry, query, name, count) {
  if (!matchMedia('(hover: hover)').matches) return;
  let timer;
  entry.addEventListener('pointerenter', () => { timer = setTimeout(() => peek(entry, query, name, count), 350); });
  entry.addEventListener('pointerleave', () => { clearTimeout(timer); peeks.get(query)?.remove(); });
}
function peek(entry, query, name, count) {
  if (!peeks.has(query)) {
    const months = api('timeline?' + query, { quiet: true });
    const caption = h('div', { class: 'caption' }, counted(count));
    months.then(list => { if (list.length) caption.textContent += ' · ' + span(list.map(one => one.month).sort()); }).catch(() => {});
    peeks.set(query, h('div', { class: 'peek', 'aria-hidden': 'true' }, stack(query, months), h('div', { class: 'name' }, name), caption));
  }
  const card = peeks.get(query);
  const box = entry.getBoundingClientRect();
  card.style.top = `${Math.max(8, Math.min(innerHeight - 220, box.top + box.height / 2 - 100))}px`;
  entry.closest('nav').append(card);
}
const shortMonth = month => monthName(month).replace(/^(\w{3})\w*/, '$1');
const span = months => months[0] === months.at(-1) ? shortMonth(months[0]) : `${shortMonth(months[0])} – ${shortMonth(months.at(-1))}`;

// A menu takes the focus, the arrows (and Home and End) move through its
// items, Escape or a click closes it, and the focus goes back to where it was
// (P15). One is open at a time.
let openMenu = null;
function closeMenu() {
  if (!openMenu) return;
  const { box, opener } = openMenu;
  openMenu = null;
  box.remove();
  if (opener?.isConnected) opener.focus({ preventScroll: true });
  holdLoupe();
}
function showMenu(box, place) {
  closeMenu();
  const opener = document.activeElement;
  place();
  openMenu = { box, opener };
  const items = () => [...box.querySelectorAll('button, a')];
  for (const item of items()) item.setAttribute('role', 'menuitem');
  for (const rule of box.querySelectorAll('.rule')) rule.setAttribute('role', 'separator');
  box.addEventListener('keydown', event => {
    const all = items();
    const at = all.indexOf(document.activeElement);
    const to = { ArrowDown: at + 1, ArrowRight: at + 1, ArrowUp: at - 1, ArrowLeft: at - 1, Home: 0, End: all.length - 1 }[event.key];
    if (event.key === 'Tab') return closeMenu();
    if (to == null) return;
    event.preventDefault();
    event.stopPropagation();
    all[(to + all.length) % all.length]?.focus();
  });
  items()[0]?.focus({ preventScroll: true });
  setTimeout(() => document.addEventListener('click', () => { if (openMenu?.box === box) closeMenu(); }, { once: true }));
}

// A menu under its place.
function menu(parent, items) {
  const box = h('div', { class: 'menu', role: 'menu' }, items);
  showMenu(box, () => parent.append(box));
}

// What a download gives, as Export in the apps: the file as it is, or
// developed with its edits at full size or at 2048 px on the long edge.
const downloadItems = (original, jpeg) => [
  h('a', { href: original, download: '' }, S.original),
  h('a', { href: jpeg, download: '' }, S.jpegFull),
  h('a', { href: `${jpeg}${jpeg.includes('?') ? '&' : '?'}edge=2048`, download: '' }, phrase('jpegEdge', { edge: 2048 })),
];

// A menu at a point of the screen, kept inside it: where the pointer is for a
// right-click, or above its button in a selection bar, whose own box would
// clip it.
function menuAt(x, y, items, above) {
  const box = h('div', { class: 'menu floating', role: 'menu' }, items);
  showMenu(box, () => {
    document.body.append(box);
    const { width, height } = box.getBoundingClientRect();
    box.style.left = Math.max(8, Math.min(above ? x - width / 2 : x, innerWidth - width - 8)) + 'px';
    box.style.top = Math.max(8, Math.min(above ? y - height - 8 : y, innerHeight - height - 8)) + 'px';
  });
}
const menuOver = (button, items) => {
  const box = button.getBoundingClientRect();
  menuAt(box.left + box.width / 2, box.top, items, true);
};

// An error that is only information is words in the `name` grey, never red
// (P7); `failed` says the text is one.
const fault = message => h('p', { class: 'body fault' }, message);
function empty(title, text, failed) {
  return h('div', { class: 'empty' }, h('h2', {}, title), failed ? fault(text) : h('p', { class: 'body' }, text));
}

// HOME-054: how tall the rows are, Linux's Photo Size: chosen with the
// slider at the One filter's end and kept in this browser. A phone keeps its own.
const SIZE = { min: 120, max: 440, step: 40, rest: 200 };
let photoSize = SIZE.rest;
try { photoSize = Number(localStorage.getItem('numa-photo-size')) || SIZE.rest; } catch (_) { /* a private window */ }
// The order of the photographs, as Numa's Sort By: kept in this browser. A date
// runs as the page does (a library oldest first, Photos newest first) until turned round.
const SORTS = {
  date: [S.sortDate, S.oldestFirst, S.newestFirst], name: [S.sortName, S.aToZ, S.zToA], rating: [S.sortRating, S.highestFirst, S.lowestFirst],
  sharpness: [S.sortSharpness, S.sharpestFirst, S.softestFirst], suggestion: [S.sortSuggestion, S.bestFirst, S.weakestFirst],
};
let viewSort = { sort: 'date', reversed: false };
try { Object.assign(viewSort, JSON.parse(localStorage.getItem('numa-sort') || '{}')); } catch (_) { /* a private window */ }
if (!SORTS[viewSort.sort]) viewSort = { sort: 'date', reversed: false };
const sortQuery = () => [viewSort.sort !== 'date' && ['sort', viewSort.sort], viewSort.reversed && ['reversed', '1']].filter(Boolean);
const rowHeight = () => innerWidth < 720 ? 110 : photoSize;
// A search's grid: tiles about as wide as a row of that height.
const tileWide = () => innerWidth < 720 ? 150 : Math.round(photoSize * 0.75);
document.documentElement.style.setProperty('--tile', tileWide() + 'px');
const GAP = 3;

// The timeline: every month's count at once, a month's photographs when it
// comes near the screen, its rows justified to the width.
async function timeline(main, params) {
  const narrowed = narrowedBy(params);
  const query = new URLSearchParams([...narrowed, ...sortQuery()]).toString();
  // The months asked at once, beside the filters, not after them.
  const asked = api('timeline' + (query ? '?' + query : '')).then(months => ({ months }), error => ({ error }));
  const filters = await filterRow(params, narrowed);
  main.append(filters.head, filters.row);
  const { months, error } = await asked;
  if (error) {
    main.append(empty(S.nothingToShow, error.message, true));
    return;
  }
  if (!main.isConnected) return;
  filters.count.textContent = counted(months.reduce((sum, month) => sum + month.count, 0));
  if (months.length && months[0].month !== 'all') filters.dates.textContent = spanOf(months);
  if (!months.length) {
    main.append(narrowed.length
      ? empty(S.nothingHere, S.noFilterMatch)
      : empty(S.noPhotosYet, S.noPhotosText));
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

// UX-081: what narrows a page of photographs, in the order its chips stand,
// and the page's filters with `change` made (a value of null takes one off).
const FILTERS = ['library', 'album', 'person', 'place', 'region', 'camera', 'date', 'by', 'thing', 'flag', 'rating', 'q'];
const narrowedBy = params => FILTERS.filter(key => params.get(key)).map(key => [key, params.get(key)]);
const narrow = (params, change) => go('/', { ...Object.fromEntries(narrowedBy(params)), ...change });
// A flag as a filter says it; the loupe's FLAGS are what a button does.
const FLAGGED = { pick: S.picked, reject: S.rejected, none: S.noFlag };
// "★ 3+", the star an icon; "★ 5". Spoken, "3 stars or more".
const rated = n => h('span', { class: 'rated' }, icon('star'), Number(n) === 5 ? '5' : phrase('orMore', { n }));
const ratedSaid = n => Number(n) === 5 ? starsOf(5) : phrase('starsOrMore', { n });
// A date filter as it is said: "2026", "August 2026", "3 August 2026".
const dateName = value => value.length === 4 ? value : value.length === 7 ? monthName(value) : dayOf.format(Date.parse(value + 'T00:00:00Z') || 0);

// A library, an album or a person is a page of its own: its name as the
// title, when its photographs were taken, what can be done with it, and the
// way back; Photos and a search have their title and Select too. Under it
// the One filter (P10): a Filter button with the whole menu, All · Picks ·
// ★ 3+, what else narrows the page each with its ×, and how many there are.
async function filterRow(params, narrowed) {
  const owner = Object.keys(OWNERS).find(key => params.get(key));
  // Asked at once; the row waits for the slowest, not for their sum.
  const [list, albums] = await Promise.all([libraries().catch(() => []), params.get('album') ? api('albums').catch(() => []) : []]);
  const library = list.find(one => String(one.id) === params.get('library'));
  const names = {
    library: library ? libraryName(library) : S.library,
    album: albums.find(album => album.key === params.get('album'))?.name || S.album,
    person: params.get('person'),
  };
  if (owner) titled(names[owner]);
  const count = h('span', { class: 'caption count' });
  const dates = h('span');
  const flag = params.get('flag'), rating = params.get('rating');
  // What a filter is set to, as its chip says it; `short`, as the menu does.
  const said = (key, short) => {
    const value = params.get(key);
    const name = value === 'Unknown' ? S.unknown : value;
    return { library: names.library, album: names.album, date: dateName(value), by: short ? name : phrase('takenByName', { name }),
      flag: FLAGGED[flag], rating: ratedSaid(rating) }[key] ?? value;
  };
  const preset = (label, on, change, spoken) => h('button', { class: 'chip preset' + (on ? ' chosen' : ''), 'aria-pressed': String(on), 'aria-label': spoken, onclick: () => narrow(params, change) }, label);
  // The page's own filter is its title; Picks and ★ 3+ are said by their chips.
  const active = narrowed.filter(([key, value]) => key !== owner && !(key === 'flag' && value === 'pick') && !(key === 'rating' && value === '3'))
    .map(([key]) => h('button', { class: 'chip on' + (key === 'thing' ? ' numa-saw' : ''), 'aria-label': phrase('removeFilter', { name: said(key) }), onclick: () => narrow(params, { [key]: null }) },
      key === 'rating' ? rated(rating) : said(key), h('span', { class: 'x' }, icon('close'))));
  const row = h('div', { class: 'filters one' },
    h('button', { class: 'chip filter', 'aria-haspopup': 'menu', onclick: event => { event.stopPropagation(); filterMenu(event.currentTarget, params, list, said); } }, icon('filter'), S.filter),
    h('span', { class: 'divider' }),
    preset(S.all, !flag && !rating, { flag: null, rating: null }),
    preset(S.picks, flag === 'pick', { flag: flag === 'pick' ? null : 'pick' }),
    preset(rated(3), rating === '3', { rating: rating === '3' ? null : 3 }, ratedSaid(3)),
    active, count, !params.get('q') && sortButton(params), sizeSlider());
  // HOME-023: a trip is a library or an album; its picks make the book.
  const book = (params.get('library') || params.get('album')) &&
    h('button', { class: 'chip', title: S.bookTip, onclick: () => bookDialog(new URLSearchParams(narrowed).toString()) }, S.forABook);
  const back = owner && { library: S.libraries, album: S.albums, person: S.people }[owner];
  const head = h('div', { class: 'collection-head' },
    back && h('button', { class: 'round small', 'aria-label': back, title: back, onclick: () => go(OWNERS[owner]) }, icon('back')),
    h('div', { class: 'titles' }, h('h1', {}, owner ? names[owner] : params.get('q') ? S.search : S.photosPage), h('div', { class: 'caption' }, dates)),
    h('div', { class: 'act' }, book,
      // HOME-019: a library's own Tidy Up, beside its book.
      owner === 'library' && library && tidyEntry(library),
      owner === 'library' && library && state.me.admin && !library.mine && h('button', { class: 'chip', onclick: () => renameLibrary(library) }, S.renameEll),
      owner === 'album' && h('button', { class: 'round small', 'aria-label': S.more, title: S.more, 'aria-haspopup': 'menu', onclick: event => {
        event.stopPropagation();
        const at = event.currentTarget.getBoundingClientRect();
        menuAt(at.left, at.bottom + 6, albumItems({ key: params.get('album'), name: names.album }));
      } }, icon('more')),
      selectToggle()));
  return { head, row, count, dates };
}

// The direction's words: the date's depend on the page (a library or an album
// reads oldest first), the others are fixed.
const sortSaid = sort => SORTS[sort][0];
function sortButton(params) {
  const oldest = !!(params.get('library') || params.get('album'));
  const words = sort => { const [name, ahead, back] = SORTS[sort]; return sort === 'date' && !oldest ? [name, back, ahead] : [name, ahead, back]; };
  const set = change => {
    Object.assign(viewSort, change);
    try { localStorage.setItem('numa-sort', JSON.stringify(viewSort)); } catch (_) { /* a private window */ }
    render();
  };
  const [name, ahead, back] = words(viewSort.sort);
  return h('button', { class: 'chip' + (viewSort.sort !== 'date' || viewSort.reversed ? ' chosen' : ''), 'aria-haspopup': 'menu', title: S.sortBy, onclick: event => {
    event.stopPropagation();
    const at = event.currentTarget.getBoundingClientRect();
    const item = (label, on, change) => h('button', { class: on ? 'chosen' : null, onclick: () => set(change) }, label);
    menuAt(at.left, at.bottom + 6, [
      h('div', { class: 'section-label' }, S.sortBy),
      Object.keys(SORTS).map(sort => item(SORTS[sort][0], viewSort.sort === sort, { sort })),
      h('div', { class: 'rule' }),
      item(ahead, !viewSort.reversed, { reversed: false }),
      item(back, viewSort.reversed, { reversed: true }),
    ]);
  } }, `${name} · ${viewSort.reversed ? back : ahead}`);
}

// HOME-054: smaller to larger; a double click puts it back.
function sizeSlider() {
  const input = h('input', { type: 'range', min: SIZE.min, max: SIZE.max, step: SIZE.step, value: photoSize, 'aria-label': S.photoSize,
    oninput: () => {
      photoSize = Number(input.value);
      try { localStorage.setItem('numa-photo-size', photoSize); } catch (_) { /* a private window */ }
      relayout();
    },
    ondblclick: () => { input.value = SIZE.rest; input.dispatchEvent(new Event('input')); } });
  return h('label', { class: 'size', title: S.photoSize }, icon('image'), input, icon('image'));
}

// The Filter menu: every filter by name, with what it is set to; a name
// opens its choices, each with how many photographs it keeps among the
// page's other filters (`filters`, asked once the menu opens), and a choice
// narrows the page; the chosen one again, or its chip's ×, takes it off.
function filterMenu(button, params, list, said) {
  const asked = api('filters?' + new URLSearchParams(narrowedBy(params)), { quiet: true });
  asked.catch(() => {});
  const box = h('div', { class: 'menu floating filter-menu', role: 'menu' });
  // Under the button, kept inside the window as it grows and shrinks.
  const place = () => {
    const at = button.getBoundingClientRect(), { width, height } = box.getBoundingClientRect();
    box.style.left = Math.max(8, Math.min(at.left, innerWidth - width - 8)) + 'px';
    box.style.top = Math.max(8, Math.min(at.bottom + 6, innerHeight - height - 8)) + 'px';
  };
  const fill = items => {
    box.replaceChildren(...items.flat(Infinity).filter(Boolean));
    for (const item of box.querySelectorAll('button')) item.setAttribute('role', 'menuitem');
    place();
    (box.querySelector('button.chosen') || box.querySelector('button:not(.back)') || box.querySelector('button'))?.focus({ preventScroll: true });
  };
  const choice = (key, value, label, count, look, spoken) => {
    const on = params.get(key) === String(value);
    return h('button', { class: [on && 'chosen', look].filter(Boolean).join(' ') || null, 'aria-label': spoken, onclick: () => narrow(params, { [key]: on ? null : value }) },
      label, h('span', { class: 'caption' }, grouped(count)));
  };
  const note = text => h('div', { class: 'caption note' }, text);
  const choices = (key, data) => {
    const rows = {
      library: () => {
        const counts = new Map(data.libraries);
        return [h('button', { class: params.get('library') ? null : 'chosen', onclick: () => narrow(params, { library: null }) }, S.allLibraries),
          list.filter(one => counts.has(one.id)).map(one => choice('library', one.id, libraryName(one), counts.get(one.id)))];
      },
      person: () => data.people.map(([name, n]) => choice('person', name, name, n)),
      place: () => [data.places.map(land => [choice('place', land.country, land.country, land.count),
        land.towns.map(([town, n]) => choice('place', town, town, n, 'town'))]),
      data.looking ? note(S.lookingWhere) : !data.places.length && note(S.noPlaces)],
      camera: () => data.cameras.map(([name, n]) => choice('camera', name, name, n)),
      date: () => data.years.map(([year, n]) => choice('date', year, year, n)),
      by: () => data.by.map(([account, n]) => choice('by', account || 'Unknown', account || S.unknown, n)),
      flag: () => {
        const counts = new Map(data.flags);
        return Object.entries(FLAGGED).map(([value, label]) => choice('flag', value, label, counts.get(value) || 0));
      },
      rating: () => [1, 2, 3, 4, 5].map(n => choice('rating', n, rated(n), data.ratings.slice(n).reduce((a, b) => a + b, 0), null, ratedSaid(n))),
      thing: () => data.things.map(([thing, n]) => choice('thing', thing, thing, n, 'numa-saw')),
    }[key]().flat(Infinity).filter(Boolean);
    return rows.length ? rows : [note(S.nothingToChoose)];
  };
  const SECTIONS = [['library', S.library], ['person', S.people], ['place', S.places], ['camera', S.cameras], ['date', S.dates],
    ['by', S.takenByHead], ['flag', S.flagLabel], ['rating', S.rating], ['thing', S.thingsNumaSaw]];
  const open = async (key, label) => {
    const head = [h('button', { class: 'back', onclick: event => { event.stopPropagation(); top(); } }, icon('back'), S.filter), h('div', { class: 'section-label' }, label)];
    fill([head, note(S.loading)]);
    let data;
    try { data = await asked; } catch (error) { return fill([head, note(error.message)]); }
    if (openMenu?.box === box) fill([head, choices(key, data)]);
  };
  const top = () => fill(SECTIONS.map(([key, label]) => h('button', { onclick: event => { event.stopPropagation(); open(key, label); } },
    label, h('span', { class: 'caption' }, params.get(key) ? (key === 'rating' ? rated(params.get(key)) : said(key, true)) : ''), icon('on'))));
  showMenu(box, () => { document.body.append(box); top(); });
}

// When a page's photographs were taken: "Aug 2026", "Mar – Aug 2026",
// "Dec 2025 – Jan 2026".
function spanOf(months) {
  const sorted = months.map(month => month.month).sort();
  const name = (month, year) => {
    const [y, m] = month.split('-').map(Number);
    return new Date(Date.UTC(y, m - 1, 1)).toLocaleString(S.locale, { month: 'short', ...(year && { year: 'numeric' }), timeZone: 'UTC' });
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
    h('div', { class: 'head' }, h('span', { class: 'section-label' }, month.month === 'all' ? sortSaid(viewSort.sort) : monthName(month.month)), caption),
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
      tiles.push(tile(section, photo, { left: left + 'px', top: top + 'px', width: wide + 'px', height: height + 'px' }, height, wide));
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

// A photograph's tile: in the rows (`style` its place), or in a search's grid.
// `wide`: how wide it is drawn, so a narrow one keeps only the marks it has
// room for (the count goes first).
function tile(section, photo, style, height, wide = 150) {
  const edge = height * (devicePixelRatio || 1) > 330 ? 512 : 256;
  const image = h('img', { src: thumb(photo.id, edge), loading: 'lazy', decoding: 'async', alt: '' });
  image.addEventListener('load', () => image.classList.add('in'));
  const look = ['tile', picked.ids.has(photo.id) && 'picked', photo.flag === 'reject' && 'rejected', wide < 120 && 'small', wide < 72 && 'tiny'];
  // HOME-017: a click chooses once something is chosen (`pick`).
  return holdToChoose(h('button', { class: look.filter(Boolean).join(' '), style, 'data-id': photo.id, 'aria-label': tileName(photo),
    onclick: event => rateStar(event, photo) || pick(event, photo.id) || openViewer(section, section.photos.indexOf(photo)) },
  image, h('span', { class: 'check', 'aria-hidden': 'true' }), tileMarks(photo), wide >= 120 && hoverStars(photo)), photo.id);
}

// LIB-025 as on the desktop: with a mouse over a tile, five stars show at its
// foot and a click on one gives that many (the same again takes them off).
// Shown by CSS for pointers that hover, and not while photographs are chosen.
function hoverStars(photo) {
  return h('span', { class: 'rate', 'aria-hidden': 'true' }, [1, 2, 3, 4, 5].map(n =>
    h('i', { 'data-n': n, class: n <= (photo.rating || 0) ? 'set' : '' }, icon('star'))));
}
function rateStar(event, photo) {
  const star = event.target.closest?.('.rate i');
  if (!star || document.body.classList.contains('picking')) return false;
  const stars = Number(star.dataset.n) === photo.rating ? 0 : Number(star.dataset.n);
  api('photos/marks', { method: 'POST', body: JSON.stringify({ ids: [photo.id], rating: stars }) }).then(() => {
    photo.rating = stars;
    remark([photo.id]);
    toast(phrase('saidCount', { said: markSaid({ rating: stars }), photos: counted(1) }));
  }, error => toast(error.message));
  return true;
}

// UX-081: what has been set on a photograph, said on its tile as every
// client says it (TileMarks): the stars and the pick bottom left in one pill,
// a reject as its cross there with the photograph dimmed; the edited dot
// bottom right; a burst's count top right on its best frame. What Numa
// noticed (Soft, eyes closed) is shown in the loupe only.
function tileMarks(photo) {
  const pill = (where, ...inside) => h('span', { class: 'mark ' + where, 'aria-hidden': 'true' }, inside);
  const judged = photo.flag === 'reject' ? pill('judged rejected', icon('close'))
    : (photo.rating > 0 || photo.flag === 'pick') && pill('judged', photo.rating > 0 && [icon('star'), String(photo.rating)], photo.flag === 'pick' && icon('flag'));
  return [judged, photo.edited && pill('edited', h('i')), photo.stack > 1 && pill('burst', icon('burst'), String(photo.stack))].filter(Boolean);
}
// What a screen reader says of a tile: its file and day, then what is set on it.
const tileName = photo => [named(photo), photo.rating > 0 && starsOf(photo.rating), photo.flag !== 'none' && FLAGGED[photo.flag], photo.edited && S.edited]
  .filter(Boolean).join(', ');
// After stars or a flag changed: these photographs' tiles say it.
function remark(ids) {
  const listed = new Map(state.sections.flatMap(section => section.photos || []).map(photo => [photo.id, photo]));
  for (const id of ids) {
    const photo = listed.get(id);
    if (!photo) continue;
    for (const tile of document.querySelectorAll(`.tile[data-id="${id}"]`)) {
      tile.querySelectorAll('.mark').forEach(mark => mark.remove());
      tile.append(...tileMarks(photo));
      tile.classList.toggle('rejected', photo.flag === 'reject');
      tile.setAttribute('aria-label', tileName(photo));
      tile.querySelectorAll('.rate i').forEach((star, at) => star.classList.toggle('set', at < (photo.rating || 0)));
    }
  }
}

// Every month again at the row height, and a search's grid at its width;
// the photograph at the top of the page stays where it was.
function relayout() {
  document.documentElement.style.setProperty('--tile', tileWide() + 'px');
  const page = document.querySelector('main')?.getBoundingClientRect();
  const kept = page && document.elementFromPoint(page.left + page.width / 2, (document.querySelector('header')?.offsetHeight || 0) + 24)?.closest('.rows .tile');
  const was = kept?.getBoundingClientRect().top;
  for (const section of state.sections) {
    if (!section.rows) continue;
    if (section.photos) layout(section);
    else section.rows.style.height = estimate(section.count, section.rows.clientWidth || innerWidth) + 'px';
  }
  const now = kept && document.querySelector(`.rows .tile[data-id="${kept.dataset.id}"]`);
  if (now) scrollBy(0, now.getBoundingClientRect().top - was);
}

let resizing;
window.addEventListener('resize', () => {
  clearTimeout(resizing);
  resizing = setTimeout(relayout, 150);
});

// LIB-028: names narrow, the rest is read by the model; best first. The
// One filter narrows what was found as it narrows Photos (UX-081).
async function search(main, params) {
  const narrowed = narrowedBy(params);
  const filters = await filterRow(params, narrowed);
  main.append(filters.head, filters.row);
  let found;
  try { found = await api('search?' + new URLSearchParams(narrowed)); } catch (error) {
    main.append(empty(S.searchWaits, error.message, true));
    return;
  }
  if (!main.isConnected) return;
  filters.count.textContent = counted(found.length);
  if (!found.length) return main.append(empty(S.nothingFound, S.tryOther));
  const section = { photos: found, load: async () => section.photos };
  state.sections = [section];
  main.append(h('div', { class: 'grid' }, found.map(photo => tile(section, photo, null, tileWide()))));
}

async function people(main) {
  let list, unnamed;
  try { [list, unnamed] = await Promise.all([api('people'), api('faces').catch(() => [])]); } catch (error) { return main.append(empty(S.noPeople, error.message, true)); }
  if (!main.isConnected) return;
  if (!list.length && !unnamed.length) {
    return main.append(empty(S.noPeopleYet, S.peopleText));
  }
  if (list.length) {
    main.append(h('div', { class: 'places' }, list.map(person => h('button', { class: 'place', onclick: () => go('/', { person: person.name }) },
      h('div', { class: 'face' }, h('img', { src: thumb(person.portrait, 256), loading: 'lazy', alt: '' })),
      h('div', {}, h('div', { class: 'name' }, person.name), h('div', { class: 'caption' }, counted(person.count)))))));
  }
  if (unnamed.length) main.append(h('div', { class: 'section-label unnamed-label' }, S.notNamedYet), h('div', { class: 'unnamed' }, unnamed.map(nameRow)));
}

// LIB-014: a group of faces nobody has named — who is this, or nobody.
function nameRow(group) {
  const field = h('input', { type: 'text', placeholder: S.whoIsThis, 'aria-label': S.name, autocapitalize: 'words' });
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
    h('div', { class: 'ask' }, h('div', { class: 'caption' }, phrase('inPhotos', { photos: counted(group.photos) })), field, said),
    h('button', { class: 'chip', type: 'submit' }, S.name),
    h('button', { class: 'chip', type: 'button', title: S.nobodyTip, onclick: () => send('faces/ignore', { faces: group.faces }) }, S.nobody));
}

// A stack of three prints, from the first photographs the filter finds.
// `months`: the filter's timeline, already asked for quietly.
function stack(query, months) {
  const el = h('div', { class: 'stack' }, h('div', { class: 'print' }), h('div', { class: 'print' }), h('div', { class: 'print' }));
  const quiet = !!months;
  (async () => {
    const ids = [];
    for (const month of await (months || api('timeline?' + query))) {
      if (ids.length >= 3) break;
      ids.push(...(await api(`timeline/${month.month}?` + query, { quiet })).slice(0, 3 - ids.length).map(photo => photo.id));
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
  const list = await libraries().catch(error => (main.append(empty(S.noLibraries, error.message, true)), null));
  if (!list) return;
  if (!list.length) return main.append(empty(S.noLibrariesYet, S.librariesText));
  // An empty Sent Home is only a grey stack; it shows once something is sent.
  const shown = list.filter(library => library.count || !library.mine);
  main.append(libraryCommands(list), h('div', { class: 'places' }, shown.map(library => libraryShelf(library, list))));
}

async function albums(main) {
  let list;
  try { list = await api('albums'); } catch (error) { return main.append(empty(S.noAlbums, error.message, true)); }
  if (!list.length) {
    return main.append(h('div', { class: 'empty' }, h('h2', {}, S.noAlbumsYet),
      h('p', { class: 'body' }, S.albumsText),
      h('button', { class: 'primary', onclick: newAlbum }, S.newAlbumEll)));
  }
  main.append(h('div', { class: 'filters' }, h('button', { class: 'chip', onclick: newAlbum }, S.newAlbumEll)), h('div', { class: 'places' }, list.map(album =>
    collection(album.name, counted(album.count), 'album=' + encodeURIComponent(album.key), () => go('/', { album: album.key })))));
}

// HOME-032: an account may delete, move and remove photographs anywhere, as
// an admin may, when an admin lets it; an admin always may.
function rights(user, said) {
  const box = h('input', { type: 'checkbox', checked: user.manage, onchange: async () => {
    try { await api(`users/${encodeURIComponent(user.name)}`, { method: 'PUT', body: JSON.stringify({ manage: box.checked }) }); said.textContent = ''; } catch (error) {
      box.checked = !box.checked;
      said.textContent = error.message;
    }
  } });
  return h('label', { class: 'check' }, box, S.canDelete);
}

// HOME-037: an admin gives an account a password, to pass on; it is signed
// out everywhere and chooses its own at the next sign-in.
async function givePassword(user) {
  const field = h('input', { type: 'text', name: 'password', placeholder: S.passwordHint, required: true, minlength: 8, autocomplete: 'off', spellcheck: 'false' });
  const asked = await dialog(phrase('givePasswordQ', { name: user.name }), phrase('givePasswordText', { name: user.name }), [field], S.givePassword);
  if (!asked) return;
  try {
    await api(`users/${encodeURIComponent(user.name)}/password`, { method: 'PUT', body: JSON.stringify(asked) });
    toast(phrase('passwordGiven', { name: user.name }));
    render();
  } catch (error) { toast(error.message); }
}

async function accounts(main) {
  const users = await api('users').catch(error => (main.append(empty(S.accounts, error.message, true)), null));
  if (!users) return;
  const name = h('input', { type: 'text', placeholder: S.name, required: true, autocomplete: 'off' });
  const password = h('input', { type: 'password', placeholder: S.passwordHint, required: true, autocomplete: 'new-password' });
  const admin = h('input', { type: 'checkbox' });
  const said = h('p', { class: 'said', role: 'status', style: { textAlign: 'left' } });
  const form = h('form', { onsubmit: async event => {
    event.preventDefault();
    try {
      await api('users', { method: 'POST', body: JSON.stringify({ name: name.value.trim(), password: password.value, admin: admin.checked }) });
      render();
    } catch (error) { said.textContent = error.message; }
  } }, h('div', { class: 'section-label' }, S.newAccount), name, password,
  h('label', { class: 'check' }, admin, S.canMake), h('button', { class: 'primary', type: 'submit' }, S.makeAccount), said);
  main.append(h('div', { class: 'accounts' },
    h('div', {}, h('div', { class: 'section-label' }, S.accounts),
      h('ul', {}, users.map(user => h('li', {}, h('span', {}, user.name, user.must_change && h('span', { class: 'caption' }, ` · ${S.mustChange}`)),
        h('div', { class: 'acts' }, user.admin ? h('span', { class: 'caption' }, S.admin) : rights(user, said),
          user.name.toLowerCase() !== state.me.name.toLowerCase() && h('button', { class: 'chip', onclick: () => givePassword(user) }, S.givePasswordEll)))))),
    form,
    h('p', { class: 'body' }, S.accountsText)));
}

// The photograph large: arrow keys, a swipe on a phone, its information, and
// the file to take away.
function openViewer(section, at) {
  closeViewer(false);
  const opener = document.activeElement;
  // Where the page stands, so the cross can put it back: its scroll, and where
  // the tile this was opened from lay.
  const scroll = scrollY;
  const tile = tileOf(section.photos[at]?.id);
  const anchor = tile && { id: section.photos[at].id, top: tile.getBoundingClientRect().top + scrollY };
  const image = h('img', { alt: '', draggable: 'false' });
  const info = h('aside', { class: 'info', hidden: true });
  const edit = h('aside', { class: 'info edit', hidden: true, 'aria-label': S.edit });
  const hud = h('div', { class: 'hud', hidden: true });
  const badge = h('div', { class: 'zoom-badge', hidden: true });
  const noticed = h('div', { class: 'noticed', hidden: true });
  const infoButton = h('button', { class: 'round', 'aria-label': S.info, title: S.infoTip, onclick: () => toggleInfo() }, icon('info'));
  const editButton = h('button', { class: 'round', 'aria-label': S.edit, title: S.editTip, onclick: () => toggleEdit() }, icon('edit'));
  // Open the photograph in Numa on this computer or device, through the
  // numa:// link the apps register. There is no Numa for Android.
  const inNuma = !/Android/i.test(navigator.userAgent) && h('a', {
    class: 'chip open-in-numa', title: S.openTip,
  }, S.openInNuma);
  const corner = h('div', { class: 'corner right' }, inNuma, editButton, infoButton,
    h('button', { class: 'round', 'aria-label': S.download, onclick: event => {
      event.stopPropagation();
      const id = current().id;
      menu(corner, downloadItems(`/api/v1/photos/${id}/original`, `/api/v1/photos/${id}/jpeg`));
    } }, icon('download')));
  const bar = h('div', { class: 'loupe-bar', role: 'toolbar', 'aria-label': S.thisPhoto });
  const stage = h('div', { class: 'stage' }, image, hud, badge, noticed,
    h('div', { class: 'corner left' }, h('button', { class: 'round', 'aria-label': S.close, title: S.closeTip, onclick: leaveViewer }, icon('close'))),
    corner, bar,
    h('button', { class: 'round step back', 'aria-label': S.previous, title: S.previousTip, onclick: () => step(-1) }, icon('back')),
    h('button', { class: 'round step on', 'aria-label': S.next, title: S.nextTip, onclick: () => step(1) }, icon('on')));
  const el = h('div', { class: 'viewer', role: 'dialog', 'aria-modal': 'true', 'aria-label': S.photograph, tabindex: '-1' }, stage, info, edit);
  document.body.append(el);
  // What floats at the foot of the photograph — the loader, the toasts — is in
  // the loupe while it is open, centred where its bar is.
  stage.append(loader, toasts);
  holdFocus(true);
  el.focus({ preventScroll: true });
  document.body.style.overflow = 'hidden';
  document.body.classList.add('viewing');
  // A history entry of its own, so Back closes it; marked, so an entry left
  // by a viewer before a reload is not taken for this one.
  const mark = Date.now();
  state.viewer = { sections: state.sections, si: state.sections.indexOf(section), pi: at, photo: null, el, stage, image, info, hud, badge, noticed, infoButton, bar, edit, editButton, mark, opener, inNuma, scroll, anchor };
  history.pushState({ viewer: mark }, '');
  handle(stage, image);
  image.addEventListener('load', () => { if (state.viewer?.image === image && !state.viewer.zoom.full) fit(); });
  show();
}

// The photograph shown. The lists it was taken from are asked again after a
// change (`refreshAround`), so it is kept here and its place in them looked up
// by its number (`placeOf`), never by the object the list held.
const current = () => state.viewer.photo;

function show() {
  const viewer = state.viewer;
  const photo = viewer.sections[viewer.si].photos[viewer.pi];
  const sameLibrary = viewer.photo && libraryOf(viewer.photo.id) === libraryOf(photo.id);
  viewer.photo = photo;
  viewer.zoom = { s: 1, tx: 0, ty: 0, full: false, aspect: null };
  viewer.detail = null;
  if (viewer.inNuma) viewer.inNuma.href = `numa://home/open?server=${encodeURIComponent(location.origin)}&photo=${photo.id}`;
  apply();
  // The thumbnail at once, the 2048 preview over it when it has come.
  viewer.image.src = thumb(photo.id, 512);
  const large = new Image();
  large.onload = () => { if (state.viewer === viewer && viewer.photo.id === photo.id && !viewer.zoom.full) viewer.image.src = large.src; };
  large.src = versioned(`/api/v1/photos/${photo.id}/preview`, photo.id);
  leaveCrop();
  viewer.crops = null;
  viewer.image.style.filter = '';
  viewer.noticed.hidden = true;
  // The bar is there at once, never empty while the details come: the list's
  // stars, and the bin as the photograph before it had it if it is of the
  // same library (an admin's always is).
  viewer.manage = !!(state.me.admin || state.me.manage || (sameLibrary && viewer.manage));
  paintBar();
  if (!viewer.edit.hidden) fillEdit();
  // What only the photograph itself says — the flag, the camera — comes after;
  // a mark made before it has come waits for it (`setMarks`).
  viewer.ready = Promise.all([api('photos/' + photo.id), mayManage([photo.id])]).then(([detail, manage]) => {
    if (state.viewer !== viewer || viewer.photo.id !== photo.id) return detail;
    viewer.detail = detail;
    viewer.manage = manage;
    viewer.el.setAttribute('aria-label', named(detail));
    paintBar();
    if (!viewer.info.hidden) fillInfo();
    notice(viewer, detail);
    return detail;
  });
  viewer.ready.catch(() => {});
  holdLoupe();
}

// UX-082: what Numa saw of the photograph's sharpness, in the desktop loupe's
// words and place: a pill at the photograph's top left, and nothing when there
// is nothing to say. One Analyse has not reached yet is measured when asked,
// once it has been looked at for a moment rather than stepped past.
async function notice(viewer, detail) {
  const here = () => state.viewer === viewer && viewer.photo.id === detail.id;
  let said = detail.sharpness;
  if (!detail.measured) {
    await new Promise(done => setTimeout(done, 400));
    if (!here()) return;
    said = (await api(`photos/${detail.id}/sharpness`, { quiet: true }).catch(() => null))?.sharpness;
  }
  if (!said || !here()) return;
  viewer.noticed.textContent = said[0].toUpperCase() + said.slice(1);
  viewer.noticed.hidden = false;
}

// Where the photograph shown stands in the lists now, or null if it has left
// them (removed, moved).
function placeOf(viewer) {
  for (let si = 0; si < viewer.sections.length; si++) {
    const pi = viewer.sections[si].photos?.findIndex(one => one.id === viewer.photo.id) ?? -1;
    if (pi >= 0) return { si, pi };
  }
  return null;
}

// The place `by` photographs on from `from`, the months not asked for yet
// asked now; null past either end.
async function neighbour(viewer, { si, pi }, by) {
  pi += by;
  for (;;) {
    const section = viewer.sections[si];
    if (!section) return null;
    if (!section.photos) await section.load();
    if (pi < 0) {
      if (--si < 0) return null;
      const before = viewer.sections[si];
      if (!before.photos) await before.load();
      pi = before.photos.length - 1;
    } else if (pi >= section.photos.length) {
      if (++si >= viewer.sections.length) return null;
      pi = 0;
    } else return { si, pi };
  }
}

// Where a step starts: the photograph's own place, or, if it has left the
// lists, the gap it left, into which the one after it has moved.
const startOf = (viewer, by) => placeOf(viewer) || { si: viewer.si, pi: viewer.pi - (by > 0 ? 1 : 0) };

// The photograph has gone from the page: the one that took its place is shown,
// or the one before it, and when none is left the loupe closes. Asked for by
// whoever finds it gone first; the second asker waits for the same answer.
function goOn(viewer) {
  return viewer.going ||= (async () => {
    let at;
    try { at = await neighbour(viewer, startOf(viewer, 1), 1) || await neighbour(viewer, startOf(viewer, -1), -1); } catch (error) { toast(error.message); }
    if (state.viewer !== viewer) return;
    if (!at) return leaveViewer();
    Object.assign(viewer, at);
    show();
  })().finally(() => { viewer.going = null; });
}

// After a change to the page the photograph shown may be one that has left it.
async function keepShown(viewer) {
  if (state.viewer === viewer && !placeOf(viewer)) await goOn(viewer);
}

// Focus belongs inside the loupe while it is open: one that fell out of it
// (a control that was taken away, a menu or a question that closed) goes back.
function holdLoupe() {
  const viewer = state.viewer;
  if (viewer && !viewer.el.contains(document.activeElement) && !document.querySelector('dialog[open]')) viewer.el.focus({ preventScroll: true });
}

// Controls built afresh lose the focus they had: it goes to the one in the
// same place, so the keys go on where they were. Where what is built is another
// panel (`kind`: the Edit panel's crop tool and its sliders) the place means
// nothing, and a slider that took the focus would take the arrow keys; it goes
// to the control that opened it (`data-landing`), else back to the loupe.
function rebuilt(box, build, kind = '') {
  const same = (box.dataset.kind || '') === kind;
  const held = box.contains(document.activeElement);
  const at = same ? [...box.querySelectorAll('button, a, input')].indexOf(document.activeElement) : -1;
  build();
  box.dataset.kind = kind;
  if (at >= 0) box.querySelectorAll('button, a, input')[at]?.focus();
  else if (held && !same) { box.querySelector('[data-landing]')?.focus(); holdLoupe(); }
}

// Everything the keys do, at the photograph's foot: the stars (0 to 5), Pick
// and Reject (P, X; again for none, U), Delete, and under ⋯ the bar's other
// choices for chosen photographs.
function paintBar() {
  const viewer = state.viewer;
  const photo = viewer.detail || viewer.photo;
  const stars = [1, 2, 3, 4, 5].map(n => h('button', {
    class: 'star' + (n <= (photo.rating || 0) ? ' set' : ''), 'aria-label': starsOf(n), title: phrase('starTip', { stars: starsOf(n), n }),
    onclick: () => setMarks(marks => ({ rating: marks.rating === n ? 0 : n })),
  }, icon('star')));
  const flags = ['pick', 'reject'].map(flag => h('button', {
    class: 'round small flag-' + flag + (photo.flag === flag ? ' chosen' : ''), 'aria-label': FLAGS[flag], 'aria-pressed': String(photo.flag === flag),
    title: phrase('flagTip', { flag: FLAGS[flag], key: flag === 'pick' ? 'P' : 'X' }),
    onclick: () => cull(flag),
  }, icon(flag === 'pick' ? 'flag' : 'close')));
  // Move to Trash under ⋯ is the bin's: the loupe goes on to the next photograph.
  const actions = chosenActions([photo.id], viewer.manage, { erase: eraseShown });
  // `replaceChildren` would write a `false` out as text; `filter` leaves it out.
  rebuilt(viewer.bar, () => viewer.bar.replaceChildren(h('div', { class: 'stars' }, stars), h('div', { class: 'acts' }, flags,
    viewer.manage && h('button', { class: 'round small', 'aria-label': S.moveToTrash, title: phrase('moveTip', { key: DELETE_KEY }), onclick: eraseShown }, icon('trash')),
    h('button', { class: 'round small', 'aria-label': S.more, title: S.more, onclick: event => {
      event.stopPropagation();
      menuOver(event.currentTarget, [actions.album(), h('div', { class: 'rule' }), ...actions.more()]);
    } }, icon('more')))));
}

// Delete, with the same question as on the grid; then what came after it
// takes its place, or the one before it at the very end.
async function eraseShown() {
  const viewer = state.viewer;
  const photo = viewer?.photo;
  if (!photo || !viewer.manage) return;
  const sure = await ask(S.trashQ1, S.trashText1, S.moveToTrash);
  holdLoupe();
  if (!sure) return;
  const before = await trashIds();
  try { await api('photos/delete', { method: 'POST', body: JSON.stringify({ ids: [photo.id] }) }); } catch (error) { return toast(phrase('notMoved', { error: error.message })); }
  state.libraries = null;
  // The grid behind, while the photograph is still in its month to be found.
  refreshAround([photo.id]);
  trashed(before).then(undo => toast(S.movedToTrash, undo));
  if (state.viewer !== viewer) return;
  // It leaves every list at once; the months are asked again meanwhile.
  const gone = viewer.photo.id === photo.id;
  if (gone) Object.assign(viewer, placeOf(viewer));
  for (const section of viewer.sections) {
    const at = section.photos?.findIndex(one => one.id === photo.id) ?? -1;
    if (at >= 0) section.photos.splice(at, 1);
  }
  if (gone) await goOn(viewer);
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
  const { image, zoom, badge } = state.viewer;
  image.style.transform = `translate(${zoom.tx}px, ${zoom.ty}px) scale(${zoom.s})`;
  const zoomed = zoom.s > 1.001;
  state.viewer.stage.classList.toggle('zoomed', zoomed);
  // Zoom has only the wheel, a pinch and a double click to reach it; the badge
  // at the top centre says it is on, and how far.
  badge.hidden = !zoomed;
  if (zoomed) badge.textContent = `${percent()}\u00a0%`;
}

// How far in: the photograph's pixels against the screen's, one to one being
// 100 %. Fitted to the stage it is a fifth or so; `zoomTo` stops at 200 %.
function percent() {
  const { image, detail, zoom } = state.viewer;
  const sides = detail?.size?.match(/\d+/g)?.map(Number);
  const long = sides ? Math.max(...sides) : Math.max(image.naturalWidth, image.naturalHeight);
  return Math.round(zoom.s * Math.max(image.offsetWidth, image.offsetHeight) * (devicePixelRatio || 1) / long * 100);
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
  whole.onload = () => { if (state.viewer === viewer && viewer.photo.id === photo.id) viewer.image.src = whole.src; };
  whole.src = versioned(`/api/v1/photos/${photo.id}/jpeg`, photo.id);
}

// Double-click or double-tap for 1:1 where it was, the wheel or a pinch to
// zoom, a drag to pan; at fit, a swipe to the next. The fingers are taken
// anywhere on the stage, not only on the picture: a second finger on the
// dark band beside a tall photograph is still a pinch.
function handle(stage, image) {
  const pointers = new Map();
  let pinch = null, swipe = null, tapped = null, touched = 0;
  // A touch screen's double-tap is the taps' to handle; the browser's own
  // dblclick after it would zoom straight back out. The pointer is captured by
  // the stage, so the click's target is the stage, not the photograph: where it
  // landed says which.
  stage.addEventListener('dblclick', event => {
    if (event.target.closest('button, a, .menu, .info, .loupe-bar, .crop-tool') || Date.now() - touched < 800) return;
    const box = image.getBoundingClientRect();
    if (event.clientX >= box.left && event.clientX <= box.right && event.clientY >= box.top && event.clientY <= box.bottom) toggleZoom(event.clientX, event.clientY);
  });
  stage.addEventListener('wheel', event => {
    event.preventDefault();
    if (state.viewer.cropping) return;
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
    if (event.target.closest('button, a, .menu, .info, .loupe-bar, .crop-tool') || event.button > 0) return;
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
    const at = await neighbour(viewer, startOf(viewer, by), by);
    if (at && state.viewer === viewer) {
      Object.assign(viewer, at);
      show();
    }
  } catch (error) { toast(error.message); } finally { viewer.busy = false; }
}

const FLAGS = { pick: S.pick, reject: S.reject, none: S.noFlag };

// Stars and the flag, the household's as the catalog's are: on the bar, the
// tiles, and an event as a toast that can be undone. `wanted` is the change,
// or a function of the marks as they are, which is only known once the
// photograph's details have come.
async function setMarks(wanted) {
  const viewer = state.viewer;
  const photo = viewer.photo;
  let detail;
  try { detail = viewer.detail || await viewer.ready; } catch (error) { return toast(phrase('notSaved', { error: error.message })); }
  if (state.viewer !== viewer || viewer.photo.id !== photo.id) return;
  const change = typeof wanted === 'function' ? wanted(detail) : wanted;
  keepMarks(detail, { ...detail, ...change }, change, true);
}

// P and X, and the bar's flag and cross, as the desktop's loupe (CULL-015,
// UX-080): the mark, and on to the next photograph in the page's order; on one
// that has that mark already, the mark comes off and the loupe stays. One after
// another, so a second P goes to the photograph the first moved on to.
function cull(flag) {
  const viewer = state.viewer;
  const go = async () => {
    if (state.viewer !== viewer) return;
    const photo = viewer.photo;
    let on = false;
    await setMarks(marks => ({ flag: (on = marks.flag !== flag) ? flag : 'none' }));
    if (on && state.viewer === viewer && viewer.photo.id === photo.id) await step(1);
  };
  viewer.culling = (viewer.culling || Promise.resolve()).then(go, go);
}

// Kept at once, so a second key straight after builds on this one; undone,
// they are put back the same way, on the same photograph.
async function keepMarks(detail, wanted, change, undo) {
  const viewer = state.viewer;
  const before = { rating: detail.rating, flag: detail.flag };
  const marks = { rating: wanted.rating, flag: wanted.flag };
  Object.assign(detail, marks);
  const shown = () => state.viewer === viewer && viewer?.detail === detail;
  try {
    await api(`photos/${detail.id}/marks`, { method: 'PUT', body: JSON.stringify(marks) });
  } catch (error) {
    Object.assign(detail, before);
    if (shown()) paintBar();
    return toast(phrase('notSaved', { error: error.message }));
  }
  for (const section of state.sections) section.photos?.filter(photo => photo.id === detail.id).forEach(photo => { photo.rating = marks.rating; photo.flag = marks.flag; });
  remark([detail.id]);
  const back = Object.fromEntries(Object.keys(change).map(key => [key, before[key]]));
  toast(markSaid(change), undo && { label: S.undo, run: () => keepMarks(detail, { ...detail, ...back }, back, false) });
  if (shown()) {
    paintBar();
    if (!viewer.info.hidden) fillInfo();
  }
}

// The floating value, for a value only: one name, one value, gone 0.9 s
// after. What happened is a toast.
function flash(name, value) {
  const hud = state.viewer.hud;
  hud.replaceChildren(h('span', { class: 'section-label' }, name), h('span', { class: 'value' }, value));
  hud.hidden = false;
  clearTimeout(hud.timer);
  hud.timer = setTimeout(() => { hud.hidden = true; }, 900);
}

function toggleInfo() {
  const viewer = state.viewer;
  if (viewer.info.hidden && !viewer.edit.hidden) toggleEdit();
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
  // `replaceChildren` would write a `false` out as text; `h` leaves it out.
  viewer.info.replaceChildren(...[
    h('div', { class: 'title' }, photo.name),
    h('div', { class: 'meta' }, when(photo.taken)),
    (photo.camera || photo.lens || photo.exposure) && label(S.camera),
    photo.camera && h('div', {}, photo.camera),
    photo.lens && h('div', { class: 'meta' }, photo.lens),
    photo.exposure && h('div', { class: 'meta' }, photo.exposure),
    photo.size && label(S.size),
    photo.size && h('div', { class: 'meta' }, photo.size),
    photo.people.length > 0 && label(S.people),
    photo.people.length > 0 && chips(photo.people, person => go('/', { person })),
    photo.albums.length > 0 && label(S.albums),
    photo.albums.length > 0 && chips(photo.albums, name => go('/', { album: albums.find(album => album.name === name)?.key })),
    label(S.by), by,
    library && label(S.library),
    library && h('div', {}, libraryName(library)),
    // HOME-041: where it was taken, on a small map.
    photo.position && label(S.placeLabel),
    photo.place && h('div', {}, photo.place),
    photo.position && infoMap(photo.position),
    photo.edited && h('div', { class: 'caption', style: { marginTop: '14px' } }, S.shownWithEdits),
  ].filter(Boolean));
}

// Basic edits, kept in the photograph's own edit stack as Numa keeps them, so
// Numa opens it with them: Auto and the crops it offers, a quarter turn, and
// the seven sliders Auto sets or the light most needs. A slider shows its change at once, roughly, and Numa Home's
// render of it when that has come.
const SLIDERS = [
  [S.light, [['exposure', S.exposure, -5, 5, 0.05], ['contrast', S.contrast, -100, 100, 1], ['highlights', S.highlights, -100, 100, 1], ['shadows', S.shadows, -100, 100, 1], ['whites', S.whites, -100, 100, 1], ['blacks', S.blacks, -100, 100, 1]]],
  [S.colour, [['vibrance', S.vibrance, -100, 100, 1]]],
];
const signed = (value, places) => (value > 0 ? '+' : value < 0 ? '−' : '') + Math.abs(value).toFixed(places);

function toggleEdit() {
  const viewer = state.viewer;
  if (viewer.edit.hidden && !viewer.info.hidden) toggleInfo();
  viewer.edit.hidden = !viewer.edit.hidden;
  viewer.editButton.classList.toggle('chosen', !viewer.edit.hidden);
  if (viewer.edit.hidden) leaveCrop();
  if (!viewer.edit.hidden) fillEdit();
}

async function fillEdit() {
  const viewer = state.viewer;
  const photo = current();
  if (viewer.cropping) return paintCrop();
  let now;
  try {
    now = await api(`photos/${photo.id}/edit`);
    // As it was when Edit was opened on it, for Revert.
    if (viewer.before?.id !== photo.id) viewer.before = { id: photo.id, edits: (await api(`photos/${photo.id}/edits`)).edits };
  } catch (error) { return viewer.edit.replaceChildren(fault(error.message)); }
  if (state.viewer === viewer && viewer.photo.id === photo.id) paintEdit(now);
}

function paintEdit(now) {
  const viewer = state.viewer;
  const saved = now.sliders;
  const inputs = {};
  // Roughly, until the render comes: the browser's own brightness, contrast
  // and saturation by how far each slider is from what was saved.
  const rough = () => {
    const by = key => Number(inputs[key].value) - saved[key];
    viewer.image.style.filter = `brightness(${2 ** by('exposure')}) contrast(${1 + by('contrast') / 200}) saturate(${1 + by('vibrance') / 200})`;
  };
  const slider = ([key, name, min, max, step]) => {
    const places = step < 1 ? 2 : 0;
    const input = h('input', { type: 'range', min, max, step, value: saved[key], 'aria-label': name });
    const value = h('span', { class: 'value' });
    const fill = () => {
      const at = (Number(input.value) - min) / (max - min) * 100, zero = -min / (max - min) * 100;
      input.style.setProperty('--from', `${Math.min(at, zero)}%`);
      input.style.setProperty('--to', `${Math.max(at, zero)}%`);
      value.textContent = signed(Number(input.value), places);
    };
    inputs[key] = input;
    fill();
    let timer, viaKey = false;
    // A value moved by the keys is away from the row's own dot: the floating value says it.
    input.addEventListener('keydown', () => { viaKey = true; });
    input.addEventListener('pointerdown', () => { viaKey = false; });
    input.addEventListener('input', () => { fill(); rough(); if (viaKey) flash(name, value.textContent); });
    // Kept when let go; the arrow keys, a moment after the last.
    input.addEventListener('change', () => { clearTimeout(timer); timer = setTimeout(() => editPhoto({ set: { [key]: Number(input.value) } }), 250); });
    input.addEventListener('dblclick', () => { input.value = 0; fill(); rough(); editPhoto({ set: { [key]: 0 } }); });
    return h('label', { class: 'slider', title: S.doubleClickNone }, h('span', { class: 'name' }, name), value, input);
  };
  const crops = viewer.crops;
  const chosenCrop = crops && (crops.chosen === 0 ? S.asShotLong : crops.offered[crops.chosen - 1].why);
  const cropRow = crops && (crops.offered.length
    ? [h('div', { class: 'chips' }, [S.asShot, ...crops.offered.map((_, at) => String(at + 1))].map((label, at) => h('button', {
      class: 'chip small' + (crops.chosen === at ? ' chosen' : ''), title: at ? crops.offered[at - 1].why : S.asShotTip,
      onclick: () => { crops.chosen = at; editPhoto({ crop: at ? crops.offered[at - 1].rect : crops.asShot }); },
    }, label))), h('div', { class: 'caption' }, chosenCrop)]
    : h('div', { class: 'caption' }, S.noCrop));
  rebuilt(viewer.edit, () => viewer.edit.replaceChildren(...[
    h('div', { class: 'title' }, S.edit),
    h('div', { class: 'tools' },
      h('button', { class: 'chip', title: S.autoTip, onclick: () => editPhoto({ auto: true }) }, S.auto),
      h('button', { class: 'chip', title: S.straightenTip, onclick: () => straighten() }, S.straighten),
      h('button', { class: 'chip', title: S.cropTip, onclick: enterCrop, 'data-landing': '' }, S.crop),
      h('button', { class: 'round small', 'aria-label': S.turnLabel, title: S.turnTip, onclick: () => turn(1) }, icon('turn'))),
    crops && h('div', { class: 'section-label' }, S.crop), cropRow,
    SLIDERS.map(([section, list]) => [h('div', { class: 'section-label' }, section), list.map(slider)]),
    h('div', { class: 'tools foot' }, h('button', { class: 'chip', title: S.revertTip, onclick: () => revert() }, S.revert)),
    h('div', { class: 'caption' }, S.editNote),
  ].flat(Infinity).filter(Boolean)), 'edit');
}

// One change to a photograph's edits, kept on the server. Whether it is kept is
// said (a failure as a toast, in words); the picture and its tile follow
// wherever the loupe has gone meanwhile, for the edit is kept either way.
// `photo` is the one it is for: an Undo comes later, perhaps from another.
async function editPhoto(change, photo = current()) {
  const viewer = state.viewer;
  let now;
  try { now = await api(`photos/${photo.id}/edit`, { method: 'POST', body: JSON.stringify(change) }); } catch (error) { toast(phrase('notSaved', { error: error.message })); return false; }
  if (state.viewer === viewer && viewer?.photo.id === photo.id) {
    // Crops offered were worked out on the frame as it was.
    if (change.auto) viewer.crops = { offered: now.crops, asShot: now.crop || [0, 0, 1, 1], chosen: 0 };
    else if (change.turn || change.turns || now.straightened) viewer.crops = null;
    if (!viewer.edit.hidden) paintEdit(now);
  }
  redrawn(photo, now.updated);
  if (change.auto) toast(S.autoSet);
  return now;
}

// A quarter turn, kept before it is said: "Turned", with Undo, which turns it
// back on the same photograph.
async function turn(quarters, photo = current()) {
  if (await editPhoto({ turns: quarters }, photo)) toast(quarters > 0 ? S.turned : S.turnedBack, quarters > 0 && { label: S.undo, run: () => turn(-quarters, photo) });
}

// UX-082: Straighten, one tap, for whoever only wants the horizon right: the
// level as Auto sets it and the light left as it is. Said as a toast; its Undo
// puts back the edits it was made on.
async function straighten(photo = current()) {
  let was;
  try { was = (await api(`photos/${photo.id}/edits`)).edits; } catch (error) { return toast(phrase('notSaved', { error: error.message })); }
  const now = await editPhoto({ level: true }, photo);
  if (!now) return;
  if (now.straightened == null) return toast(S.noHorizon);
  if (!now.straightened) return toast(S.alreadyLevel);
  toast(phrase('straightened', { angle: Math.abs(now.straightened).toFixed(1) }), { label: S.undo, run: () => revert(photo, was) });
}

// Back to `edits`: as they were when Edit was opened, or as an Undo kept them.
async function revert(photo = current(), edits = state.viewer.before?.id === photo.id ? state.viewer.before.edits : null) {
  const viewer = state.viewer;
  let answer;
  try { answer = await api(`photos/${photo.id}/edits`, { method: 'PUT', body: edits ? JSON.stringify(edits) : '' }); } catch (error) { return toast(phrase('notSaved', { error: error.message })); }
  redrawn(photo, answer.updated);
  if (state.viewer === viewer && viewer?.photo.id === photo.id) {
    viewer.crops = null;
    fillEdit();
  }
}

// Crop & Straighten, as the desktop's crop tool: the whole frame turned by
// the angle, the box over it to drag by its corners, edges or middle, its
// ratios (again for portrait or landscape) and the straighten slider. The
// box stays on the photograph; the server keeps it there too.
const RATIOS = [[S.original, null], [S.free, 0], ['1:1', 1], ['5:4', 5 / 4], ['3:2', 3 / 2], ['16:9', 16 / 9]];

async function enterCrop() {
  const viewer = state.viewer;
  const photo = current();
  // Asked once: a second press while the frame is being made would leave a
  // tool behind that nothing can close.
  if (viewer.cropping || viewer.entering) return;
  viewer.entering = true;
  try { await openCrop(viewer, photo); } finally { viewer.entering = false; }
}

async function openCrop(viewer, photo) {
  waiting(false, 1);
  let response = null;
  try { response = await fetch(`/api/v1/photos/${photo.id}/frame`, { credentials: 'same-origin' }); } catch (_) { /* said below */ } finally { waiting(false, -1); }
  if (!response?.ok) {
    const why = response && (await response.json().catch(() => null))?.error;
    return toast(why ? `${S.frameFail}. ${why}` : S.frameFail);
  }
  const [left, top, wide, high, angle] = (response.headers.get('X-Numa-Crop') || '0,0,1,1,0').split(',').map(Number);
  const src = URL.createObjectURL(await response.blob());
  if (state.viewer !== viewer || viewer.photo.id !== photo.id) return URL.revokeObjectURL(src);
  const image = h('img', { alt: '', draggable: 'false' });
  await new Promise(done => { image.onload = image.onerror = done; image.src = src; });
  if (!image.naturalWidth) {
    URL.revokeObjectURL(src);
    return toast(S.frameFail);
  }
  const size = [image.naturalWidth, image.naturalHeight];
  const box = h('div', { class: 'crop-box' }, ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'].map(edge => h('i', { class: 'handle ' + edge, 'data-edge': edge })));
  const frame = h('div', { class: 'crop-frame' }, image, box);
  const tool = h('div', { class: 'crop-tool' }, frame);
  const c = { photo, view: [left, top, wide, high], wanted: [left, top, wide, high], was: { view: [left, top, wide, high], angle }, angle, ratio: 0, landscape: wide * size[0] >= high * size[1], image, box, frame, tool, size, src };
  viewer.cropping = c;
  viewer.stage.append(tool);
  viewer.el.classList.add('cropping');
  // The frame is fitted to what is free: on a phone the panel is a sheet over
  // the foot of the photograph, and the frame lies above it, where its
  // bottom edge and corners can be reached.
  const fitFrame = () => {
    const room = tool.getBoundingClientRect();
    const sheet = viewer.edit.hidden || getComputedStyle(viewer.edit).position !== 'absolute' ? 0 : Math.max(0, room.bottom - viewer.edit.getBoundingClientRect().top);
    tool.style.paddingBottom = `${sheet}px`;
    // Room round it for the handles to be taken by: on a phone, where the
    // screen is narrow and a handle is 44 wide anyway, less at the sides.
    const side = matchMedia('(pointer: coarse)').matches ? 56 : 112;
    const k = Math.max(0.01, Math.min((room.width - side) / size[0], (room.height - sheet - 112) / size[1]));
    Object.assign(frame.style, { width: `${size[0] * k}px`, height: `${size[1] * k}px` });
  };
  c.watch = new ResizeObserver(fitFrame);
  c.watch.observe(tool);
  c.watch.observe(viewer.edit);
  box.addEventListener('pointerdown', event => dragCrop(event, c));
  if (viewer.edit.hidden) toggleEdit(); else paintCrop();
  fitFrame();
  paintCropBox();
}

function leaveCrop() {
  const viewer = state.viewer;
  const c = viewer?.cropping;
  if (!c) return;
  c.watch.disconnect();
  c.tool.remove();
  URL.revokeObjectURL(c.src);
  viewer.cropping = null;
  viewer.el.classList.remove('cropping');
  if (!viewer.edit.hidden) fillEdit();
}

// Done: the frame kept. Once, however often it is pressed; and a frame left as
// it was is not written again, for the server would take it through the tool's
// geometry and back and move it by a rounding.
async function keepCrop() {
  const viewer = state.viewer;
  const c = viewer.cropping;
  if (!c || c.saving) return;
  if (c.angle === c.was.angle && c.view.every((value, at) => Math.abs(value - c.was.view[at]) < 1e-4)) return leaveCrop();
  c.saving = true;
  let now;
  try { now = await api(`photos/${c.photo.id}/edit`, { method: 'POST', body: JSON.stringify({ view: c.view, angle: c.angle }) }); } catch (error) {
    c.saving = false;
    return toast(phrase('notSaved', { error: error.message }));
  }
  if (state.viewer === viewer) {
    viewer.crops = null;
    leaveCrop();
  }
  redrawn(c.photo, now.updated);
}

function paintCropBox() {
  const c = state.viewer.cropping;
  const [x, y, w, h] = c.view;
  Object.assign(c.box.style, { left: `${x * 100}%`, top: `${y * 100}%`, width: `${w * 100}%`, height: `${h * 100}%` });
  c.image.style.transform = `rotate(${-c.angle}deg)`;
}

// Whether every corner of the box lands on the photograph, turned about
// the frame's centre as the core turns it (`crop_from_view`).
// ponytail: without a keystone's stretch; the server's `crop_inside` has it.
function onPhoto([x, y, w, h], angle, [width, height]) {
  const a = angle * Math.PI / 180, cos = Math.cos(a), sin = Math.sin(a);
  return [[x, y], [x + w, y], [x, y + h], [x + w, y + h]].every(([u, v]) => {
    const vx = (u - 0.5) * width, vy = (v - 0.5) * height;
    return Math.abs(cos * vx - sin * vy) <= width / 2 + 0.5 && Math.abs(sin * vx + cos * vy) <= height / 2 + 0.5;
  });
}

// The box pulled in about its middle until it is all photograph.
function shrunk(view, angle, size) {
  if (onPhoto(view, angle, size)) return view;
  const [x, y, w, h] = view;
  const at = k => [x + w * (1 - k) / 2, y + h * (1 - k) / 2, w * k, h * k];
  let [lo, hi] = [0, 1];
  for (let i = 0; i < 24; i++) { const mid = (lo + hi) / 2; if (onPhoto(at(mid), angle, size)) lo = mid; else hi = mid; }
  return at(lo);
}

// The chosen ratio, as width over height in pixels; 0 is free.
function cropRatio(c) {
  if (c.ratio === null) return c.size[0] / c.size[1];
  if (!c.ratio) return 0;
  return c.landscape ? c.ratio : 1 / c.ratio;
}

// A ratio held while an edge moves: it sets the other side about the middle.
// (A corner is `cornered`.)
function held([x, y, w, h], edge, ratio, [width, height]) {
  const k = ratio * height / width;
  if (edge === 'n' || edge === 's') { const across = h * k; return [x + (w - across) / 2, y, across, h]; }
  const down = w / k;
  return [x, y + (h - down) / 2, w, down];
}

// The box a drag makes when its hand has gone `p` across and `q` down (fractions
// of the frame): the box moved, or with the edges the handle holds moved; a
// locked ratio holds by the sides the handle does not.
function dragged([x, y, w, h], edge, p, q, c) {
  if (edge === 'move') return [x + p, y + q, w, h];
  let [l, t, r, b] = [x, y, x + w, y + h];
  if (edge.includes('w')) l += p;
  if (edge.includes('e')) r += p;
  if (edge.includes('n')) t += q;
  if (edge.includes('s')) b += q;
  const ratio = cropRatio(c);
  return ratio ? held([l, t, r - l, b - t], edge, ratio, c.size) : [l, t, r - l, b - t];
}

// A corner of a box of locked ratio, the opposite corner staying: the box of
// height `h + t` and the width its ratio gives.
function cornered([x, y, w, h], edge, t, k) {
  const down = h + t, across = k * down;
  return [edge.includes('w') ? x + w - across : x, edge.includes('n') ? y + h - down : y, across, down];
}

// How far a hand's travel of `want` (a signed fraction) takes the box while
// `ok` holds of it. All of it, but where it meets a limit, as far as the limit,
// so a box slides along an edge of the photograph instead of stopping at it;
// and flush with the limit when it lies within `near` beyond `want` (the snap).
// Nothing holds a box once the hand is further than `near` short of a limit,
// and a hand moving away lets go at once: it is worked out from the hand's
// place alone, never from where the box was.
function slide(want, near, ok) {
  const to = want + Math.sign(want) * near;
  if (ok(to)) return want;
  let [lo, hi] = [0, to];
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    if (ok(mid)) lo = mid; else hi = mid;
  }
  return lo;
}

// Within this many screen pixels of the photograph's edge a side of the box is
// drawn flush to it.
const SNAP = 6;
const SMALLEST = 0.05;

// Where a drag has got to: the box the hand asks for, kept on the photograph
// and no smaller than a twentieth of it.
function settle(c, start, edge, dx, dy, room) {
  const ok = box => box[2] >= SMALLEST && box[3] >= SMALLEST && onPhoto(box, c.angle, c.size);
  const ratio = cropRatio(c);
  if (ratio && edge.length === 2) {
    // A corner of a locked ratio follows the hand along the box's diagonal.
    const k = ratio * c.size[1] / c.size[0];
    const out = (edge.includes('w') ? -dx : dx) * k + (edge.includes('n') ? -dy : dy);
    const t = slide(out / (k * k + 1), SNAP / Math.max(room.height, k * room.width), value => ok(cornered(start, edge, value, k)));
    return cornered(start, edge, t, k);
  }
  const across = edge === 'move' || /[we]/.test(edge), down = edge === 'move' || /[ns]/.test(edge);
  const p = across ? slide(dx, SNAP / room.width, value => ok(dragged(start, edge, value, 0, c))) : 0;
  const q = down ? slide(dy, SNAP / room.height, value => ok(dragged(start, edge, p, value, c))) : 0;
  return dragged(start, edge, p, q, c);
}

function dragCrop(event, c) {
  // One hand at a time, the main button.
  if (event.button > 0 || !event.isPrimary || c.dragging) return;
  event.preventDefault();
  event.stopPropagation();
  const edge = event.target.dataset.edge || 'move';
  const room = c.frame.getBoundingClientRect();
  const from = { x: event.clientX, y: event.clientY, view: shrunk(c.view, c.angle, c.size) };
  try { c.box.setPointerCapture(event.pointerId); } catch (_) { /* a synthetic pointer */ }
  c.dragging = true;
  c.tool.classList.add('dragging');
  const move = moved => {
    c.view = c.wanted = settle(c, from.view, edge, (moved.clientX - from.x) / room.width, (moved.clientY - from.y) / room.height, room);
    paintCropBox();
  };
  const up = () => {
    for (const type of ['pointermove', 'pointerup', 'pointercancel', 'lostpointercapture']) c.box.removeEventListener(type, type === 'pointermove' ? move : up);
    c.tool.classList.remove('dragging');
    c.dragging = false;
  };
  c.box.addEventListener('pointermove', move);
  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) c.box.addEventListener(type, up);
}

// A ratio chosen: the largest box of it about the box's middle, on the
// photograph.
function chooseRatio(c, ratio) {
  if (c.ratio === ratio && ratio) c.landscape = !c.landscape;
  c.ratio = ratio;
  const k = cropRatio(c) * c.size[1] / c.size[0];
  if (k) {
    const [x, y, w, h] = c.view;
    let [across, down] = k >= 1 ? [1, 1 / k] : [k, 1];
    const cx = Math.min(1 - across / 2, Math.max(across / 2, x + w / 2)), cy = Math.min(1 - down / 2, Math.max(down / 2, y + h / 2));
    c.wanted = [cx - across / 2, cy - down / 2, across, down];
    c.view = shrunk(c.wanted, c.angle, c.size);
  }
  paintCropBox();
  paintCrop();
}

function paintCrop() {
  const viewer = state.viewer;
  const c = viewer.cropping;
  const input = h('input', { type: 'range', min: -15, max: 15, step: 0.1, value: c.angle, 'aria-label': S.straighten });
  const value = h('span', { class: 'value' });
  const fill = () => {
    const at = (Number(input.value) + 15) / 30 * 100;
    input.style.setProperty('--from', `${Math.min(at, 50)}%`);
    input.style.setProperty('--to', `${Math.max(at, 50)}%`);
    value.textContent = `${signed(Number(input.value), 1)}°`;
  };
  let viaKey = false;
  const straighten = () => { c.angle = Number(input.value); c.view = shrunk(c.wanted, c.angle, c.size); fill(); paintCropBox(); if (viaKey) flash(S.straighten, value.textContent); };
  input.addEventListener('keydown', () => { viaKey = true; });
  input.addEventListener('pointerdown', () => { viaKey = false; });
  input.addEventListener('input', straighten);
  input.addEventListener('dblclick', () => { input.value = 0; straighten(); });
  fill();
  rebuilt(viewer.edit, () => viewer.edit.replaceChildren(
    h('div', { class: 'title' }, S.cropTitle),
    h('div', { class: 'section-label' }, S.ratio),
    h('div', { class: 'chips' }, RATIOS.map(([label, ratio]) => h('button', {
      class: 'chip small' + (c.ratio === ratio ? ' chosen' : ''), title: ratio ? S.ratioTip : label === S.original ? S.ownShape : S.anyShape,
      onclick: () => chooseRatio(c, ratio),
    }, label))),
    h('label', { class: 'slider', title: S.doubleClickNone }, h('span', { class: 'name' }, S.straighten), value, input),
    h('div', { class: 'tools' }, h('button', { class: 'chip', title: S.resetTip, onclick: () => {
      c.ratio = 0; c.angle = 0; c.view = c.wanted = [0, 0, 1, 1]; paintCropBox(); paintCrop();
    } }, S.reset)),
    h('div', { class: 'tools foot' }, h('button', { class: 'chip', title: S.cancelTip, onclick: leaveCrop }, S.cancel),
      h('button', { class: 'primary', title: S.doneTip, onclick: keepCrop }, S.done)),
    h('div', { class: 'caption' }, S.dragHint)), 'crop');
}

// The picture as Numa Home renders it now, fitted afresh as a turn or a crop
// changes its shape: in the loupe, if it is still on this photograph, and on the
// tile behind, which takes the new shape and the rows are laid out again. Each
// time with a number of its own, the latest being the one drawn; a picture
// that takes its time is waited for under the loader.
let redraws = 0;
const newest = new Map();
function redrawn(photo, version) {
  const viewer = state.viewer;
  const bust = `${version}.${++redraws}`;
  newest.set(photo.id, bust);
  versions.set(photo.id, bust);
  const latest = () => newest.get(photo.id) === bust;
  if (viewer) {
    const large = new Image();
    waiting(false, 1);
    large.onerror = () => waiting(false, -1);
    large.onload = () => {
      waiting(false, -1);
      if (!latest() || state.viewer !== viewer || viewer.photo.id !== photo.id) return;
      viewer.zoom = { s: 1, tx: 0, ty: 0, full: false, aspect: null };
      viewer.image.src = large.src;
      viewer.image.style.filter = '';
    };
    large.src = versioned(`/api/v1/photos/${photo.id}/preview`, photo.id);
  }
  const small = new Image();
  small.onload = () => { if (latest()) reshape(photo.id, small.naturalWidth / small.naturalHeight); };
  small.src = thumb(photo.id, 256);
  for (const img of document.querySelectorAll(`.tile[data-id="${photo.id}"] img`)) img.src = img.src.replace(/(&v=[\d.]+)?$/, `&v=${bust}`);
}

// A turn or a crop changes a photograph's shape: its tile takes the new one and
// the page's rows are laid out again.
function reshape(id, shape) {
  for (const section of state.sections) {
    const photo = section.photos?.find(one => one.id === id);
    if (!photo || Math.abs(aspect(photo) / shape - 1) < 0.01) continue;
    photo.aspect = shape;
    if (section.el) layout(section);
  }
}

// The loupe is modal: the page behind it is inert, so Tab stays among its
// controls and a screen reader reads nothing else; Escape closes it and the
// focus goes back to the tile it came from.
function holdFocus(on) {
  for (const el of [app, ...document.querySelectorAll('body > .selection')]) el.inert = on;
}

// The control a photograph has on the page: its tile, or in Tidy Up its Look
// Closer.
const tileOf = id => id != null && document.querySelector(`.tile[data-id="${id}"], .frame[data-id="${id}"] .look`);

// The cross, Escape: back to the page the photograph was opened from. The
// loupe has a history entry of its own, so going back is what closes it; if
// that entry is no longer the current one it is closed directly.
function leaveViewer() {
  const viewer = state.viewer;
  if (viewer && history.state?.viewer === viewer.mark) history.back(); else closeViewer();
}

// The photograph and the grid cross-fade over 300 ms: the closed one stays
// a moment over the page, fading, and takes nothing more. The page is as it
// was: its scroll, with the photograph last viewed in sight and its tile
// focused.
function closeViewer(back = true) {
  if (!state.viewer) return;
  const viewer = state.viewer;
  const { el } = viewer;
  document.body.append(loader, toasts);
  if (viewer.cropping) {
    viewer.cropping.watch.disconnect();
    URL.revokeObjectURL(viewer.cropping.src);
  }
  state.viewer = null;
  holdFocus(false);
  document.body.style.overflow = '';
  document.body.classList.remove('viewing');
  el.inert = true;
  el.classList.add('leaving');
  setTimeout(() => el.remove(), 300);
  // Closed by going elsewhere (a page asked for, a sign-in) the page behind is not the one it came from.
  if (!back) return;
  // A page with no months of its own was changed from the loupe: shown anew.
  if (viewer.stale) return Promise.resolve(render()).then(() => backToPage(viewer));
  backToPage(viewer);
  // Once more when the browser has done its own scroll for the way back.
  requestAnimationFrame(() => backToPage(viewer));
}

// The part of the window the page leaves free: under its header, above a
// phone's tab bar.
function freeBand() {
  const nav = document.querySelector('.places-nav');
  const tabs = nav && getComputedStyle(nav).position === 'fixed' && nav.offsetWidth > innerWidth / 2;
  return [document.querySelector('header')?.offsetHeight || 0, tabs ? nav.getBoundingClientRect().top : innerHeight];
}

// The scroll the page had, kept by where the tile the loupe was opened from
// stands now (a month asked for meanwhile can have moved it); then the
// photograph viewed last in sight, if it is not, and its tile focused.
function backToPage(viewer) {
  if (state.viewer) return;
  let y = viewer.scroll;
  const opened = viewer.anchor && tileOf(viewer.anchor.id);
  if (opened) y += opened.getBoundingClientRect().top + scrollY - viewer.anchor.top;
  scrollTo(0, Math.max(0, y));
  const last = tileOf(viewer.photo?.id);
  if (!last) return viewer.opener?.isConnected && viewer.opener.focus({ preventScroll: true });
  const [top, bottom] = freeBand();
  const box = last.getBoundingClientRect();
  if (box.top < top || box.bottom > bottom) scrollBy(0, box.top - top - Math.max(0, (bottom - top - box.height) / 2));
  last.focus({ preventScroll: true });
}

window.addEventListener('popstate', () => { if (state.viewer && history.state?.viewer !== state.viewer.mark) closeViewer(); });
// The desktop's keys, where they mean the same here.
document.addEventListener('keydown', event => {
  if (!state.viewer || document.querySelector('dialog[open]')) return;
  const key = event.key.toLowerCase();
  // A field has its own keys, a slider the arrows; Escape still closes.
  if (event.target.matches?.('input, select, textarea') && !(key === 'escape' && event.target.matches('[type=range]'))) return;
  if ((key === 'delete' || key === 'backspace') && !event.ctrlKey && !event.altKey) {
    event.preventDefault();
    return eraseShown();
  }
  if (event.ctrlKey || event.metaKey || event.altKey) return;
  const zoom = state.viewer.zoom;
  if (state.viewer.cropping) {
    if (key === 'escape') leaveCrop();
    // Enter keeps the frame, unless a button has the focus: it is that button's.
    else if (key === 'enter') { if (event.target.closest?.('button, a')) return; keepCrop(); }
    else if (!key.startsWith('arrow')) return;
    return event.preventDefault();
  }
  if (key === 'arrowright') step(1);
  else if (key === 'arrowleft') step(-1);
  else if (key === 'escape') {
    if (openMenu) closeMenu();
    else if (zoom.s > 1.001) { zoom.s = 1; clamp(); apply(); } else leaveViewer();
  } else if (key === 'i') toggleInfo();
  else if (key === 'e') toggleEdit();
  else if (key === 'c') enterCrop();
  else if (key === 'a' && !state.viewer.edit.hidden) editPhoto({ auto: true });
  else if (/^[0-5]$/.test(key)) setMarks({ rating: Number(key) });
  else if (key === 'p') cull('pick');
  else if (key === 'x') cull('reject');
  else if (key === 'u') setMarks({ flag: 'none' });
  else return;
  event.preventDefault();
});

// HOME-012: whose camera is whose — every camera in the archive, its count,
// the account it is linked to, and what its EXIF Artist says.
async function cameras(main) {
  // The frame at once, the cameras when they come.
  const waiting = h('div', { class: 'accounts cameras' }, h('div', {}, h('div', { class: 'section-label' }, S.cameras), h('p', { class: 'body' }, S.loading)));
  main.append(waiting);
  const [list, users] = await Promise.all([api('cameras', { wait: 15 }), api('users', { wait: 15 })])
    .catch(error => (waiting.replaceWith(empty(S.cameras, error.message, true)), [null, null]));
  if (!list) return;
  waiting.remove();
  const said = h('p', { class: 'said', role: 'status', style: { textAlign: 'left' } });
  const link = async (camera, account) => {
    try { await api('cameras', { method: 'PUT', body: JSON.stringify({ camera, account }) }); render(); } catch (error) { said.textContent = error.message; }
  };
  const rows = list.map(camera => {
    const select = h('select', { 'aria-label': S.whoseCamera, onchange: () => link(camera.camera, select.value || null) },
      h('option', { value: '' }, S.nobody), users.map(user => h('option', { value: user.name, selected: user.name === camera.account }, user.name)));
    const hint = camera.hint && camera.hint !== camera.account
      ? h('button', { class: 'chip', title: phrase('linkTo', { name: camera.hint }), onclick: () => link(camera.camera, camera.hint) }, phrase('artistSays', { name: camera.hint }))
      : camera.artist && !camera.hint && h('span', { class: 'caption' }, phrase('artistIs', { name: camera.artist }));
    return h('li', {}, h('div', {}, h('div', {}, [camera.make, camera.model].filter(Boolean).join(' ')),
      h('div', { class: 'caption' }, [camera.serial && phrase('serial', { n: camera.serial }), counted(camera.count)].filter(Boolean).join(' · '))),
    h('div', { class: 'link' }, hint, select));
  });
  main.append(h('div', { class: 'accounts cameras' },
    h('div', {}, h('div', { class: 'section-label' }, S.cameras),
      list.length ? h('ul', {}, rows) : h('p', { class: 'body' }, S.camerasEmpty)),
    said,
    h('p', { class: 'body' }, S.camerasText)));
}

start();
