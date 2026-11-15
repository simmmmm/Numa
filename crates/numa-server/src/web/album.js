'use strict';

const gridAlbums = new Set();

let albumNow = null;

let albumMap = null;

const chapterNames = new Map();
const chapterSaid = value => value === 'none' ? S.inNoChapter : chapterNames.get(value) || S.chapterOne;

const SPAN_LONG = 62 * 86400;
const dayNumber = seconds => Math.floor(seconds / 86400);

const todayNumber = () => Math.floor((Date.now() - new Date().getTimezoneOffset() * 60000) / 86400000);

async function albumPage(main, params, narrowed) {
  const key = params.get('album');
  if (!key || sortQuery().length || narrowed.some(([name]) => !['album', 'flag', 'rating'].includes(name))) return false;
  const chips = narrowed.filter(([name]) => name !== 'album');
  if (chips.length && gridAlbums.has(key)) return false;
  const query = new URLSearchParams(chips).toString();
  let page;
  try { page = await api(`albums/${encodeURIComponent(key)}/page` + (query ? '?' + query : '')); } catch (_) { return false; }
  if (!main.isConnected) return true;

  const long = page.grid || (page.kind === 'album' && !page.chapters.length && page.count > 0 && page.first != null && page.last - page.first >= SPAN_LONG);
  if (long) {
    if (!chips.length) gridAlbums.add(key);
    return false;
  }
  if (!chips.length) gridAlbums.delete(key);
  const view = { main, params, key, query, keeps: new Set(), page: null, body: null, dates: null, cover: null };
  const filters = await filterRow(params, narrowed, { kind: page.kind, may_change: page.may_change });
  if (!main.isConnected) return true;

  filters.head.classList.add('album-bar');
  view.dates = filters.dates;
  view.body = h('div', { class: 'album-page' });

  view.cover = h('section', { class: 'album-cover' });
  main.append(filters.head, view.cover, view.body);
  albumNow = view;
  paint(view, page);

  const named = () => {
    if (!main.isConnected) return removeEventListener('scroll', named);
    const title = view.cover.querySelector('.title');
    filters.head.classList.toggle('named', !!title && title.getBoundingClientRect().bottom <= filters.head.getBoundingClientRect().bottom);
  };
  addEventListener('scroll', named, { passive: true });
  return true;
}

const part = (date, options) => date.toLocaleString(S.locale, { ...options, timeZone: 'UTC' });
const dateOf = day => new Date(day * 86400000);
const clock = seconds => part(new Date(seconds * 1000), { hour: '2-digit', minute: '2-digit', hour12: false });

function daysSpan(from, to, year) {
  const a = dateOf(from), b = dateOf(to);
  const d = date => date.getUTCDate(), m = date => part(date, { month: 'short' }), y = date => date.getUTCFullYear();
  if (year && y(a) !== y(b)) return `${d(a)} ${m(a)} ${y(a)} – ${d(b)} ${m(b)} ${y(b)}`;
  const text = from === to ? `${d(a)} ${m(a)}` : m(a) === m(b) && y(a) === y(b) ? `${d(a)}–${d(b)} ${m(b)}` : `${d(a)} ${m(a)} – ${d(b)} ${m(b)}`;
  return year ? `${text} ${y(b)}` : text;
}

const dayName = (day, year) => {
  const date = dateOf(day);
  return `${part(date, { weekday: 'short' })} ${date.getUTCDate()} ${part(date, { month: 'short' })}${year ? ' ' + date.getUTCFullYear() : ''}`;
};

const timeSpan = (first, last) => dayNumber(first) === dayNumber(last) ? `${clock(first)}–${clock(last)}`
  : `${daysSpan(dayNumber(first), dayNumber(first))} ${clock(first)} – ${daysSpan(dayNumber(last), dayNumber(last))} ${clock(last)}`;
const nightsSaid = n => phrase(n === 1 ? 'oneNight' : 'nights', { n });

const hereNow = chapter => {
  const today = todayNumber();
  return !!(chapter.place && chapter.days && today >= chapter.days[0] && today <= chapter.days[1]);
};

const isAhead = (page, chapter) => page.kind === 'trip' && !chapter.count && !!chapter.place;

function chapterLine(chapter, ahead) {
  if (chapter.days) {
    const [from, to] = chapter.days;
    return [daysSpan(from, to), to > from && nightsSaid(to - from), chapter.count > 0 && counted(chapter.count), hereNow(chapter) ? S.hereNow : ahead && S.aheadHead].filter(Boolean).join(' · ');
  }
  if (chapter.count && chapter.first != null) return `${timeSpan(chapter.first, chapter.last)} · ${counted(chapter.count)}`;
  return ahead ? S.aheadHead : S.noPhotosYetLine;
}

function routeLength(chapters) {
  const places = chapters.filter(chapter => chapter.place).map(chapter => chapter.place.position);
  const rad = degrees => degrees * Math.PI / 180;
  let total = 0;
  for (let at = 1; at < places.length; at++) {
    const [lat1, lon1] = places[at - 1], [lat2, lon2] = places[at];
    const a = Math.sin(rad(lat2 - lat1) / 2) ** 2 + Math.cos(rad(lat1)) * Math.cos(rad(lat2)) * Math.sin(rad(lon2 - lon1) / 2) ** 2;
    total += 6371 * 2 * Math.asin(Math.min(1, Math.sqrt(a)));
  }
  return Math.round(total);
}

function daysOf(page) {
  const planned = page.chapters.filter(chapter => chapter.days).flatMap(chapter => chapter.days);
  const all = [...planned, ...(page.first != null ? [dayNumber(page.first), dayNumber(page.last)] : [])];
  return all.length ? [Math.min(...all), Math.max(...all)] : null;
}

function overName(page) {
  const span = daysOf(page);
  const kind = { trip: S.kindTrip, wedding: S.kindWedding }[page.kind] || S.kindAlbum;
  return span ? `${kind} · ${daysSpan(span[0], span[1], true)}` : kind;
}

function figuresOf(page) {
  const trip = page.kind === 'trip';
  const { chapters } = page;
  const span = daysOf(page);
  const filled = chapters.filter(chapter => chapter.count).length;
  const all = [];
  if (span) {
    const today = todayNumber(), long = span[1] - span[0] + 1;
    if (trip && today >= span[0] && today <= span[1]) all.push([S.figDay, grouped(today - span[0] + 1), phrase('ofN', { n: grouped(long) })]);
    else if (long > 1) all.push([S.figDays, grouped(long)]);
  }
  if (trip) {
    if (chapters.length) all.push([S.figPlaces, grouped(filled), phrase('ofN', { n: grouped(chapters.length) })]);

    const km = routeLength(chapters.filter(chapter => chapter.count));
    if (km) all.push([S.figTravelled, grouped(km), S.kmUnit]);
  } else if (chapters.length) all.push([S.figChapters, grouped(filled), phrase('ofN', { n: grouped(chapters.length) })]);
  if (page.count) all.push([S.figPhotos, grouped(page.count)]);
  return all;
}

