// HOME-041: Places — where the household's photographs were taken: the
// countries with their regions and towns, and a map of them; and a small map
// in a photograph's info. The map is Numa Home's own: OpenStreetMap's data as
// Protomaps builds it, kept on the NAS, drawn by MapLibre GL JS (BSD-3-Clause,
// licensed on /licences) in the design system's map greys, so the page asks
// no server but Numa Home and the photographs stay the brightest thing.
'use strict';

const MAPLIBRE = '/map/6.13.0/';
let mapLibrary = null;
function loadMapLibre() {
  if (!mapLibrary) {
    document.head.append(h('link', { rel: 'stylesheet', href: MAPLIBRE + 'maplibre-gl.css' }));
    mapLibrary = import(MAPLIBRE + 'maplibre-gl.mjs');
    mapLibrary.catch(() => { mapLibrary = null; });
  }
  return mapLibrary;
}

// The map's look, from the tokens, so it follows the theme. The world to zoom
// 6 is everywhere and drawn larger past it; the detail kept around the
// photographs lies over it from zoom 7. Lines and names come only from the
// one that has them sharp.
function mapStyle() {
  const css = getComputedStyle(document.documentElement);
  const token = name => css.getPropertyValue(`--map-${name}`).trim();
  const kind = (...kinds) => ['in', ['get', 'kind'], ['literal', kinds]];
  const label = (colour, size, more) => ({
    layout: { 'text-field': ['coalesce', ['get', 'name:en'], ['get', 'name']], 'text-font': ['Noto Sans Regular'], 'text-size': size, 'text-max-width': 8, ...more },
    paint: { 'text-color': colour, 'text-halo-color': token('land'), 'text-halo-width': 1.2 },
  });
  // Seas and countries as the section labels are set: small, spaced, upper case.
  const spaced = { 'text-transform': 'uppercase', 'text-letter-spacing': 0.12 };
  const layers = [{ id: 'sea', type: 'background', paint: { 'background-color': token('water') } }];
  for (const source of ['world', 'here']) {
    const sharp = source === 'world' ? { maxzoom: 7 } : {};
    const layer = (id, type, from, more) => ({ id: `${source}-${id}`, type, source, 'source-layer': from, ...more });
    layers.push(
      layer('land', 'fill', 'earth', { paint: { 'fill-color': token('land') } }),
      layer('water', 'fill', 'water', { filter: ['==', ['geometry-type'], 'Polygon'], paint: { 'fill-color': token('water') } }),
      layer('roads', 'line', 'roads', { ...sharp, minzoom: 11, filter: kind('minor_road'),
        paint: { 'line-color': token('road'), 'line-width': ['interpolate', ['exponential', 1.6], ['zoom'], 11, 0.5, 18, 6] } }),
      layer('major', 'line', 'roads', { ...sharp, filter: kind('highway', 'major_road'),
        paint: { 'line-color': token('road-major'), 'line-width': ['interpolate', ['exponential', 1.6], ['zoom'], 5, 0.4, 18, 10] } }),
      layer('regions', 'line', 'boundaries', { ...sharp, minzoom: 4, filter: kind('region'), paint: { 'line-color': token('border'), 'line-width': 0.6, 'line-dasharray': [2, 2] } }),
      layer('borders', 'line', 'boundaries', { ...sharp, filter: kind('country'), paint: { 'line-color': token('border'), 'line-width': 1 } }),
      layer('seas', 'symbol', 'water', { ...sharp, filter: ['==', ['geometry-type'], 'Point'], ...label(token('label'), 11, spaced) }),
      layer('countries', 'symbol', 'places', { ...sharp, filter: kind('country'), ...label(token('label'), 11, spaced) }),
      layer('towns', 'symbol', 'places', { ...sharp, filter: kind('locality'),
        ...label(token('label-place'), ['interpolate', ['linear'], ['zoom'], 4, 11, 12, 13], { 'symbol-sort-key': ['get', 'min_zoom'] }) }),
    );
  }
  const tiles = [`${location.origin}/api/v1/map/{z}/{x}/{y}`];
  return {
    version: 8, glyphs: `${location.origin}/map/fonts/{fontstack}/{range}.pbf`, layers,
    sources: { world: { type: 'vector', tiles, maxzoom: 6 }, here: { type: 'vector', tiles, minzoom: 7, maxzoom: 14 } },
  };
}

