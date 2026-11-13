'use strict';

const token = location.pathname.split('/')[2] || '';
const base = `/api/v1/t/${token}`;
const main = document.querySelector('main');
const ICONS = {
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
  el.append(...children.flat(Infinity).filter(child => child != null && child !== false));
  return el;
}
function icon(name) {
  const box = document.createElement('span');
  box.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${ICONS[name]}</svg>`;
  return box.firstChild;
}

const grouped = n => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
const counted = n => `${grouped(n)} ${n === 1 ? S.photo : S.photos}`;
const picture = (n, edge) => `${base}/${n}?edge=${edge}`;
const altOf = photo => photo.caption || phrase('proofOpen', { n: photo.n + 1 });
const pause = ms => new Promise(done => setTimeout(done, ms));

const narrow = matchMedia('(max-width: 699px)').matches;
const part = (date, options) => date.toLocaleString(S.locale, { ...options, timeZone: 'UTC' });
const dateOf = day => new Date(day * 86400000);
const dayNumber = seconds => Math.floor(seconds / 86400);

function daysSpan(from, to) {
  const a = dateOf(from), b = dateOf(to);
  const d = date => date.getUTCDate(), y = date => date.getUTCFullYear();
  const m = date => part(date, { month: narrow ? 'short' : 'long' });
  if (y(a) !== y(b)) return `${d(a)} ${m(a)} ${y(a)} – ${d(b)} ${m(b)} ${y(b)}`;
  return from === to ? `${d(a)} ${m(a)} ${y(a)}` : m(a) === m(b) ? `${d(a)}–${d(b)} ${m(b)} ${y(b)}` : `${d(a)} ${m(a)} – ${d(b)} ${m(b)} ${y(b)}`;
}

function chapterSpan(from, to) {
  const a = dateOf(from), b = dateOf(to);
  const d = date => date.getUTCDate(), m = date => part(date, { month: narrow ? 'short' : 'long' });
  return from === to ? `${d(a)} ${m(a)}` : m(a) === m(b) ? `${d(a)}–${d(b)} ${m(b)}` : `${d(a)} ${m(a)} – ${d(b)} ${m(b)}`;
}
const dayName = text => {
  const date = new Date(text + 'T00:00:00Z');
  return `${part(date, { weekday: 'short' })} ${date.getUTCDate()} ${part(date, { month: 'short' })}`;
};
const longDate = seconds => new Date(seconds * 1000).toLocaleDateString(S.locale, { day: 'numeric', month: 'long', year: 'numeric' });
const today = () => Math.floor((Date.now() - new Date().getTimezoneOffset() * 60000) / 86400000);

function chapterLine(chapter) {
  if (!chapter.days) return '';
  const [from, to] = chapter.days;
  const now = today();
  return [chapterSpan(from, to), to > from && phrase(to - from === 1 ? 'oneNight' : 'nights', { n: to - from }), now >= from && now <= to && S.now].filter(Boolean).join(' · ');
}

function factsOf(trip) {
  const days = [...trip.chapters.flatMap(chapter => chapter.days || []), ...(trip.first != null ? [dayNumber(trip.first), dayNumber(trip.last)] : [])];
  const bits = [];
  if (days.length) bits.push(daysSpan(Math.min(...days), Math.max(...days)));
  if (trip.route) {
    if (trip.chapters.length) bits.push(phrase('placesOfTotal', { n: trip.chapters.filter(chapter => chapter.count).length, m: trip.chapters.length }));
    if (trip.km) bits.push(phrase('km', { n: grouped(trip.km) }));
  } else bits.push(counted(trip.count));
  return bits.join(' · ');
}

function picturing(img, src, tries = 0) {
  img.addEventListener('error', async () => {
    if (tries >= 12) return;
    await pause(Math.min(1200 * (tries + 1), 9000));
    if (img.isConnected) picturing(img, src, tries + 1);
  }, { once: true });
  img.src = src;
}

let all = [];
let opener = null;

function shot(photo, edge, ratio) {
  const img = h('img', { alt: altOf(photo), loading: 'lazy', decoding: 'async', width: Math.round(photo.aspect * 100), height: 100 });
  picturing(img, picture(photo.n, edge));
  const button = h('button', { class: 'shot', type: 'button', 'aria-label': altOf(photo), onclick: event => { opener = event.currentTarget; loupe(photo.n); } }, img);
  button.style.setProperty('--a', ratio ?? photo.aspect);
  return button;
}

function photosOf(list) {
  const parts = [];
  let run = [], sum = 0;
  const flush = () => {
    if (!run.length) return;
    const row = h('div', { class: 'row' }, run.map(photo => shot(photo, 1024)));
    row.style.setProperty('--sum', sum);
    parts.push(row);
    run = []; sum = 0;
  };
  for (const photo of list) {
    if (photo.caption) {
      flush();
      const wide = h('figure', { class: 'wide' }, shot(photo, 1024, 1), h('figcaption', {}, photo.caption));
      wide.style.setProperty('--w', Math.min(16 / 9, Math.max(5 / 4, photo.aspect)));
      parts.push(wide);
    } else {
      run.push(photo);
      sum += photo.aspect;
      if (sum >= 4 || run.length === 4) flush();
    }
  }
  flush();
  return parts.length ? h('div', { class: 'photos' }, parts) : null;
}

const prints = list => h('div', { class: 'stack' }, [2, 1, 0].map(at => h('div', { class: 'print' },
  list[at] && h('img', { src: picture(list[at].n, 1024), loading: 'lazy', alt: '' }))));

function chapterBlock(chapter, at) {
  return h('section', { id: `chapter-${at}` }, h('div', { class: 'label' }, chapterLine(chapter)), h('h2', {}, chapter.name),
    chapter.words && h('p', { class: 'words' }, chapter.words), photosOf(chapter.photos));
}

function dayBlock(day) {
  return h('section', {}, h('div', { class: 'label' }, dayName(day.day) + (day.places?.length ? ' · ' + day.places.join(', ') : '')), photosOf(day.photos));
}

function render(trip) {
  all = [...trip.chapters.flatMap(chapter => chapter.photos), ...trip.days.flatMap(day => day.photos)];
  const lead = all.find(photo => photo.caption) || all[0];
  const chapters = trip.chapters.map((chapter, at) => [chapter, at]);
  const shown = chapters.filter(([chapter]) => chapter.photos.length);
  const ahead = chapters.filter(([chapter]) => !chapter.photos.length).map(([chapter]) => chapter.name);
  const mapped = trip.route && chapters.some(([chapter]) => chapter.place);
  const map = mapped && h('div', { class: 'map' }, h('div', { class: 'canvas', role: 'img', 'aria-label': S.tripMap }), h('div', { class: 'caption' }, S.osmCredit));
  main.replaceChildren(
    lead && coverOf(lead),
    h('header', { class: 'column' }, h('div', { class: 'label' }, phrase('tripBy', { name: trip.by })), h('h1', {}, trip.name), h('div', { class: 'facts' }, factsOf(trip))),
    map,
    h('div', { class: 'column story' },
      shown.map(([chapter, at]) => chapterBlock(chapter, at)),
      trip.days.filter(day => day.photos.length).map(dayBlock),
      ahead.length > 0 && h('section', {}, h('div', { class: 'label' }, S.aheadHead), h('p', { class: 'ahead-line' }, ahead.join(' · ')))),
    h('footer', {}, h('span', { class: 'made' }, h('img', { class: 'mark', src: '/icon.svg', alt: '' }), S.tripMadeWith),
      h('span', {}, phrase('tripEndsOn', { date: longDate(trip.expires) }))));
  if (map) requestAnimationFrame(() => drawMap(map.firstChild, trip));
}

function coverOf(photo) {
  const button = shot(photo, 1024);
  button.classList.add('cover');
  return button;
}

async function drawMap(box, trip) {
  let lib;
  try { lib = await loadMapLibre(); } catch (_) { box.parentNode?.remove(); return; }
  if (!box.isConnected) return;
  try {
    const css = getComputedStyle(document.documentElement);
    const colour = name => css.getPropertyValue(`--${name}`).trim();
    const stops = trip.chapters.map((chapter, at) => ({ chapter, at, lngLat: chapter.place && [chapter.place.position[1], chapter.place.position[0]] })).filter(stop => stop.lngLat);
    const points = stops.map(stop => stop.lngLat);
    if (!points.length) { box.parentNode?.remove(); return; }
    const west = Math.min(...points.map(point => point[0])), east = Math.max(...points.map(point => point[0]));
    const south = Math.min(...points.map(point => point[1])), north = Math.max(...points.map(point => point[1]));
    const fine = matchMedia('(min-width: 900px) and (pointer: fine)').matches;
    const map = new lib.Map({ container: box, style: mapStyle([`${location.origin}${base}/map/{z}/{x}/{y}`]), attributionControl: false, dragRotate: false,
      pitchWithRotate: false, touchPitch: false, renderWorldCopies: false, minZoom: 1, maxZoom: 14, interactive: fine, center: [(west + east) / 2, (south + north) / 2], zoom: 4 });
    map.on('error', () => {});
    if (fine) { map.scrollZoom.disable(); map.touchZoomRotate?.disableRotation(); map.keyboard?.disableRotation(); }

    const fit = () => map.fitBounds([[west, south], [east, north]], { padding: narrow ? 44 : 72, maxZoom: 9, duration: 0 });
    fit();
    map.on('resize', fit);

    const lines = [];
    for (let at = 1; at < stops.length; at++) {
      lines.push({ type: 'Feature', properties: { ahead: !(stops[at - 1].chapter.photos.length && stops[at].chapter.photos.length) },
        geometry: { type: 'LineString', coordinates: [stops[at - 1].lngLat, stops[at].lngLat] } });
    }
    let added = false;
    const draw = () => {
      if (added || !map.getStyle()) return;
      added = true;
      map.addSource('route', { type: 'geojson', data: { type: 'FeatureCollection', features: lines } });
      const layout = { 'line-cap': 'round', 'line-join': 'round' };
      map.addLayer({ id: 'route-ahead', type: 'line', source: 'route', filter: ['==', ['get', 'ahead'], true], layout, paint: { 'line-color': colour('label'), 'line-width': 2.2, 'line-dasharray': [0.1, 2.2] } });
      map.addLayer({ id: 'route-done', type: 'line', source: 'route', filter: ['==', ['get', 'ahead'], false], layout, paint: { 'line-color': colour('ink'), 'line-width': 2.4 } });
    };
    if (map.isStyleLoaded()) draw(); else { map.once('style.load', draw); map.once('load', draw); }
    const toChapter = at => document.getElementById(`chapter-${at}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    for (const { chapter, at, lngLat } of stops) {
      const name = h('span', { class: 'pin-name' }, chapter.name);
      const element = chapter.photos.length
        ? h('button', { class: 'pin', type: 'button', 'aria-label': `${chapter.name}, ${counted(chapter.count)}`, onclick: () => toChapter(at) },
          prints(chapter.photos.slice(0, 3)), h('span', { class: 'count' }, grouped(chapter.count)), name)
        : h('span', { class: 'dot', role: 'img', 'aria-label': chapter.name }, name);
      new lib.Marker({ element }).setLngLat(lngLat).addTo(map);
    }
  } catch (_) { box.parentNode?.remove(); }
}