function coverOf(page) {
  const shown = [...page.chapters.flatMap(chapter => chapter.shown || []), ...page.days.flatMap(day => day.shown || [])];
  return shown.find(photo => photo.flag === 'pick') || shown[0] || null;
}

function fillCover(view, page) {
  const cover = coverOf(page);
  const trip = page.kind === 'trip';
  const figures = figuresOf(page);
  view.cover.className = 'album-cover' + (cover ? '' : ' bare');
  view.cover.replaceChildren(...[
    cover && h('img', { src: thumb(cover.id, 2048), alt: '', decoding: 'async' }),
    cover && h('div', { class: 'scrim' }),

    trip && page.may_change && h('div', { class: 'cover-act' },
      h('button', { class: 'primary', onclick: () => tripLinkDialog(view.key) }, S.tripShareEll)),
    h('div', { class: 'say' },
      h('div', { class: 'section-label' }, overName(page)),
      h('div', { class: 'title', 'aria-hidden': 'true' }, page.name || view.main.querySelector('.album-bar h1')?.textContent || ''),
      figures.length > 0 && h('dl', { class: 'album-figures' }, figures.map(([label, value, of]) => h('div', {}, h('dt', { class: 'section-label' }, label), h('dd', {}, String(value), of && h('span', {}, ' ' + of))))),
      trip && page.chapters.length > 1 && h('div', { class: 'album-parts', role: 'img', 'aria-label': phrase('placesOfTotal', { n: page.chapters.filter(chapter => chapter.count).length, m: page.chapters.length }) },
        page.chapters.map(chapter => h('i', { class: chapter.count ? 'been' : '' })))),
  ].filter(Boolean));
}

const albumLink = (view, change) => '#/?' + new URLSearchParams(Object.entries({
  album: view.key, flag: view.params.get('flag'), rating: view.params.get('rating'), ...change,
}).filter(([, value]) => value != null && value !== '')).toString();

function paint(view, page) {
  view.page = page;
  view.keeps = new Set();
  page.chapters.forEach(chapter => chapterNames.set(chapter.key, chapter.name));
  const trip = page.kind === 'trip';
  const sections = [];
  const column = h('div', { class: 'album-column' });
  const nothing = !page.count && !page.chapters.length;
  const starting = nothing && !page.places?.length && !view.query && page.may_change;

  const words = !(nothing && !page.places?.length) && textField(view, {
    spans: page.spans, placeholder: trip ? S.writeTrip : S.writeAbout, label: S.albumText,
    keep: text => api(`albums/${encodeURIComponent(view.key)}/text`, { method: 'PUT', body: JSON.stringify({ text }) }),
  });
  if (words) {
    words.classList.add('album-lead');
    column.append(words);
  }
  const offered = trip && page.places?.length ? placesOffered(view, page) : null;
  if (offered) column.append(offered);

  if (page.chapters.length) {
    const line = h('div', { class: 'album-line' });
    page.chapters.forEach((chapter, at) => {
      line.append(chapterBlock(view, chapter, sections, at + 1));
      const next = page.chapters[at + 1];
      if (next) line.append(h('div', { class: 'album-leg' + (isAhead(page, next) ? ' ahead' : '') }, trip && chapter.place && next.place ? phrase('km', { n: grouped(routeLength([chapter, next])) }) : ''));
    });
    column.append(line);
  }

  const loose = page.days.map(day => dayBlock(view, day, sections, null)).filter(Boolean);
  const adding = !page.chapters.length && !nothing && page.may_change && !view.query;
  if (loose.length || adding) column.append(h('div', { class: 'album-days' }, loose, adding && addDayRow(view, null)));
  if (nothing && !page.places?.length) column.append(starting ? startBlock(view, page) : h('p', { class: 'body' }, view.query ? S.noFilterMatch : S.albumNothingYet));
  else if (view.query && !page.chapters.length && !page.days.length) column.append(empty(S.nothingHere, S.noFilterMatch));

  if (!trip && page.may_change && !starting) column.append(addField(view, page));

  const side = [];
  if (trip) {
    const map = tripMap(view, page);
    if (map) side.push(map);
    if (page.chapters.length || page.may_change) side.push(routeBlock(view, page));
  } else if (page.chapters.length) side.push(chapterList(view, page));
  const sided = side.length > 0;
  view.body.className = 'album-page' + (sided ? ' sided' : '') + (nothing && !page.places?.length ? ' starting' : '');
  view.body.replaceChildren(...[column, sided && h('aside', { class: 'album-side' + (trip ? '' : ' list'), 'aria-label': trip ? S.tripMap : S.chaptersHead }, side)].filter(Boolean));
  view.dates.textContent = '';
  fillCover(view, page);
  state.sections = sections;
  sections.forEach(section => section.arrange());
  const box = view.body.querySelector('.album-canvas');
  if (box) requestAnimationFrame(() => drawMap(view, box, page));
}

async function refill(view) {
  await Promise.allSettled([...view.keeps].map(keep => keep()));
  let page;
  try { page = await api(`albums/${encodeURIComponent(view.key)}/page` + (view.query ? '?' + view.query : ''), { quiet: true }); } catch (error) { toast(error.message); return; }
  if (!view.body.isConnected || albumNow !== view) return;
  const kept = scrollY;
  view.body.style.minHeight = view.body.offsetHeight + 'px';
  paint(view, page);
  view.body.style.minHeight = '';
  scrollTo(0, kept);
}

function groupOf(sections, photos, rest) {
  const box = h('div', { class: 'album-photos' });
  const section = { photos, rest: rest?.n > 0 ? rest : null, rows: box, count: photos.length, load: async () => photos, arrange: () => arrange(section) };
  sections.push(section);
  return box;
}

function arrange(section) {
  const width = section.rows.clientWidth;
  if (!width) return;
  const parts = [];
  let run = [];
  const flush = () => { if (run.length) parts.push(rows(section, run, width)); run = []; };
  for (const photo of section.photos) {
    if (photo.caption) { flush(); parts.push(figure(section, photo, width)); } else run.push(photo);
  }
  if (section.rest) run.push({ rest: section.rest, aspect: 1 });
  flush();
  section.rows.replaceChildren(...parts);
}

const TILE_HIGH = 148;
function rows(section, photos, width) {
  const across = width < 480 ? 3 : 4;
  const wide = (width - GAP * (across - 1)) / across;
  const tiles = photos.map((photo, at) => {
    const style = { left: (at % across) * (wide + GAP) + 'px', top: Math.floor(at / across) * (TILE_HIGH + GAP) + 'px', width: wide + 'px', height: TILE_HIGH + 'px' };
    return photo.rest ? restTile(photo.rest, style) : tile(section, photo, style, TILE_HIGH, wide);
  });
  const lines = Math.ceil(photos.length / across);
  return h('div', { class: 'rows', style: { height: lines * TILE_HIGH + (lines - 1) * GAP + 'px' } }, tiles);
}

function figure(section, photo, width) {
  const height = Math.round(width / Math.min(16 / 9, Math.max(5 / 4, aspect(photo))));
  const frame = h('div', { class: 'rows', style: { height: height + 'px' } },
    tile(section, photo, { left: '0px', top: '0px', width: width + 'px', height: height + 'px' }, height, width));
  return h('figure', { class: 'album-figure' }, frame, h('figcaption', { class: 'album-caption' }, photo.caption));
}