// The points as countries, then regions, then towns: each with its count,
// its first three photographs, the box its photographs lie in, and where its
// stack stands (a town where its photographs are, the others on their
// largest town). A region of one town is that town.
function placeTree(found) {
  const countries = new Map();
  const child = (kids, name, level, parent) => {
    if (!kids.has(name)) kids.set(name, { name, level, parent, key: (parent ? parent.key + '/' : '') + name, count: 0, ids: [], lat: 0, lon: 0, kids: new Map(), box: [180, 90, -180, -90] });
    return kids.get(name);
  };
  for (const [id, lat, lon, at] of found.points) {
    const [town, region, country] = found.towns[at];
    const land = child(countries, country, 'country', null);
    const area = region ? child(land.kids, region, 'region', land) : land;
    const here = child(area.kids, town, 'town', area);
    for (const node of new Set([land, area, here])) {
      node.count += 1;
      if (node.ids.length < 3) node.ids.push(id);
      node.box = [Math.min(node.box[0], lon), Math.min(node.box[1], lat), Math.max(node.box[2], lon), Math.max(node.box[3], lat)];
    }
    here.lat += lat;
    here.lon += lon;
  }
  const finish = node => {
    node.kids = [...node.kids.values()].sort((a, b) => b.count - a.count);
    node.kids.forEach(finish);
    node.kids = node.kids.map(kid => kid.level === 'region' && kid.kids.length === 1 ? Object.assign(kid.kids[0], { parent: node }) : kid);
    if (node.level === 'town') [node.lat, node.lon] = [node.lat / node.count, node.lon / node.count];
    else [node.lat, node.lon] = [node.kids[0].lat, node.kids[0].lon];
  };
  const tree = [...countries.values()].sort((a, b) => b.count - a.count);
  tree.forEach(finish);
  return tree;
}

// Which places stand on the map at a zoom: a country opens to its parts from
// zoom 4, a region to its towns from 6, each only when its parts' stacks lie
// apart, so a stack never covers one of its own. Places of different
// countries that would still cover each other stand as one stack, which
// zooms in to them.
const OPENS = { country: 4, region: 6 };
const APART = 64;
function shownAt(tree, zoom) {
  const size = 512 * 2 ** zoom;
  const at = node => [(node.lon + 180) / 360 * size, (1 - Math.asinh(Math.tan(node.lat * Math.PI / 180)) / Math.PI) / 2 * size];
  const apart = kids => kids.every((a, i) => kids.slice(i + 1).every(b => {
    const [ax, ay] = at(a), [bx, by] = at(b);
    return Math.hypot(ax - bx, ay - by) >= APART;
  }));
  const shown = [];
  const visit = node => node.kids.length && zoom >= OPENS[node.level] && apart(node.kids) ? node.kids.forEach(visit) : shown.push(node);
  tree.forEach(visit);
  const stacks = [];
  for (const node of shown.sort((a, b) => b.count - a.count)) {
    const [x, y] = at(node);
    const near = stacks.find(stack => Math.hypot(stack.x - x, stack.y - y) < APART);
    if (near) near.places.push(node);
    else stacks.push({ x, y, places: [node] });
  }
  return stacks.map(stack => stack.places);
}

// The page's own filter for a place: a region is asked as one, a town or a country as the place.
const placeFilter = node => ({ [node.level === 'region' ? 'region' : 'place']: node.name });

// The map is made once and kept, with where it looks, while the page is
// rebuilt around it (a chip, a visit elsewhere and back).
let mapView = null;

