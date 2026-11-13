'use strict';

const MAPLIBRE = '/map/6.13.0/';
const MAP_TILES = [`${location.origin}/api/v1/map/{z}/{x}/{y}`];
let mapLibrary = null;
function loadMapLibre() {
  if (!mapLibrary) {
    document.head.append(h('link', { rel: 'stylesheet', href: MAPLIBRE + 'maplibre-gl.css' }));
    mapLibrary = import(MAPLIBRE + 'maplibre-gl.mjs');
    mapLibrary.catch(() => { mapLibrary = null; });
  }
  return mapLibrary;
}

function mapStyle(tiles = MAP_TILES) {
  const css = getComputedStyle(document.documentElement);
  const token = name => css.getPropertyValue(`--map-${name}`).trim();
  const kind = (...kinds) => ['in', ['get', 'kind'], ['literal', kinds]];
  const label = (colour, size, more) => ({
    layout: { 'text-field': ['coalesce', ['get', 'name:en'], ['get', 'name']], 'text-font': ['Noto Sans Regular'], 'text-size': size, 'text-max-width': 8, ...more },
    paint: { 'text-color': colour, 'text-halo-color': token('land'), 'text-halo-width': 1.2 },
  });

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
  return {
    version: 8, glyphs: `${location.origin}/map/fonts/{fontstack}/{range}.pbf`, layers,
    sources: { world: { type: 'vector', tiles, maxzoom: 6 }, here: { type: 'vector', tiles, minzoom: 7, maxzoom: 14 } },
  };
}