const markOf = (page, chapter, number) => h('span', { class: 'album-mark' + (isAhead(page, chapter) ? ' ahead' : '') + (page.kind === 'trip' && hereNow(chapter) ? ' here' : ''), 'aria-hidden': 'true' }, String(number));

function stripOf(sections, photos) {
  const rows = h('div', { class: 'album-strip-row' });
  const slide = (way, said, symbol) => h('button', { type: 'button', class: 'album-strip-go ' + way, 'aria-label': said, title: said, tabindex: '-1',
    onclick: () => rows.scrollBy({ left: (way === 'back' ? -0.85 : 0.85) * rows.clientWidth, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' }) }, icon(symbol));
  const back = slide('back', S.stripBack, 'back'), on = slide('on', S.stripOn, 'on');
  const ends = () => {
    back.hidden = rows.scrollLeft <= 1;
    on.hidden = rows.scrollLeft + rows.clientWidth >= rows.scrollWidth - 1;
  };
  rows.addEventListener('scroll', ends, { passive: true });
  const section = { photos, rows, count: photos.length, load: async () => photos, arrange: () => {
    if (!rows.childElementCount) rows.replaceChildren(...photos.map(photo => tile(section, photo, { width: aspect(photo) * TILE_HIGH + 'px', height: TILE_HIGH + 'px' }, TILE_HIGH, aspect(photo) * TILE_HIGH)));
    ends();
  } };
  sections.push(section);
  return h('div', { class: 'album-photos album-strip' }, rows, back, on);
}

const restTile = (rest, style) => h('a', { class: 'album-rest', href: rest.href, style }, h('b', {}, '+' + grouped(rest.n)), h('span', {}, S.showAllPlain));

function chapterBlock(view, chapter, sections, number) {
  const page = view.page;
  const ahead = isAhead(page, chapter);
  const bare = !chapter.count;
  const may = page.may_change;
  const shown = chapter.shown || [];

  const days = chapter.by_day;
  const photos = !days && shown.length ? groupOf(sections, shown, { n: chapter.count - shown.length, href: albumLink(view, { chapter: chapter.key }) }) : null;
  return h('section', { class: 'album-chapter' + (bare ? ' bare' : '') + (ahead ? ' ahead' : ''), 'data-chapter': chapter.key },
    markOf(page, chapter, number),
    h('div', { class: 'section-label' }, chapterLine(chapter, ahead)),
    h('div', { class: 'album-title' }, h('h2', {}, chapter.name), may && chapterMenu(view, chapter)),
    textField(view, {
      spans: chapter.spans, label: S.chapterWords,
      placeholder: chapter.place ? S.writePlace : chapter.count ? S.writeAbout : S.writeWhat,
      keep: words => api(`chapters/${chapter.key}`, { method: 'PUT', body: JSON.stringify({ words }) }),
    }),
    photos,
    days && days.map(day => dayBlock(view, day, sections, chapter)),
    days && may && addDayRow(view, chapter),
    (chapter.offers || []).map(offer => offerRow(view, chapter, offer)));
}

const dayHead = day => {
  const date = new Date(day + 'T00:00:00Z');
  return `${part(date, { weekday: 'long' })} ${date.getUTCDate()} ${part(date, { month: 'long' })}`;
};

function dayBlock(view, day, sections, chapter) {
  const may = view.page.may_change;
  if (!may && day.written && !day.spans?.length) return null;
  const where = `albums/${encodeURIComponent(view.key)}/days/${day.number}`;
  const show = chapter ? { chapter: chapter.key, date: day.day } : { date: day.day };
  return h('section', { class: 'album-day', 'data-day': day.number },
    h('div', { class: 'album-day-head' }, h('h3', {}, dayHead(day.day)), h('span', { class: 'caption' }, day.count ? counted(day.count) : S.noPhotosDay)),
    textField(view, {
      spans: day.spans, placeholder: phrase('writeAboutDay', { day: part(new Date(day.day + 'T00:00:00Z'), { weekday: 'long' }) }), label: S.dayReport,
      keep: text => api(where, { method: 'PUT', body: JSON.stringify({ chapter: chapter?.key ?? null, text }) }),
      left: async text => {
        if (!day.written || text) return;
        try { await api(where + (chapter ? '?chapter=' + encodeURIComponent(chapter.key) : ''), { method: 'DELETE' }); await refill(view); } catch (error) { toast(error.message); }
      },
    }),
    day.shown?.length > 0 && (day.strip ? stripOf(sections, day.shown) : groupOf(sections, day.shown, { n: day.rest, href: albumLink(view, show) })));
}

function addDayRow(view, chapter) {
  const iso = number => new Date(number * 86400000).toISOString().slice(0, 10);
  return h('button', { class: 'album-add-day', type: 'button', onclick: async () => {
    const page = view.page;
    const span = page.kind === 'trip' ? daysOf(page) : null;
    const shown = chapter ? page.chapters.find(one => one.key === chapter.key)?.by_day || [] : page.days;
    const last = shown.length ? Math.max(...shown.map(day => day.number)) : null;
    const keep = span || chapter?.days;
    let first = last != null ? last + 1 : chapter?.days?.[0] ?? todayNumber();
    if (keep) first = Math.min(keep[1], Math.max(keep[0], first));
    const field = h('input', { type: 'date', name: 'day', required: true, 'aria-label': S.dayDate,
      min: span && iso(span[0]), max: span && iso(span[1]), value: iso(first) });
    const asked = await dialog(S.addDay, null, [field], S.add);
    const number = asked?.day && Math.floor(Date.parse(asked.day + 'T00:00:00Z') / 86400000);
    if (!Number.isFinite(number)) return;
    try {
      await api(`albums/${encodeURIComponent(view.key)}/days`, { method: 'POST', body: JSON.stringify({ day: number, chapter: chapter?.key ?? null }) });
      await refill(view);
      const scope = chapter ? view.body.querySelector(`.album-chapter[data-chapter="${chapter.key}"]`) : view.body;
      scope?.querySelector(`.album-day[data-day="${number}"] .text-field`)?.open();
    } catch (error) { toast(error.message); }
  } }, icon('plus'), S.addDayEll);
}

const TEXT_LIMIT = 10000;

const textNodes = blocks => (blocks || []).map(block => {
  const run = one => {
    let node = document.createTextNode(one.text);
    if (one.italic) node = h('i', {}, node);
    if (one.bold) node = h('b', {}, node);
    return node;
  };
  return block.list ? h('ul', {}, block.lines.map(line => h('li', {}, line.map(run))))
    : h('p', {}, block.lines.map((line, at) => [at > 0 && h('br'), line.map(run)]));
});

function blocksOf(root) {
  const blocks = [];
  let block = null, runs = [];
  const line = () => { if (runs.length) (block ??= { list: false, lines: [] }).lines.push(runs); runs = []; };
  const finish = () => { line(); if (block?.lines.length) blocks.push(block); block = null; };
  const add = (text, look, list) => {
    const bold = Number(look.fontWeight) >= 600, italic = look.fontStyle === 'italic';
    block ??= { list, lines: [] };
    const last = runs.at(-1);
    if (last && last.bold === bold && last.italic === italic) last.text += text; else runs.push({ text, bold, italic });
  };
  const walk = (node, list) => {
    for (const child of node.childNodes) {
      if (child.nodeType === Node.TEXT_NODE) {
        const look = getComputedStyle(child.parentElement);
        child.data.replace(/\u00a0/g, ' ').replace(/\u200b/g, '').split('\n').forEach((text, at) => { if (at) line(); if (text) add(text, look, list); });
      } else if (child.nodeType !== Node.ELEMENT_NODE) continue;
      else if (child.tagName === 'BR') line();
      else if (child.tagName === 'UL' || child.tagName === 'OL') { finish(); block = { list: true, lines: [] }; walk(child, true); finish(); }
      else if (list && /^(LI|P|DIV)$/.test(child.tagName)) { line(); walk(child, true); line(); }
      else if (/^(P|DIV|LI)$/.test(child.tagName)) { finish(); walk(child, false); finish(); }
      else walk(child, list);
    }
  };
  walk(root, false);
  finish();
  return blocks;
}

const escapedWords = words => {
  const all = [...words];
  return all.map((c, at) => c === '*' ? '\\*' : c === '\\' && [undefined, '*', '\\', '-'].includes(all[at + 1]) ? '\\\\' : c).join('');
};
function writeLine(line) {
  const close = (bold, italic) => (italic ? '*' : '') + (bold ? '**' : '');
  let out = '', bold = false, italic = false, gap = '';
  for (const one of line) {
    const core = one.text.trim();
    if (!core) { gap += one.text; continue; }
    const lead = one.text.slice(0, one.text.length - one.text.trimStart().length);
    gap += lead;
    const reset = (bold && !one.bold) || (one.bold && !bold && italic);
    if (reset) { out += close(bold, italic); bold = italic = false; } else if (italic && !one.italic) { out += '*'; italic = false; }

    const reopen = !(reset && !gap);
    out += gap;
    gap = '';
    if (reopen && one.bold && !bold) { out += '**'; bold = true; }
    if (reopen && one.italic && !italic) { out += '*'; italic = true; }
    out += escapedWords(core);
    gap += one.text.slice(lead.length + core.length);
  }
  return (out + close(bold, italic)).trim();
}
const writeBlocks = blocks => blocks.map(block => block.lines.map(writeLine).filter(Boolean)
  .map(line => block.list ? '- ' + line : line === '-' || line.startsWith('- ') ? '\\' + line : line).join('\n')).filter(Boolean).join('\n\n');

function richBlocks(html) {
  if (!html) return null;
  const root = new DOMParser().parseFromString(html, 'text/html').body;

  const styled = (node, name) => (node.getAttribute('style') || '').match(new RegExp(`(?:^|;)\\s*${name}\\s*:\\s*([^;!]+)`, 'i'))?.[1].trim().toLowerCase();
  const weight = node => {
    const said = styled(node, 'font-weight');
    if (said) return said === 'bold' || said === 'bolder' || Number(said) >= 600;
    return /^(B|STRONG)$/.test(node.tagName) ? true : null;
  };
  const slant = node => {
    const said = styled(node, 'font-style');
    if (said) return said === 'italic' || said === 'oblique';
    return /^(I|EM)$/.test(node.tagName) ? true : null;
  };
  const blocks = [];
  let block = null, runs = [], marked = false;
  const line = () => { if (runs.some(run => run.text.trim())) (block ??= { list: false, lines: [] }).lines.push(runs); runs = []; };
  const finish = () => { line(); if (block?.lines.length) blocks.push(block); block = null; };
  const walk = (node, bold, italic, list) => {
    for (const child of node.childNodes) {
      if (child.nodeType === Node.TEXT_NODE) {
        const words = child.data.replace(/[\s\u00a0]+/g, ' ');
        if (!words.trim() && !runs.length) continue;
        block ??= { list, lines: [] };
        marked ||= bold || italic;
        const last = runs.at(-1);
        if (last && last.bold === bold && last.italic === italic) last.text += words; else runs.push({ text: words, bold, italic });
        continue;
      }
      if (child.nodeType !== Node.ELEMENT_NODE || /^(SCRIPT|STYLE|HEAD|TITLE|TEMPLATE)$/.test(child.tagName)) continue;
      const b = weight(child) ?? bold, i = slant(child) ?? italic;
      if (child.tagName === 'BR') line();
      else if (child.tagName === 'UL' || child.tagName === 'OL') { finish(); block = { list: true, lines: [] }; walk(child, b, i, true); finish(); }
      else if (list && child.tagName === 'LI') { line(); walk(child, b, i, true); line(); }
      else if (/^(P|DIV|LI|H[1-6]|BLOCKQUOTE|TR)$/.test(child.tagName)) { finish(); walk(child, b, i, false); finish(); }
      else walk(child, b, i, list);
    }
  };
  walk(root, false, false, false);
  finish();
  if (!marked) return null;
  blocks.forEach(one => one.lines.forEach(runs => { runs[0].text = runs[0].text.trimStart(); runs.at(-1).text = runs.at(-1).text.trimEnd(); }));
  return blocks;
}

function pastedHtml(blocks, text) {
  const box = document.createElement('div');
  const nodes = textNodes(blocks);
  if (nodes.length === 1 && nodes[0].tagName === 'P') box.append(text.match(/^[ \t]*/)[0], ...nodes[0].childNodes, text.match(/[ \t]*$/)[0]);
  else box.append(...nodes);
  return box.innerHTML;
}

function textField(view, { spans, placeholder, label, keep, left }) {
  if (!view.page.may_change) return spans?.length ? h('div', { class: 'album-text' }, textNodes(spans)) : null;
  const area = h('div', { class: 'text-area', contenteditable: 'true', role: 'textbox', 'aria-multiline': 'true', 'aria-label': label, 'data-placeholder': placeholder }, textNodes(spans));
  const command = (name, said, symbol) => h('button', { type: 'button', tabindex: '-1', 'data-command': name, 'aria-label': said, title: said, 'aria-pressed': 'false',
    onmousedown: event => event.preventDefault(), onclick: () => { document.execCommand(name); changed(); } }, icon(symbol));
  const status = h('span', { class: 'text-kept', 'aria-live': 'polite' });
  const prompt = h('button', { type: 'button', class: 'text-prompt', onclick: () => open() }, icon('pencil'), h('span', {}, placeholder));
  const done = h('button', { type: 'button', class: 'text-done', onmousedown: event => event.preventDefault(), onclick: () => shut() }, icon('check'), S.done);
  const commands = [command('bold', S.textBold, 'bold'), command('italic', S.textItalic, 'italic'), command('insertUnorderedList', S.textList, 'list')];
  const bar = h('div', { class: 'text-bar', role: 'toolbar', 'aria-label': S.textFormat }, commands, status, done);
  const editor = h('div', { class: 'text-edit' }, area, h('span', { class: 'text-pencil', 'aria-hidden': 'true' }, icon('pencil')), bar);
  const box = h('div', { class: 'text-field' }, prompt, editor);
  const mark = () => { for (const button of commands) button.setAttribute('aria-pressed', String(document.queryCommandState(button.dataset.command))); };
  const blank = () => box.classList.toggle('blank', !area.textContent.trim());
  const now = () => writeBlocks(blocksOf(area));

  let sent = null, latest = null, timer = 0;
  const flush = async () => {
    clearTimeout(timer);
    if (sent === null) return;
    const text = area.isConnected ? now() : latest ?? sent;
    if (text === sent) return;
    const before = sent;
    sent = text;
    try { await keep(text); if (box.classList.contains('on')) status.textContent = S.textKept; } catch (error) { sent = before; toast(phrase('wordsNotKept', { error: error.message })); }
  };
  const changed = () => { latest = now(); status.textContent = ''; blank(); mark(); clearTimeout(timer); timer = setTimeout(flush, 900); };
  view.keeps.add(flush);
  const open = () => { box.classList.add('on'); area.focus(); };

  const shut = () => { area.blur(); document.activeElement?.blur(); if (box.classList.contains('blank')) prompt.focus(); };
  area.addEventListener('focus', () => {
    sent ??= now();
    box.classList.add('on');
    mark();
    document.execCommand('defaultParagraphSeparator', false, 'p');

    const away = () => flush();
    addEventListener('hashchange', away, { once: true });
    addEventListener('pagehide', away, { once: true });
  });
  editor.addEventListener('focusout', event => {
    if (editor.contains(event.relatedTarget)) return;
    box.classList.remove('on');
    status.textContent = '';
    flush().then(() => left?.(sent ?? now()));
  });
  area.addEventListener('keydown', event => { if (event.key === 'Escape') shut(); });
  document.addEventListener('visibilitychange', function hidden() {
    if (!area.isConnected) return document.removeEventListener('visibilitychange', hidden);
    if (document.visibilityState === 'hidden') flush();
  });
  document.addEventListener('selectionchange', function moved() {
    if (!area.isConnected) return document.removeEventListener('selectionchange', moved);
    if (box.classList.contains('on')) mark();
  });
  typing(area, now, changed);
  blank();
  box.open = open;
  return box;
}

function typing(area, now, changed) {

  const full = extra => charCount(area.textContent) + extra > TEXT_LIMIT - 200 && charCount(now()) + extra > TEXT_LIMIT;
  area.addEventListener('beforeinput', event => {
    if (event.inputType.startsWith('insert') && event.inputType !== 'insertFromPaste' && full(charCount(event.data || ' '))) event.preventDefault();

    if (event.inputType.startsWith('format') && !['formatBold', 'formatItalic'].includes(event.inputType)) event.preventDefault();
  });

  area.addEventListener('paste', async event => {
    event.preventDefault();
    const text = (event.clipboardData?.getData('text/plain') || '').replace(/\r\n?/g, '\n');
    const rich = richBlocks(event.clipboardData?.getData('text/html') || '');
    const room = TEXT_LIMIT - charCount(now());
    if (room <= 0 || (!text && !rich)) return;
    const at = getSelection().rangeCount ? getSelection().getRangeAt(0).cloneRange() : null;
    let blocks = rich;
    if (!blocks) try { blocks = (await api('texts/clean', { method: 'POST', body: JSON.stringify({ text }), quiet: true })).spans; } catch (_) { blocks = null; }
    if (!area.isConnected) return;
    if (at) { getSelection().removeAllRanges(); getSelection().addRange(at); }
    if (blocks?.length && charCount(writeBlocks(blocks)) <= room) document.execCommand('insertHTML', false, pastedHtml(blocks, text));
    else if (text) document.execCommand('insertText', false, [...text].slice(0, room).join(''));
  });
  area.addEventListener('drop', event => event.preventDefault());
  area.addEventListener('input', event => {

    const at = getSelection(), node = at.anchorNode;
    if (event.inputType === 'insertText' && event.data === ' ' && node?.nodeType === Node.TEXT_NODE && at.anchorOffset === 2 && /^-[ \u00a0]/.test(node.data)
      && !node.parentElement.closest('li') && !node.previousSibling && area.contains(node)) {
      const range = document.createRange();
      range.setStart(node, 0);
      range.setEnd(node, 2);
      at.removeAllRanges();
      at.addRange(range);
      document.execCommand('delete');
      document.execCommand('insertUnorderedList');
    }
    changed();
  });
}

function chapterMenu(view, chapter) {
  return h('button', { class: 'album-more', 'aria-label': S.more, title: S.more, 'aria-haspopup': 'menu', onclick: event => {
    event.stopPropagation();
    const at = event.currentTarget.getBoundingClientRect();
    menuAt(at.left, at.bottom + 6, [
      h('button', { onclick: async () => {
        const asked = await dialog(S.renameChapter, null, [h('input', { type: 'text', name: 'name', value: chapter.name, required: true, maxlength: 80, autocomplete: 'off' })], S.rename);
        const name = (asked?.name || '').trim();
        if (!name || name === chapter.name) return;
        try { await api(`chapters/${chapter.key}`, { method: 'PUT', body: JSON.stringify({ name }) }); await refill(view); } catch (error) { toast(error.message); }
      } }, S.renameEll),
      h('button', { onclick: async () => {
        if (!await ask(phrase('removeChapterQ', { name: chapter.name }), S.removeChapterText, S.removeChapter)) return;
        try { await api(`chapters/${chapter.key}`, { method: 'DELETE' }); await refill(view); toast(S.chapterRemoved); } catch (error) { toast(error.message); }
      } }, S.removeChapterEll),
    ]);
  } }, icon('more'));
}

function lookAt(ids) {
  const section = { photos: ids.map(id => ({ id })), load: async () => section.photos };
  const own = state.sections;
  state.sections = own.concat(section);
  openViewer(section, 0);
  state.sections = own;
}

function pauseWhy(view, chapter, offer) {
  if (offer.after) return phrase('offerAfter', { name: offer.after });
  return view.page.chapters[0]?.key === chapter.key ? S.offerFirst : S.offerNext;
}

function offerRow(view, chapter, offer) {
  const n = grouped(offer.count);
  const line = offer.why === 'place' ? phrase(chapter.count ? 'offerPlaceMore' : 'offerPlace', { n })
    : offer.why === 'days' ? phrase('offerDays', { n })
      : offer.why === 'pause' ? phrase('offerTaken', { n, span: timeSpan(offer.first, offer.last) }) : counted(offer.count);
  const why = offer.why === 'place' ? phrase('offerWhyPlace', { place: chapter.place?.name || chapter.name })
    : offer.why === 'days' ? S.offerWhyDays
      : offer.why === 'pause' ? pauseWhy(view, chapter, offer) : '';
  const ids = offer.ids;
  const some = ids.slice(0, 3);
  const big = !chapter.count;
  const add = h('button', { class: 'chip', onclick: async () => {
    add.disabled = true;
    const put = inside => api(`chapters/${chapter.key}/photos`, { method: 'POST', body: JSON.stringify({ ids, inside }) });
    try {

      const fresh = (await put(true)).new || [];
      await refill(view);
      toast(phrase('addedTo', { n, name: chapter.name }), { label: S.undo, run: async () => {
        try {
          await put(false);
          if (fresh.length) await api('photos/album', { method: 'POST', body: JSON.stringify({ ids: fresh, album: view.key, inside: false }) });
          await refill(view);
        } catch (error) { toast(error.message); }
      } });
    } catch (error) { add.disabled = false; toast(error.message); }
  } }, phrase('addN', { n }));
  return h('div', { class: 'album-offer' + (big ? ' big' : '') },
    h('button', { class: 'album-prints', 'aria-label': phrase('lookAtOffered', { n }), onclick: () => lookAt(ids) },
      some.map(id => h('img', { src: thumb(id, 256), loading: 'lazy', alt: '' }))),
    h('div', { class: 'album-offer-text' }, h('div', { class: 'album-offer-line' }, line), why && h('div', { class: 'caption' }, why)),
    add);
}

function placesOffered(view, page) {
  const { places } = page;
  const add = h('button', { class: 'chip', onclick: async () => {
    add.disabled = true;
    try {
      const { added } = await api(`albums/${encodeURIComponent(view.key)}/places`, { method: 'POST', body: JSON.stringify({}) });
      await refill(view);
      toast(added === 1 ? S.onePlaceAdded : phrase('placesAdded', { n: grouped(added) }));
    } catch (error) { add.disabled = false; toast(error.message); }
  } }, phrase('addN', { n: grouped(places.length) }));
  return h('section', { class: 'album-offered' },
    h('div', { class: 'section-label' }, S.whereTaken),
    places.map(place => h('div', { class: 'album-place' }, h('span', { class: 'dot', 'aria-hidden': 'true' }), h('span', { class: 'name' }, place.name),
      h('span', { class: 'caption' }, [place.days && daysSpan(place.days[0], place.days[1]), counted(place.count)].filter(Boolean).join(' · ')))),
    h('div', { class: 'album-offer' },
      h('div', { class: 'album-offer-text' }, h('div', { class: 'album-offer-line' }, phrase(places.length === 1 ? 'onePlaceOffered' : 'placesOffered', { n: places.length })),
        h('div', { class: 'caption' }, S.placesOfferedText)),
      add));
}

function startBlock(view, page) {
  const album = { key: view.key, kind: page.kind };
  const way = (name, label, line, run) => h('button', { class: 'album-start-row', onclick: run },
    icon(name), h('span', { class: 'album-start-text' }, h('span', { class: 'name' }, label), h('span', { class: 'caption' }, line)), icon('on'));
  return h('div', { class: 'album-start' },
    empty(S.noPhotosYet, phrase('albumEmptyLine', { album: page.name }), false, 'images'),
    page.kind !== 'trip' && h('div', { class: 'section-label' }, S.startHead),
    page.kind === 'album' && way('route', S.startTrip, S.startTripLine, () => setKind(album, 'trip')),
    page.kind === 'album' && way('heart', S.startWedding, S.startWeddingLine, () => setKind(album, 'wedding')),
    page.kind !== 'trip' && way('list', S.startChapters, S.startChaptersLine, () => addChapterDialog(album)));
}

function addField(view, page) {
  const trip = page.kind === 'trip';
  const input = h('input', { type: 'text', placeholder: trip ? S.addPlaceEll : S.addChapterEll, 'aria-label': trip ? S.addPlaceEll : S.addChapterEll,
    maxlength: 80, autocomplete: 'off', autocapitalize: 'words', role: trip ? 'combobox' : null, 'aria-expanded': trip ? 'false' : null });

  const list = h('div', { class: 'menu suggest album-suggest', role: 'listbox', hidden: true, onmousedown: event => event.preventDefault() });
  let found = [], chosen = -1, asked = 0, timer;
  const close = () => { list.hidden = true; list.replaceChildren(); found = []; chosen = -1; input.setAttribute('aria-expanded', 'false'); };
  const add = async body => {
    close();
    try {
      await api(`albums/${encodeURIComponent(view.key)}/chapters`, { method: 'POST', body: JSON.stringify(body) });
      gridAlbums.delete(view.key);
      input.value = '';
      await refill(view);
    } catch (error) { toast(error.message); }
  };
  const take = place => add({ name: place.name, place: { name: place.name, position: place.position } });
  const mark = () => [...list.children].forEach((item, at) => item.setAttribute('aria-selected', String(at === chosen)));
  const suggest = () => {
    const words = input.value.trim();
    clearTimeout(timer);
    if (!trip || words.length < 2) return close();
    timer = setTimeout(async () => {
      const turn = ++asked;
      let places;
      try { places = await api('places/find?q=' + encodeURIComponent(words), { quiet: true }); } catch (_) { return; }
      if (turn !== asked || !input.isConnected) return;
      found = places;
      chosen = -1;
      list.replaceChildren(...places.map(place => h('button', { role: 'option', 'aria-selected': 'false', type: 'button', onclick: () => take(place) },
        place.name, h('span', { class: 'caption' }, [place.region, place.country].filter(one => one && one !== place.name).join(', ')))));
      list.hidden = !places.length;
      input.setAttribute('aria-expanded', String(places.length > 0));
    }, 150);
  };
  input.addEventListener('input', suggest);
  input.addEventListener('keydown', event => {
    if (event.key === 'Escape') { close(); input.blur(); return; }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      if (!found.length) return;
      event.preventDefault();
      chosen = (chosen + 1 + (event.key === 'ArrowDown' ? 1 : -1) + found.length + 1) % (found.length + 1) - 1;
      mark();
      return;
    }
    if (event.key !== 'Enter') return;
    event.preventDefault();
    const name = input.value.trim();
    if (!name) return;
    const same = found.find(place => place.name.toLowerCase() === name.toLowerCase());
    if (found[chosen]) take(found[chosen]); else if (same) take(same); else add({ name });
  });
  input.addEventListener('blur', () => setTimeout(close, 150));
  return h('label', { class: 'album-add' }, icon('plus'), input, list);
}

function routeBlock(view, page) {
  const to = chapter => document.querySelector(`.album-chapter[data-chapter="${chapter.key}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  const said = chapter => [hereNow(chapter) ? S.hereNow : chapter.days && daysSpan(chapter.days[0], chapter.days[1]),
    chapter.count ? grouped(chapter.count) : chapter.days && chapter.days[1] > chapter.days[0] && nightsSaid(chapter.days[1] - chapter.days[0])].filter(Boolean).join(' · ');
  return h('section', { class: 'album-route' },
    page.chapters.length > 0 && h('div', { class: 'section-label' }, S.routeHead),
    page.chapters.map((chapter, at) => h('button', { class: 'album-stop' + (chapter.count ? ' has' : '') + (hereNow(chapter) ? ' here' : ''), onclick: () => to(chapter) },
      markOf(page, chapter, at + 1), h('span', { class: 'name' }, chapter.name), h('span', { class: 'caption' }, said(chapter)))),
    page.may_change && addField(view, page));
}

function chapterList(view, page) {
  const to = chapter => document.querySelector(`.album-chapter[data-chapter="${chapter.key}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  const standing = chapter => chapter.count ? [grouped(chapter.count), true]
    : chapter.offers.length ? [phrase('offered', { n: grouped(Math.max(...chapter.offers.map(offer => offer.count))) }), false] : [S.nothingYet, false];
  return [
    h('div', { class: 'section-label' }, S.chaptersHead),
    page.chapters.map(chapter => {
      const [said, has] = standing(chapter);
      return h('button', { class: 'album-row' + (has ? ' has' : ''), onclick: () => to(chapter) }, h('span', { class: 'name' }, chapter.name), h('span', { class: 'caption' }, said));
    }),
    page.loose > 0 && h('p', { class: 'caption album-loose' }, phrase(page.loose === 1 ? 'oneNoChapterYet' : 'noChapterYet', { n: grouped(page.loose) }), ' ',
      h('a', { href: albumLink(view, { chapter: 'none' }) }, S.showThem)),
  ];
}

const tripMap = (view, page) => {
  const places = [...page.chapters.filter(chapter => chapter.place).map(chapter => chapter.place), ...(page.places || [])];
  return places.length ? h('div', { class: 'album-map' }, h('div', { class: 'album-canvas', role: 'img', 'aria-label': S.tripMap }), h('div', { class: 'caption' }, S.osmCredit)) : null;
};

async function drawMap(view, box, page) {
  let lib;
  try { lib = await loadMapLibre(); } catch (_) { box.parentNode?.remove(); return; }
  if (!box.isConnected || albumNow !== view) return;
  albumMap?.remove();
  albumMap = null;
  try {
    const css = getComputedStyle(document.documentElement);
    const token = name => css.getPropertyValue(`--${name}`).trim();
    const stops = page.chapters.filter(chapter => chapter.place).map(chapter => ({ chapter, at: [chapter.place.position[1], chapter.place.position[0]] }));
    const offered = (page.places || []).map(place => ({ place, at: [place.position[1], place.position[0]] }));
    const points = [...stops.map(stop => stop.at), ...offered.map(one => one.at)];
    if (!points.length) return;
    const west = Math.min(...points.map(at => at[0])), east = Math.max(...points.map(at => at[0]));
    const south = Math.min(...points.map(at => at[1])), north = Math.max(...points.map(at => at[1]));
    const fine = matchMedia('(min-width: 900px) and (pointer: fine)').matches;
    const map = new lib.Map({ container: box, style: mapStyle(), attributionControl: false, dragRotate: false, pitchWithRotate: false, touchPitch: false,
      renderWorldCopies: false, minZoom: 1, maxZoom: 14, interactive: fine, center: [(west + east) / 2, (south + north) / 2], zoom: 4 });
    albumMap = map;

    map.on('error', () => {});
    if (fine) { map.scrollZoom.disable(); map.touchZoomRotate?.disableRotation(); map.keyboard?.disableRotation(); }

    const fit = () => map.fitBounds([[west, south], [east, north]], { padding: 72, maxZoom: 9, duration: 0 });
    fit();
    map.on('resize', fit);

    const lines = [];
    for (let at = 1; at < stops.length; at++) {
      lines.push({ type: 'Feature', properties: { ahead: !(stops[at - 1].chapter.count && stops[at].chapter.count) }, geometry: { type: 'LineString', coordinates: [stops[at - 1].at, stops[at].at] } });
    }
    let added = false;
    const draw = () => {
      if (added || !map.getStyle()) return;
      added = true;
      map.addSource('route', { type: 'geojson', data: { type: 'FeatureCollection', features: lines } });
      const layout = { 'line-cap': 'round', 'line-join': 'round' };
      map.addLayer({ id: 'route-ahead', type: 'line', source: 'route', filter: ['==', ['get', 'ahead'], true], layout, paint: { 'line-color': token('label'), 'line-width': 2.2, 'line-dasharray': [0.1, 2.2] } });
      map.addLayer({ id: 'route-done', type: 'line', source: 'route', filter: ['==', ['get', 'ahead'], false], layout, paint: { 'line-color': token('ink'), 'line-width': 2.4 } });
    };
    if (map.isStyleLoaded()) draw(); else { map.once('style.load', draw); map.once('load', draw); }
    const toChapter = key => document.querySelector(`[data-chapter="${key}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    for (const { chapter, at } of stops) {
      const named = h('span', { class: 'album-pin-name' }, chapter.name);
      const element = chapter.count
        ? h('button', { class: 'pin', 'aria-label': `${chapter.name}, ${counted(chapter.count)}`, onclick: () => toChapter(chapter.key) },
          prints((chapter.shown || []).slice(0, 3).map(photo => photo.id)), h('span', { class: 'count' }, grouped(chapter.count)), markOf(page, chapter, page.chapters.indexOf(chapter) + 1), named)
        : h('button', { class: 'album-dot numbered', 'aria-label': chapter.name, onclick: () => toChapter(chapter.key) }, markOf(page, chapter, page.chapters.indexOf(chapter) + 1), named);
      new lib.Marker({ element }).setLngLat(at).addTo(map);
    }
    for (const { place, at } of offered) {
      new lib.Marker({ element: h('span', { class: 'album-dot', role: 'img', 'aria-label': place.name }, h('i'), h('span', { class: 'album-pin-name' }, place.name)) }).setLngLat(at).addTo(map);
    }
  } catch (_) { box.parentNode?.remove(); }
}

const linkDay = seconds => {
  const date = new Date(seconds * 1000);
  return date.toLocaleDateString(S.locale, { day: 'numeric', month: 'short', ...(date.getFullYear() !== new Date().getFullYear() && { year: 'numeric' }) });
};

async function tripLinkDialog(key) {
  const where = `albums/${encodeURIComponent(key)}/links`;
  const places = h('input', { type: 'checkbox', role: 'switch', class: 'switch', checked: true });
  const follows = h('input', { type: 'checkbox', role: 'switch', class: 'switch', checked: true });
  const days = h('select', { 'aria-label': S.tripEnds },
    [[7, S.tripSevenDays], [30, S.tripThirtyDays], [90, S.tripNinetyDays], [365, S.tripYear]].map(([value, label]) => h('option', { value, selected: value === 30 }, label)));
  const row = (label, text, control) => h('label', { class: 'trip-row' }, h('span', { class: 'trip-what' }, h('span', {}, label), text && h('span', { class: 'caption' }, text)), control);
  const list = h('div', { class: 'trip-links' });
  const paint = links => list.replaceChildren(...(links.length ? [h('div', { class: 'section-label' }, S.tripLinksHead), ...links.map(link => h('div', { class: 'trip-link' },
    h('span', { class: 'caption' }, phrase('tripLinkDates', { made: linkDay(link.made), ends: linkDay(link.expires) })),
    h('button', { class: 'chip small', type: 'button', onclick: async () => {
      if (!await ask(S.tripEndQ, S.tripEndText, S.tripEnd)) return;
      try { await api(`shares/${link.id}/revoke`, { method: 'POST' }); await reload(); toast(S.tripLinkEnded); } catch (error) { toast(error.message); }
    } }, S.tripEnd)))] : []));
  const reload = async () => { try { paint(await api(where, { quiet: true })); } catch (_) {  } };
  const make = h('button', { class: 'primary', type: 'button', onclick: async () => {
    make.disabled = true;
    try {
      const made = await api(where, { method: 'POST', body: JSON.stringify({ route: places.checked, follows: follows.checked, days: Number(days.value) }) });

      try { navigator.clipboard.writeText((made.base || location.origin) + made.link).then(() => toast(S.linkCopied), () => {}); } catch (_) {  }
      showLink(made.link, made.base, S.tripLinkOnce);
      await reload();
    } catch (error) { toast(error.message); }
    make.disabled = false;
  } }, S.copyLink);
  const box = h('dialog', { class: 'ask trip-link-box', 'aria-label': S.tripLinkTitle, onclose: () => box.remove() }, h('form', { method: 'dialog' },
    h('h2', {}, S.tripLinkTitle), h('p', { class: 'body' }, S.tripLinkText),
    row(S.tripRoute, S.tripRouteText, places), row(S.tripFollows, S.tripFollowsText, follows), row(S.tripEnds, null, days),
    h('div', { class: 'buttons' }, h('button', { class: 'chip', type: 'submit' }, S.done), make), list));
  document.body.append(box);
  box.showModal();
  reload();
}

const EDGES = [4096, 2560, 2048, 1600, 1080];
const BOOK_SIZES = [['21 × 28 cm', 280], ['30 × 30 cm', 300], ['21 × 21 cm', 210], ['33 × 28 cm', 330.2]];
const dpi300 = mm => Math.round(mm / 25.4 * 300);

async function exportDialog(key) {
  let page;
  try { page = await api(`albums/${encodeURIComponent(key)}/page`); } catch (error) { toast(error.message); return; }
  const first = page.kind === 'trip' ? 'place' : page.chapters?.length ? 'chapter' : 'day';
  const folders = h('select', { 'aria-label': S.exportFolders },
    [['place', S.byPlaceAndDay], ['chapter', S.byChapterAndDay], ['day', S.byDay], ['one', S.oneFolder]].map(([value, label]) => h('option', { value, selected: value === first }, label)));
  const texts = h('input', { type: 'checkbox', role: 'switch', class: 'switch', checked: true });
  const size = h('select', { 'aria-label': S.exportSize },
    h('option', { value: '' }, S.fullSize), EDGES.map(edge => h('option', { value: 'edge=' + edge }, phrase('longEdgePx', { n: edge }))),
    BOOK_SIZES.map(([name, mm]) => h('option', { value: 'mm=' + mm }, phrase('bookSize', { name }))), h('option', { value: 'custom' }, S.customSize));
  const cm = h('input', { type: 'number', min: 5, max: 100, step: 0.5, value: 28, 'aria-label': S.customCm, inputmode: 'decimal' });
  const edge = h('span', { class: 'caption' });
  const asked = () => size.value === 'custom' ? 'mm=' + Math.round(Number(cm.value) * 10) : size.value;
  const said = () => {
    const value = asked();

    const px = value.startsWith('mm=') ? dpi300(Number(value.slice(3))) : 0;
    edge.textContent = px > 0 ? phrase('pixels', { n: grouped(px) }) : '';
    custom.hidden = size.value !== 'custom';
    long.hidden = !px;
  };
  const row = (label, control) => h('label', { class: 'trip-row' }, h('span', { class: 'trip-what' }, h('span', {}, label)), control);
  const custom = row(S.customCm, cm), long = row(S.longEdge, edge);
  size.addEventListener('change', said);
  cm.addEventListener('input', said);

  const count = page.photographs ?? page.count;
  const go = h('button', { class: 'primary', type: 'submit' }, count === 1 ? S.exportOne : phrase('exportN', { n: grouped(count) }));
  const title = phrase('exportAlbum', { name: page.name });
  const box = h('dialog', { class: 'ask', 'aria-label': title, onclose: () => box.remove() }, h('form', { method: 'dialog', onsubmit: event => {
    event.preventDefault();
    const query = [`folders=${folders.value}`, !texts.checked && 'texts=false', asked()].filter(Boolean).join('&');
    const link = h('a', { href: `/api/v1/albums/${encodeURIComponent(key)}/export?${query}`, download: '', hidden: true });
    document.body.append(link);
    link.click();
    link.remove();
    box.close();
  } },
  h('h2', {}, title),
  h('div', { class: 'trip-links' }, h('div', { class: 'section-label' }, S.exportGroup)),
  row(S.exportFolders, folders), row(S.exportTexts, texts), row(S.exportSize, size), custom, long,
  h('div', { class: 'buttons' }, h('button', { class: 'chip', type: 'button', onclick: () => box.close() }, S.cancel), go)));
  said();
  document.body.append(box);
  box.showModal();
}

async function setKind(album, kind) {
  try {
    await api(`albums/${encodeURIComponent(album.key)}/kind`, { method: 'PUT', body: JSON.stringify({ kind }) });
    gridAlbums.delete(album.key);
    navLists.album = null;
    render();
  } catch (error) { toast(error.message); }
}

async function addChapterDialog(album) {
  const asked = await dialog(S.addChapter, null, [h('input', { type: 'text', name: 'name', placeholder: S.chapterName, required: true, maxlength: 80, autocomplete: 'off', autocapitalize: 'words' })], S.add);
  const name = (asked?.name || '').trim();
  if (!name) return;
  try {
    await api(`albums/${encodeURIComponent(album.key)}/chapters`, { method: 'POST', body: JSON.stringify({ name }) });
    gridAlbums.delete(album.key);
    render();
  } catch (error) { toast(error.message); }
}

function kindItems(album) {
  const kind = album.kind || 'album';
  return [
    kind !== 'trip' && h('button', { onclick: () => addChapterDialog(album) }, S.addChapterEll),
    kind === 'trip' && h('button', { onclick: () => tripLinkDialog(album.key) }, S.tripShareEll),
    kind === 'album' ? [h('button', { onclick: () => setKind(album, 'trip') }, S.makeTrip), h('button', { onclick: () => setKind(album, 'wedding') }, S.makeWedding)]
      : h('button', { onclick: () => setKind(album, 'album') }, S.makeAlbum),
    album.count > 0 && h('button', { onclick: () => exportDialog(album.key) }, S.exportEll),
    h('div', { class: 'rule' }),
  ].flat().filter(Boolean);
}

const changeable = row => row?.may_change ?? (!row || row.mine || state.me.admin || !row.owner);

function tripSaid(row) {
  if (row.first == null) return null;
  const first = dayNumber(row.first), last = dayNumber(row.last), today = todayNumber();
  return today >= first && today <= last ? phrase('dayN', { n: today - first + 1 }) : phrase(last === first ? 'tripDay' : 'tripDays', { n: last - first + 1 });
}
const routeMark = () => h('span', { class: 'route-mark', role: 'img', 'aria-label': S.aTrip }, icon('route'));