async function placesPage(main, params) {
  state.sections = [];
  main.classList.add('map-page');
  const filters = narrowedBy(params).filter(([key]) => key !== 'place' && key !== 'region');
  const query = new URLSearchParams(filters).toString();
  const view = mapView || (mapView = { box: h('div', { class: 'map', role: 'region', 'aria-label': S.placesMap }), markers: new Map(), open: new Set() });
  view.filters = filters;
  closePlace(view);
  const head = h('button', { class: 'list-head', 'aria-expanded': 'false', onclick: () => {
    const open = !list.classList.contains('open');
    list.classList.toggle('open', open);
    head.setAttribute('aria-expanded', String(open));
  } }, h('h1', {}, S.places), h('span', { class: 'caption' }));
  const rows = h('div', { class: 'rows' });
  const list = h('div', { class: 'place-list' }, head, rows);
  view.streets = coming('streets');
  view.streets.box.hidden = view.row?.state !== 'fetching';
  drawComing(view.streets, view.row, S.mapStreets);
  const stage = h('div', { class: 'map-stage' }, view.box, mapChips(params), view.streets.box,
    h('div', { class: 'zoom' },
      h('button', { class: 'round', 'aria-label': S.zoomIn, title: S.zoomIn, onclick: () => view.map?.zoomIn() }, icon('plus')),
      h('button', { class: 'round', 'aria-label': S.zoomOut, title: S.zoomOut, onclick: () => view.map?.zoomOut() }, icon('minus'))),
    h('div', { class: 'credit' }, h('a', { href: 'https://www.openstreetmap.org/copyright', target: '_blank', rel: 'noreferrer' }, S.osmCredit), ` · ${S.protomapsCredit}`));
  main.append(list, stage);
  let found;
  try { found = await api('places' + (query ? '?' + query : '')); } catch (error) {
    rows.append(fault(error.message));
    return;
  }
  if (!main.isConnected) return;
  const tree = placeTree(found);
  if (!filters.length && !found.looking) navLists.places = found.towns.length;
  head.lastChild.textContent = tree.length ? phrase(tree.length === 1 ? 'oneCountry' : 'countries', { n: tree.length }) : '';
  view.tree = tree;
  view.rows = rows;
  view.looking = found.looking;
  fillPlaces(view);
  if (found.none || found.looking) {
    list.append(h('p', { class: 'caption none' }, found.looking ? S.lookingWhere : phrase(found.none === 1 ? 'oneNoPlace' : 'noPlace', { n: grouped(found.none) })));
  }
  // Still reading where the photographs were taken: asked again in a moment.
  if (found.looking) setTimeout(() => { if (main.isConnected && route().path === '/places') render(); }, 4000);
  if (found.map !== 'ready') {
    stage.replaceChildren(h('div', { class: 'note' }, mapNote(view, found)));
    return;
  }
  view.total = 0;
  let lib;
  try { lib = await loadMapLibre(); } catch (error) {
    stage.replaceChildren(h('div', { class: 'note' }, empty(S.places, error.message, true)));
    return;
  }
  if (!main.isConnected) return;
  const at = (params.get('at') || '').split(',').map(Number);
  if (!view.map) {
    view.lib = lib;
    view.map = new lib.Map({ container: view.box, style: mapStyle(), attributionControl: false, dragRotate: false, pitchWithRotate: false, touchPitch: false,
      renderWorldCopies: false, minZoom: 1, maxZoom: 16, center: [0, 30], zoom: 1.5 });
    view.map.touchZoomRotate.disableRotation();
    view.map.keyboard.disableRotation();
    let pending = false;
    view.map.on('zoom', () => {
      if (pending) return;
      pending = true;
      requestAnimationFrame(() => { pending = false; placeStacks(view); });
    });
    view.map.on('move', () => view.follow?.());
    view.map.on('click', () => closePlace(view));
    view.fitted = false;
  } else {
    view.map.resize();
  }
  if (at.length === 2 && at.every(Number.isFinite)) view.map.jumpTo({ center: [at[1], at[0]], zoom: 12 });
  else if (!view.fitted && tree.length) {
    // Every place, or where most were taken when every place is more than
    // the map can show; clear of the chips, and of the sheet on a phone.
    const box = tree.reduce((all, node) => [Math.min(all[0], node.box[0]), Math.min(all[1], node.box[1]), Math.max(all[2], node.box[2]), Math.max(all[3], node.box[3])], [180, 90, -180, -90]);
    const padding = { top: 80, left: 60, right: 60, bottom: list.offsetParent && getComputedStyle(list).position === 'absolute' ? list.offsetHeight + 40 : 60 };
    const fits = view.map.cameraForBounds([[box[0], box[1]], [box[2], box[3]]], { padding });
    const [west, south, east, north] = fits && fits.zoom >= 1.5 ? box : tree[0].box;
    view.map.fitBounds([[west, south], [east, north]], { padding, maxZoom: 10, duration: 0 });
  }
  view.fitted = true;
  for (const marker of view.markers.values()) marker.remove();
  view.markers.clear();
  placeStacks(view);
}

