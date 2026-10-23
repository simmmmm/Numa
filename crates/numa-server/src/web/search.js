// HOME-040: search with tokens. One field holds the page's filters as
// tokens, then the words being typed. Typing suggests things, places,
// people, cameras, dates and libraries, each with how many photographs it
// keeps among the tokens already there. A token is the Filter menu's chip:
// the same parameter of the address, shown in both places.
'use strict';

// What a token can be, in the order they stand (FILTERS'), and its kind's word.
const TOKEN_KINDS = { library: S.library, person: S.person, place: S.placeLabel, region: S.placeLabel, camera: S.camera, date: S.date, thing: S.thing };
const tokensOf = params => FILTERS.filter(key => TOKEN_KINDS[key] && params.get(key));
const tokenSaid = (key, value, list = state.libraries) => key === 'library'
  ? libraryName(list?.find(one => String(one.id) === value) || { name: S.library })
  : key === 'date' ? dateName(value) : value;
// Set before a token changes the page: the new page's field takes the focus,
// so the keys go on where they were. Not on touch, where it would raise the keyboard.
let refocus = false;

// Folded for matching: no case and no accents, so "sao" finds São Paulo.
const fold = text => text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
// Where each typed word starts a word of `name` ("bea" in Beasain, "sao pa"
// in São Paulo), as [from, to] in its characters; null when one does not.
function matched(name, words) {
  const chars = [...name], at = [];
  let low = '';
  chars.forEach((c, i) => { const f = fold(c); low += f; at.push(...Array(f.length).fill(i)); });
  at.push(chars.length);
  const start = word => {
    for (let i = low.indexOf(word); i >= 0; i = low.indexOf(word, i + 1)) if (i === 0 || !/[\p{L}\p{N}]/u.test(low[i - 1])) return i;
    return -1;
  };
  const spans = [];
  for (const word of words) {
    const i = start(word);
    if (i < 0) return null;
    spans.push([at[i], at[i + word.length]]);
  }
  return spans;
}
// A name with the typed part in bold.
function emphasised(label, spans) {
  const parts = [];
  [...label].forEach((c, i) => {
    const bold = spans.some(([from, to]) => i >= from && i < to);
    if (parts.length && parts[parts.length - 1].bold === bold) parts[parts.length - 1].text += c;
    else parts.push({ bold, text: c });
  });
  return parts.map(part => part.bold ? h('b', {}, part.text) : part.text);
}

// What the suggestions are made of, asked once the field is typed in: the
// Filter menu's choices, where the photographs were taken (regions too) and
// their months, each counted among the page's filters but its own.
function suggestionData(params) {
  const but = (...keys) => narrowedBy(params).filter(([key]) => key !== 'q' && !keys.includes(key));
  const ask = (path, pairs) => api(path + '?' + new URLSearchParams(pairs), { quiet: true }).catch(() => null);
  const days = new Map();
  return {
    filters: ask('filters', but()),
    places: ask('places', but('place', 'region')),
    months: ask('timeline', but('date')),
    day: value => days.get(value) || days.set(value, ask('timeline', [...but('date'), ['date', value]])).get(value),
  };
}

// Countries, regions and towns, each with how many photographs were taken there.
function placeRows(places, row) {
  const counts = new Map();
  const add = (key, value, label) => {
    const id = key + '\n' + label;
    counts.set(id, [key, value, label, (counts.get(id)?.[3] || 0) + 1]);
  };
  for (const [, , , at] of places?.points || []) {
    const [town, region, country] = places.towns[at];
    add('place', country, country);
    if (region) add('region', region, `${region}, ${country}`);
    add('place', town, `${town}, ${country}`);
  }
  return [...counts.values()].map(([key, value, label, n]) => row(key, value, value, n, label));
}

// A year, a month ("2025-03", "March 2025", "mar 2025") or a day ("2025-03-14"),
// offered only when photographs were taken then.
async function dateRows(data, months, text, row) {
  months = (months || []).filter(({ month }) => /^\d{4}-\d{2}$/.test(month));
  const years = new Map();
  for (const { month, count } of months) years.set(month.slice(0, 4), (years.get(month.slice(0, 4)) || 0) + count);
  const rows = [...years].map(([year, n]) => row('date', year, year, n))
    .concat(months.map(({ month, count }) => row('date', month, monthName(month), count, monthName(month), month)));
  if (/^\d{4}-\d{2}-\d{2}$/.test(text)) {
    const n = ((await data.day(text)) || []).reduce((sum, month) => sum + month.count, 0);
    if (n) rows.push({ key: 'date', value: text, label: dateName(text), spans: [], count: n });
  }
  return rows;
}

// The sections in the spec's order, each its three that keep the most.
async function suggestions(data, text) {
  const words = fold(text).split(/\s+/).filter(Boolean);
  const [filters, places, months, list] = await Promise.all([data.filters, data.places, data.months, libraries().catch(() => [])]);
  const row = (key, value, name, count, label = name, alias) => {
    const spans = matched(name, words) || (alias && matched(alias, words) && []);
    return spans && { key, value: String(value), label, spans, count };
  };
  const pairs = (key, list) => (list || []).map(([name, n]) => row(key, name, name, n));
  const sections = [
    [S.things, pairs('thing', filters?.things)],
    [S.places, placeRows(places, row)],
    [S.people, pairs('person', filters?.people)],
    [S.cameras, pairs('camera', filters?.cameras)],
    [S.dates, await dateRows(data, months, text, row)],
    [S.libraries, (filters?.libraries || []).map(([id, n]) => { const one = list.find(one => one.id === id); return one && row('library', id, libraryName(one), n); })],
  ];
  const best = rows => rows.filter(Boolean).sort((a, b) => b.count - a.count)
    .filter((one, at, all) => all.findIndex(other => other.label === one.label) === at).slice(0, 3);
  return sections.map(([label, rows]) => [label, best(rows)]).filter(([, rows]) => rows.length);
}

