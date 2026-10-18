// HOME-039: Today, Numa Home's front page: what was taken on this day in
// earlier years, what came home in the last two weeks, what waits for a
// person, and what the night did. A part with nothing in it is left out.
'use strict';

// The page an address without a route opens, and the mark goes to.
const startPage = () => state.me?.start === 'photos' ? '/' : '/today';

// Three prints, the first photograph in front; grey where there is none.
const prints = ids => h('div', { class: 'stack' }, [2, 1, 0].map(at => h('div', { class: 'print' },
  ids[at] != null && h('img', { src: thumb(ids[at], 256), loading: 'lazy', alt: '' }))));
const openIds = (ids, at) => openFrame({ photos: ids.map(id => ({ id })) }, at);

function cameAgo(at) {
  const days = Math.floor((Date.now() / 1000 - at) / 86400);
  return days < 1 ? S.cameToday : days < 2 ? S.cameYesterday : phrase('cameDays', { n: days });
}

// `day` (MM-DD) shows another day's On This Day, for a test.
async function today(main, params) {
  state.sections = [];
  const day = params.get('day');
  const night = api('background', { quiet: true }).then(activity => activity.night, () => null);
  const safe = api('safe', { quiet: true }).catch(() => null);
  let found;
  try { found = await api('today' + (day ? '?day=' + encodeURIComponent(day) : '')); } catch (error) {
    return main.append(empty(S.today, error.message, true));
  }
  const [report, home] = await Promise.all([night, safe]);
  if (!main.isConnected) return;
  // HOME-035's one line: what needs a person first, else that all is home.
  const said = home && (home.attention[0]?.text || (home.devices.length > 0 && S.allHomeLine));
  const pill = said && h('a', { class: 'safe-pill', href: '#/safe' },
    h('i', { class: 'dot' + (home.attention.length ? ' attention' : '') }), said);

  const years = found.on_this_day.length > 0 && h('section', { class: 'today-part' },
    h('div', { class: 'head' }, h('span', { class: 'section-label' }, S.onThisDay), h('span', { class: 'caption' }, S.picksFirst)),
    h('div', { class: 'places' }, found.on_this_day.map(year => h('button', { class: 'place', onclick: () => openIds(year.ids, 0) }, prints(year.ids),
      h('div', {}, h('div', { class: 'name' }, phrase(year.ago === 1 ? 'yearAgo' : 'yearsAgo', { n: year.ago })),
        h('div', { class: 'caption' }, `${year.name ? libraryName(year) : librariesCounted(year.libraries)} · ${counted(year.count)}`))))));

  const arrived = found.arrived.length > 0 && h('section', { class: 'today-part' },
    h('div', { class: 'section-label' }, S.justArrived),
    found.arrived.map(group => h('div', { class: 'arrival' },
      h('div', { class: 'head' }, h('span', { class: 'name' }, libraryName(group)), h('span', { class: 'caption' }, `${counted(group.count)} · ${cameAgo(group.at)}`),
        h('button', { class: 'chip', onclick: () => go('/', { library: group.library }) }, S.open)),
      h('div', { class: 'strip' }, group.ids.map((id, at) => h('button', { 'aria-label': phrase('stripPhoto', { library: libraryName(group), n: at + 1, of: group.ids.length }), onclick: () => openIds(group.ids, at) },
        h('img', { src: thumb(id, 256), loading: 'lazy', alt: '' })))))));

  const { tidy, faces, folders } = found.waiting;
  const wait = (n, one, more, caption, label, to) => n > 0 && h('div', { class: 'wait' },
    h('div', { class: 'what' }, h('span', { class: 'name' }, `${grouped(n)} ${n === 1 ? one : more}`), h('span', { class: 'caption' }, caption)),
    h('button', { class: 'chip', onclick: () => go(to) }, label));
  const waits = [
    wait(tidy, S.setToChoose, S.setsToChoose, S.tidyUp, S.choose, '/tidy'),
    wait(faces, S.faceToName, S.facesToName, S.peopleNotNamed, S.name, '/people'),
    wait(folders, S.newFolderIn, S.newFoldersIn, S.librariesNotIn, S.takeIn, '/libraries'),
  ].filter(Boolean);
  const waiting = waits.length > 0 && h('section', { class: 'today-part waiting' }, h('div', { class: 'section-label' }, S.waitingForYou), waits);

  // The night's first lines; Activity has the rest.
  const lastLines = report && h('section', { class: 'today-part' },
    lastNight({ ...report, done: report.done.slice(0, 3), needs: [] }), h('a', { class: 'caption', href: '#/activity' }, S.activity));

  const left = [years, arrived].filter(Boolean), right = [waiting, lastLines].filter(Boolean);
  if (!found.photos || !left.length && !right.length) return main.append(empty(S.today, S.todayEmpty));
  const date = new Date(found.date + 'T12:00:00Z').toLocaleDateString(S.locale, { weekday: 'long', day: 'numeric', month: 'long', timeZone: 'UTC' });
  main.append(h('div', { class: 'today' + (left.length && right.length ? ' two' : '') },
    h('div', { class: 'today-head' }, h('h1', {}, S.today), h('span', { class: 'caption' }, date), pill),
    left.length > 0 && h('div', { class: 'today-column' }, left),
    right.length > 0 && h('aside', { class: 'today-column' }, right)));
}