// What stands in the map's place until it has come: how far its download is,
// or what stopped it and Try Again, which has Numa Home fetch it now.
function mapNote(view, found) {
  if (found.map === 'fetching') {
    view.note = coming('world');
    drawComing(view.note, view.row, S.mapBringing);
    return h('div', { class: 'empty' }, h('h2', {}, S.places), h('p', { class: 'body' }, S.mapOnce), view.note.box);
  }
  view.note = null;
  view.total = 0;
  const box = empty(S.places, found.why || S.mapMissing, Boolean(found.why));
  box.append(h('button', { class: 'primary', onclick: () => api('map/bring', { method: 'POST' }).then(render, error => toast(error.message)) }, S.tryAgain));
  return box;
}

// A download of the map: what comes and how far, the activity ring turning
// until its size is known, then its meter.
function coming(kind) {
  const turning = ring(0.25), said = h('span', {}), meter = h('div', { class: 'meter' }, h('div', { class: 'fill' }));
  return { turning, said, meter, box: h('div', { class: 'coming ' + kind }, h('div', { class: 'line' }, turning, said), meter) };
}

function drawComing(block, row, lead) {
  const total = row?.state === 'fetching' ? row.total : 0;
  const megabytes = bytes => grouped(Math.round(bytes / 1e6));
  block.said.textContent = total ? `${lead} ${phrase('ofTotal', { done: megabytes(row.done), total: `${megabytes(total)} MB` })}` : lead;
  block.turning.toggleAttribute('hidden', Boolean(total));
  block.meter.hidden = !total;
  block.meter.firstChild.style.transform = filled(total ? row.done / total : 0).transform;
}

// The map's download as Activity reports it, asked every half second while
// it comes (activityBadge). The world's on the note, the page drawn again
// once that download is over — another one, the streets', begins only once
// the world is here; the streets around new places in a caption over the
// map, their tiles asked again once they have come.
function mapProgress(activity) {
  const row = activity.downloads.find(item => item.name === S.map);
  const fetching = row?.state === 'fetching', view = mapView;
  if (!view) return fetching;
  view.row = row;
  if (view.note?.box.isConnected) {
    if (!fetching || (view.total && row.total !== view.total)) {
      view.total = 0;
      render();
      return false;
    }
    view.total = row.total;
    drawComing(view.note, row, S.mapBringing);
    return true;
  }
  if (view.map && view.streets?.box.isConnected) {
    if (!fetching && !view.streets.box.hidden) view.map.getSource('here')?.setTiles([`${location.origin}/api/v1/map/{z}/{x}/{y}`]);
    view.streets.box.hidden = !fetching;
    drawComing(view.streets, row, S.mapStreets);
  }
  return fetching;
}