// The header's field: the tokens, the words, and under it what typing
// suggests: a combobox and its listbox, the focus staying in the field.
function searchField(params) {
  const tokens = tokensOf(params);
  const field = h('input', {
    type: 'search', placeholder: tokens.length ? null : S.searchHint, value: params.get('q') || '', enterkeyhint: 'search', 'aria-label': S.search, autocomplete: 'off',
    role: 'combobox', 'aria-autocomplete': 'list', 'aria-expanded': 'false', 'aria-controls': 'suggestions',
  });
  const box = h('div', { class: 'menu suggest', id: 'suggestions', role: 'listbox', 'aria-label': S.suggestions, hidden: true,
    onmousedown: event => event.preventDefault(),
    // On a phone the list is the screen's height: its empty part closes it.
    onclick: event => { if (event.target === box) field.blur(); } });
  let data = null, options = [], at = -1;
  const highlight = to => {
    at = to;
    options.forEach((option, i) => option.setAttribute('aria-selected', String(i === at)));
    if (at < 0) return field.removeAttribute('aria-activedescendant');
    field.setAttribute('aria-activedescendant', options[at].id);
    options[at].scrollIntoView({ block: 'nearest' });
  };
  const close = () => { box.hidden = true; field.setAttribute('aria-expanded', 'false'); highlight(-1); };
  const leave = change => {
    refocus = !matchMedia('(pointer: coarse)').matches;
    narrow(params, change);
  };
  const choose = row => {
    if (row.key !== 'q') field.value = '';
    close();
    leave(row.key === 'q' ? { q: row.value || null } : { [row.key]: row.value, q: null });
  };
  const option = (row, n, label, end, spoken) => h('button', { type: 'button', role: 'option', id: 'suggestion-' + n, tabindex: -1, 'aria-selected': 'false', 'aria-label': spoken,
    onclick: () => choose(row) }, label, end);
  const group = (n, label, rows) => h('div', { role: 'group', 'aria-labelledby': 'suggested-' + n }, h('div', { class: 'section-label', id: 'suggested-' + n }, label), rows);
  const paint = (text, sections) => {
    let n = 0;
    const [before, after] = S.looksLike.split('{text}');
    box.replaceChildren(...sections.map(([label, rows], s) => group(s, label, rows.map(row =>
      option(row, n++, h('span', {}, emphasised(row.label, row.spans)), h('span', { class: 'caption' }, grouped(row.count)), `${row.label}, ${counted(row.count)}`)))),
    group('words', S.words, option({ key: 'q', value: text }, n, h('span', {}, before, h('b', {}, text), after), h('span', { class: 'caption key' }, S.enterKey), phrase('looksLike', { text }))));
    options = [...box.querySelectorAll('[role=option]')];
    box.hidden = false;
    field.setAttribute('aria-expanded', 'true');
    highlight(-1);
  };
  const update = async () => {
    const text = field.value.trim();
    if (!text) return close();
    paint(text, []);
    data ??= suggestionData(params);
    const sections = await suggestions(data, text);
    if (field.value.trim() === text && document.activeElement === field) paint(text, sections);
  };
  field.addEventListener('input', update);
  field.addEventListener('blur', close);
  field.addEventListener('keydown', event => {
    if (event.isComposing) return;
    const open = !box.hidden;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!open) return update();
      highlight(event.key === 'ArrowDown' ? (at + 1) % options.length : at <= 0 ? options.length - 1 : at - 1);
    } else if (event.key === 'Enter' && open && at >= 0) {
      event.preventDefault();
      options[at].click();
    } else if (event.key === 'Escape' && open) {
      event.preventDefault();
      close();
    } else if (event.key === 'Backspace' && !field.value && tokens.length) {
      event.preventDefault();
      leave({ [tokens[tokens.length - 1]]: null, q: null });
    }
  });
  const token = key => {
    const value = params.get(key);
    const name = h('span', {}, tokenSaid(key, value));
    const x = h('button', { type: 'button', class: 'x', 'aria-label': phrase('removeFilter', { name: name.textContent }), onclick: () => leave({ [key]: null }) }, icon('close'));
    if (key === 'library') libraries().then(list => {
      name.textContent = tokenSaid(key, value, list);
      x.setAttribute('aria-label', phrase('removeFilter', { name: name.textContent }));
    }, () => {});
    return h('span', { class: 'chip small token' }, h('span', { class: 'kind' }, TOKEN_KINDS[key]), name, x);
  };
  const form = h('form', { class: 'field', role: 'search', onsubmit: event => { event.preventDefault(); choose({ key: 'q', value: field.value.trim() }); } },
    tokens.map(token), field);
  if (refocus) {
    refocus = false;
    queueMicrotask(() => field.focus({ preventScroll: true }));
  }
  return h('div', { class: 'search' }, form, box);
}