function loupe(n) {
  let at = Math.max(0, all.findIndex(photo => photo.n === n));
  const box = h('dialog', { class: 'loupe', 'aria-label': S.tripLarge });
  const stage = h('div', { class: 'stage' });
  const words = h('p', { class: 'words' });
  const where = h('div', { class: 'where caption' });
  const show = to => {
    at = (to + all.length) % all.length;
    const photo = all[at];
    const img = h('img', { alt: altOf(photo) });
    picturing(img, picture(photo.n, 2048));
    stage.replaceChildren(img);
    words.textContent = photo.caption || '';
    words.hidden = !photo.caption;
    where.textContent = `${at + 1} / ${all.length}`;
  };
  const step = by => all.length > 1 && h('button', { class: 'round step ' + (by < 0 ? 'back' : 'on'), type: 'button', 'aria-label': by < 0 ? S.proofPrevious : S.proofNext,
    onclick: () => show(at + by) }, icon(by < 0 ? 'back' : 'on'));
  box.append(stage, h('button', { class: 'round close', type: 'button', 'aria-label': S.close, onclick: () => box.close() }, icon('close')),
    step(-1), step(1), words, where);
  box.addEventListener('keydown', event => {
    if (event.key === 'ArrowLeft') show(at - 1);
    if (event.key === 'ArrowRight') show(at + 1);
  });

  const onPicture = (x, y) => {
    const img = stage.firstChild, frame = img.getBoundingClientRect();
    const k = img.naturalWidth ? Math.min(frame.width / img.naturalWidth, frame.height / img.naturalHeight) : 0;
    const [w, v] = k ? [img.naturalWidth * k, img.naturalHeight * k] : [frame.width, frame.height];
    const left = frame.left + (frame.width - w) / 2, top = frame.top + (frame.height - v) / 2;
    return x >= left && x <= left + w && y >= top && y <= top + v;
  };

  let from = null;
  stage.addEventListener('pointerdown', event => { from = [event.clientX, event.clientY]; });
  stage.addEventListener('pointerup', event => {
    if (!from) return;
    const [dx, dy] = [event.clientX - from[0], event.clientY - from[1]];
    from = null;
    if (Math.abs(dx) > 50 && Math.abs(dx) > 2 * Math.abs(dy)) return show(at + (dx < 0 ? 1 : -1));
    if (Math.abs(dx) < 8 && Math.abs(dy) < 8 && !onPicture(event.clientX, event.clientY)) box.close();
  });
  stage.addEventListener('pointercancel', () => { from = null; });
  box.addEventListener('close', () => { box.remove(); opener?.focus(); });
  document.body.append(box);
  box.showModal();
  show(at);
}

function closed() {
  document.title = S.tripClosed;
  main.className = 'closed';
  main.replaceChildren(h('img', { class: 'mark', src: '/icon.svg', alt: '' }), h('h1', {}, S.tripClosed));
}

async function asked(tries = 0) {
  const answer = await fetch(base).catch(() => null);
  if (answer?.status !== 429 || tries >= 8) return answer;
  await pause(1500 * (tries + 1));
  return asked(tries + 1);
}

async function start() {
  const answer = await asked();
  let trip = null;
  try { trip = answer?.ok ? await answer.json() : null; } catch (_) { trip = null; }
  if (!trip || !Array.isArray(trip.chapters)) return closed();
  document.title = trip.name;
  try { render(trip); } catch (_) { closed(); }
}
start();