// The places on the map as small stacks of their photographs, the count beside.
function placeStacks(view) {
  if (!view.map || !view.tree) return;
  const shown = shownAt(view.tree, view.map.getZoom());
  const keyOf = places => places.map(node => node.key).join('|');
  const keys = new Set(shown.map(keyOf));
  for (const [key, marker] of view.markers) {
    if (!keys.has(key)) {
      marker.remove();
      view.markers.delete(key);
    }
  }
  for (const places of shown) {
    const key = keyOf(places), [node] = places;
    if (view.markers.has(key)) continue;
    const count = places.reduce((sum, one) => sum + one.count, 0);
    // Several places as one: a click zooms in until they stand apart.
    const open = event => {
      event.stopPropagation();
      if (places.length === 1) return choosePlace(view, node, false);
      closePlace(view);
      const box = places.reduce((all, one) => [Math.min(all[0], one.box[0]), Math.min(all[1], one.box[1]), Math.max(all[2], one.box[2]), Math.max(all[3], one.box[3])], [180, 90, -180, -90]);
      view.map.fitBounds([[box[0], box[1]], [box[2], box[3]]], { padding: 120, maxZoom: Math.max(view.map.getZoom() + 2, 6) });
    };
    const pin = h('button', { class: 'pin' + (view.chosen?.key === key ? ' chosen' : ''), 'aria-label': `${places.map(one => one.name).join(', ')}, ${counted(count)}`, style: { zIndex: String(count) }, onclick: open },
      prints(node.ids), h('span', { class: 'count' }, grouped(count)));
    view.markers.set(key, new view.lib.Marker({ element: pin }).setLngLat([node.lon, node.lat]).addTo(view.map));
  }
}

// The list: every country with its count, opening to its regions and towns.
function fillPlaces(view) {
  const row = (node, top) => {
    const chosen = view.chosen?.key === node.key;
    const name = h('button', { class: (top ? 'place-link' : 'place-entry') + (chosen ? ' chosen' : ''), 'aria-current': chosen ? 'true' : null,
      onclick: () => choosePlace(view, node, true) }, h('span', { class: 'name' }, node.name), h('span', { class: 'caption' }, grouped(node.count)));
    if (!node.kids.length) return name;
    const open = view.open.has(node.key);
    const fold = h('button', { class: 'place-fold', 'aria-expanded': String(open), 'aria-label': phrase('placesIn', { name: node.name }), onclick: () => {
      if (open) view.open.delete(node.key); else view.open.add(node.key);
      fillPlaces(view);
    } }, icon('on'));
    return h('div', { class: 'place-group' + (open ? ' open' : '') }, h('div', { class: 'place-head' }, name, fold),
      h('div', { class: 'place-entries', inert: !open }, h('div', { class: 'rail' }, node.kids.map(kid => row(kid, false)))));
  };
  view.rows.replaceChildren(...(view.tree.length ? view.tree.map(node => row(node, true)) : view.looking ? [] : [h('p', { class: 'body' }, S.noPlaces)]));
}

// A place chosen, in the list or on the map: its row and stack white, and
// over the map its name, its count and months, three of its photographs and
// Show N, the timeline of it with the page's filters. From the list, the map
// goes to it first.
function choosePlace(view, node, fly) {
  closePlace(view);
  view.chosen = node;
  // Its parts in the list too, when it was chosen there.
  for (let up = fly ? node : node.parent; up; up = up.parent) view.open.add(up.key);
  fillPlaces(view);
  view.rows.querySelector('[aria-current]')?.scrollIntoView({ block: 'nearest' });
  view.markers.get(node.key)?.getElement().classList.add('chosen');
  if (!view.map) return;
  if (fly) {
    const [west, south, east, north] = node.box;
    view.map.fitBounds([[west, south], [east, north]], { padding: 120, maxZoom: node.level === 'town' ? 12 : 9 });
  }
  const filter = placeFilter(node);
  const caption = h('span', { class: 'caption' }, counted(node.count));
  api('timeline?' + new URLSearchParams([...view.filters, ...Object.entries(filter)]), { quiet: true })
    .then(months => { if (months.length && months[0].month !== 'all') caption.textContent = `${counted(node.count)} · ${spanOf(months)}`; }, () => {});
  const pop = h('div', { class: 'map-pop', role: 'dialog', 'aria-label': node.name },
    h('div', { class: 'head' }, h('span', { class: 'name' }, node.name), caption),
    h('div', { class: 'strip' }, node.ids.map(id => h('img', { src: thumb(id, 256), alt: '' }))),
    h('button', { class: 'primary', onclick: () => go('/', { ...Object.fromEntries(view.filters), ...filter }) }, phrase('showN', { n: grouped(node.count) })));
  pop.addEventListener('click', event => event.stopPropagation());
  view.box.parentNode.append(pop);
  view.pop = pop;
  // Above its stack, inside the map; under it when the top is too near.
  view.follow = () => {
    const { x, y } = view.map.project([node.lon, node.lat]);
    const { width, height } = pop.getBoundingClientRect();
    const room = view.box.getBoundingClientRect();
    const top = y - height - 40 < 8 ? y + 40 : y - height - 40;
    Object.assign(pop.style, { left: `${Math.max(8, Math.min(x - width / 2, room.width - width - 8))}px`, top: `${Math.max(8, Math.min(top, room.height - height - 8))}px` });
  };
  view.follow();
}

function closePlace(view) {
  view.pop?.remove();
  view.pop = view.follow = null;
  if (view.chosen) view.markers.get(view.chosen.key)?.getElement().classList.remove('chosen');
  view.chosen = null;
  if (view.rows?.isConnected) fillPlaces(view);
}
document.addEventListener('keydown', event => { if (event.key === 'Escape' && mapView?.pop) closePlace(mapView); });

// The timeline's own chips, on the map: All · Picks · ★ 3+, and the year.
function mapChips(params) {
  const flag = params.get('flag'), rating = params.get('rating'), date = params.get('date');
  const to = change => go('/places', { ...Object.fromEntries(narrowedBy(params).filter(([key]) => key !== 'place' && key !== 'region')), ...change });
  const preset = (label, on, change, spoken) => h('button', { class: 'chip preset' + (on ? ' chosen' : ''), 'aria-pressed': String(on), 'aria-label': spoken, onclick: () => to(change) }, label);
  const year = h('button', { class: 'chip' + (date ? ' chosen' : ''), 'aria-haspopup': 'menu', onclick: async event => {
    event.stopPropagation();
    const at = event.currentTarget.getBoundingClientRect();
    const data = await api('filters?' + new URLSearchParams(narrowedBy(params)), { quiet: true }).catch(() => null);
    const item = (label, value, n) => h('button', { class: (date || null) === value ? 'chosen' : null, onclick: () => to({ date: value }) }, label, n != null && h('span', { class: 'caption' }, grouped(n)));
    menuAt(at.left, at.bottom + 6, [item(S.allYears, null), ...(data?.years || []).map(([value, n]) => item(value, value, n))]);
    openMenu?.box.classList.add('filter-menu');
  } }, date ? dateName(date) : S.allYears);
  return h('div', { class: 'filters one map-chips' },
    preset(S.all, !flag && !rating, { flag: null, rating: null }),
    preset(S.picks, flag === 'pick', { flag: flag === 'pick' ? null : 'pick' }),
    preset(rated(3), rating === '3', { rating: rating === '3' ? null : 3 }, ratedSaid(3)),
    h('span', { class: 'divider' }), year);
}

// A photograph's place in its info: a small map with where it was taken, one
// map kept and moved from photograph to photograph; a click opens Places there.
let miniMap = null;
function infoMap([lat, lon]) {
  if (!miniMap) {
    const box = h('div', { class: 'mini-map' });
    miniMap = { box, link: h('button', { class: 'mini-map-link', 'aria-label': S.showOnMap, title: S.showOnMap, onclick: () => go('/places', { at: miniMap.at.join(',') }) }, box) };
    loadMapLibre().then(lib => {
      const [y, x] = miniMap.at;
      miniMap.map = new lib.Map({ container: box, style: mapStyle(), interactive: false, attributionControl: false, center: [x, y], zoom: 13 });
      miniMap.dot = new lib.Marker({ element: h('i', { class: 'map-here' }) }).setLngLat([x, y]).addTo(miniMap.map);
    }, () => miniMap.link.remove());
  }
  miniMap.at = [lat, lon];
  if (miniMap.map) {
    miniMap.map.jumpTo({ center: [lon, lat] });
    miniMap.dot.setLngLat([lon, lat]);
    requestAnimationFrame(() => miniMap.map.resize());
  }
  return h('div', { class: 'mini-map-box' }, miniMap.link, h('div', { class: 'caption' }, S.osmCredit));
}
