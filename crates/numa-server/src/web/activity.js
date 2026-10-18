// HOME-010: the background work — a small ring and a word in the main
// navigation while anything runs (an import from a card and the models'
// downloads too), and the Activity page it opens: every job in plain words,
// how far, where, how long is left, the models on their way, and Pause.
'use strict';

const megabytes = bytes => `${grouped(Math.round(bytes / 1e6))} MB`;
function duration(seconds) {
  if (seconds < 90) return S.underTwo;
  if (seconds < 90 * 60) return phrase('aboutMinutes', { n: Math.round(seconds / 60) });
  return phrase('aboutHours', { n: Math.round(seconds / 3600) });
}

// How far the work in hand is, 0..1, or null when nothing is.
function share(activity) {
  const busy = activity.jobs.filter(job => ['running', 'waiting', 'paused'].includes(job.state) && job.total > 0);
  const total = busy.reduce((sum, job) => sum + job.total, 0);
  if (total) return busy.reduce((sum, job) => sum + job.done, 0) / total;
  const fetching = activity.downloads.filter(item => item.state === 'fetching' && item.total > 0);
  const bytes = fetching.reduce((sum, item) => sum + item.total, 0);
  return bytes ? fetching.reduce((sum, item) => sum + item.done, 0) / bytes : null;
}

// The ring's word: what runs, short enough for the navigation.
const SHORT = { scan: S.scanning, thumbnails: S.thumbnails, analyse: S.faces, words: S.words, things: S.things, night: S.night, import: S.importing };
function badgeWord(activity) {
  if (activity.paused) return S.paused;
  const running = activity.jobs.find(job => job.state === 'running');
  if (running) return SHORT[running.job] || running.name;
  if (activity.downloads.some(item => item.state === 'fetching')) return S.models;
  return S.waiting;
}

function ring(fraction, paused) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 20 20');
  svg.setAttribute('class', 'ring');
  const length = 2 * Math.PI * 8;
  const arc = paused ? '' : `<circle cx="10" cy="10" r="8" class="arc" stroke-dasharray="${Math.max(0.04, fraction ?? 0.04) * length} ${length}"/>`;
  svg.innerHTML = `<circle cx="10" cy="10" r="8" class="hair"/>${arc}${paused ? '<path d="M8 7v6M12 7v6" class="pause"/>' : ''}`;
  return svg;
}

// The header's indicator: shown while anything runs, waits or is paused.
// render() and the Activity page both ask; they share one request, the
// badge is looked up once the answer is in (a render on the way replaces
// the header), and the next ask is always set, whatever came back.
let activityTimer = null, activityAsked = null;
async function activityBadge() {
  clearTimeout(activityTimer);
  if (!state.me) return;
  activityAsked ??= api('background', { wait: 15, quiet: true }).then(activity => ({ activity }), error => ({ error }))
    .finally(() => { activityAsked = null; });
  const { activity, error } = await activityAsked;
  const badge = document.querySelector('header .activity');
  const running = activity?.jobs.find(job => job.state === 'running');
  const show = activity && (activity.busy || activity.paused);
  if (badge && activity) {
    badge.hidden = !show;
    if (show) {
      badge.replaceChildren(ring(share(activity), activity.paused), h('span', { class: 'what' }, badgeWord(activity)));
      const numbers = running?.total ? ` · ${phrase('ofTotal', { done: grouped(running.done), total: grouped(running.total) })}` : '';
      badge.title = activity.paused ? S.pausedTip : running ? `${running.doing}${numbers}` : S.backgroundTip;
    }
  }
  if (route().path === '/activity') drawActivity(activity, error);
  clearTimeout(activityTimer);
  activityTimer = setTimeout(activityBadge, route().path === '/activity' || activity?.busy || !activity ? 2000 : 30000);
}

function badgeButton() {
  return h('button', { class: 'chip activity', hidden: true, 'aria-label': S.activity, onclick: () => go('/activity') });
}

// The page's frame at once — heading, Pause, "Loading…" — and the jobs
// when the answer comes. Made once and kept: redrawn every two seconds, a
// button under the pointer would be gone before the click.
function activityPage(main) {
  const said = h('span', { class: 'said' });
  const page = h('div', { class: 'activity-page' });
  const toggle = h('button', { class: 'primary', hidden: true, onclick: async () => {
    try { await api('background/' + (page.paused ? 'resume' : 'pause'), { method: 'POST' }); activityBadge(); } catch (error) { said.textContent = error.message; }
  } });
  // Its heading is Safe at Home's, over the tabs.
  page.append(h('div', { class: 'head' }, h('div', {},
    h('p', { class: 'body' }, S.activityText)), toggle),
  said, h('span', { class: 'said late' }), h('div', { class: 'list' }, h('p', { class: 'body' }, S.loading)));
  main.append(page);
  activityBadge();
}

function drawActivity(activity, error) {
  const page = document.querySelector('.activity-page');
  if (!page) return;
  // An answer that failed leaves the last one standing, the reason over it;
  // with none yet, the reason stands in for "Loading…".
  const late = page.querySelector('.late');
  late.textContent = error && page.querySelector('.job') ? error.message : '';
  if (!activity) return error && page.querySelector('.list').replaceChildren(fault(error.message));
  // Over every photograph when the job knows them all (done and to do),
  // else this round's; waiting for its model, nothing is counted.
  const row = job => {
    const todo = Math.min(job.total - job.done, job.all);
    const whole = job.all > 0 && job.state !== 'waits';
    const fraction = whole ? (job.all - todo) / job.all : job.total ? job.done / job.total : job.state === 'idle' ? 1 : 0;
    const left = job.left != null && phrase('timeLeft', { time: duration(job.left) });
    const numbers = (whole ? [phrase('photosDone', { photos: counted(job.all - todo) }), todo > 0 && phrase('toDo', { n: grouped(todo) }), left]
      : job.total ? [phrase('ofTotal', { done: grouped(job.done), total: counted(job.total) }), left] : []).filter(Boolean).join(' · ');
    return h('div', { class: 'job ' + job.state },
      h('div', { class: 'line' }, h('span', { class: 'name' }, job.name), h('span', { class: 'caption' }, numbers)),
      h('div', { class: 'meter' }, h('div', { class: 'fill', style: filled(fraction) })),
      h('div', { class: 'caption' }, [job.doing, job.library && job.state === 'running' && phrase('inLibrary', { library: job.library })].filter(Boolean).join(' ')));
  };
  const fetching = activity.downloads.filter(download => download.state !== 'done');
  const download = item => h('div', { class: 'job ' + (item.state === 'fetching' ? 'running' : item.state) },
    h('div', { class: 'line' }, h('span', { class: 'name' }, item.name),
      h('span', { class: 'caption' }, phrase('ofTotal', { done: megabytes(item.done), total: megabytes(item.total) }))),
    h('div', { class: 'meter' }, h('div', { class: 'fill', style: filled(item.total ? item.done / item.total : 0) })),
    h('div', { class: 'caption' }, { waiting: S.waitsTurn, fetching: S.downloading, failed: S.downloadFailed }[item.state]));
  page.paused = activity.paused;
  const toggle = page.querySelector('.head .primary');
  toggle.hidden = !state.me.admin;
  toggle.textContent = activity.paused ? S.resume : S.pause;
  page.querySelector('.head .body').textContent = activity.paused
    ? S.pausedText
    : S.activityText;
  page.querySelector('.list').replaceChildren(...[
    ...lastNight(activity.night),
    h('div', { class: 'section-label' }, S.background),
    ...activity.jobs.filter(job => job.job !== 'night' || job.state !== 'idle').map(row),
    fetching.length > 0 && h('div', { class: 'section-label' }, S.models),
    ...fetching.map(download),
  ].filter(Boolean));
}

// HOME-020: what the night's round did, and what needs a person, each
// linking to where to act.
function lastNight(night) {
  if (!night) return [];
  const at = seconds => new Date(seconds * 1000).toLocaleTimeString(S.locale, { hour: '2-digit', minute: '2-digit' });
  const today = new Date().toLocaleDateString('sv-SE');
  const span = night.finished ? `${at(night.started)}–${at(night.finished)}` : phrase('sinceAt', { time: at(night.started) });
  return [
    h('div', { class: 'section-label' }, night.date === today ? S.lastNight : phrase('nightOf', { date: night.date })),
    h('div', { class: 'night' },
      h('p', { class: 'caption' }, span),
      night.stopped && h('p', { class: 'body' }, night.stopped),
      h('ul', { class: 'done' }, night.done.map(line => h('li', {}, line))),
      night.needs.length > 0 && h('div', { class: 'section-label' }, S.needsPerson),
      night.needs.length > 0 && h('ul', { class: 'needs' }, night.needs.map(need => h('li', {}, h('a', { href: need.link }, need.text))))),
  ];
}

// HOME-035: Safe at Home — whether everything that should come home did,
// room for it, and last night. Accounts, Cameras and Activity are its tabs,
// a chip row under its title.
const SAFE = ['/safe', '/accounts', '/cameras', '/activity'];
function safeHead(path) {
  const tabs = [[S.overview, '/safe'], state.me.admin && [S.accounts, '/accounts'], state.me.admin && [S.cameras, '/cameras'], [S.activity, '/activity']].filter(Boolean);
  if (!tabs.some(([, to]) => to === path)) return null;
  return h('div', { class: 'safe-head' }, h('h1', {}, S.safeAtHome),
    h('div', { class: 'chips' }, tabs.map(([label, to]) => h('button', { class: 'chip' + (to === path ? ' chosen' : ''),
      'aria-current': to === path ? 'page' : null, onclick: () => go(to) }, label))));
}

// As the server says a disk's size: 120 GB, 3.8 TB.
const diskSize = bytes => bytes >= 1e12 ? `${(bytes / 1e12).toFixed(1)} TB` : `${grouped(Math.round(bytes / 1e9))} GB`;
function ago(at) {
  const seconds = Date.now() / 1000 - at;
  if (seconds < 90) return S.justNow;
  if (seconds < 90 * 60) return phrase('minAgo', { n: Math.round(seconds / 60) });
  if (seconds < 36 * 3600) return phrase('hoursAgo', { n: Math.round(seconds / 3600) });
  return phrase('daysAgo', { n: Math.round(seconds / 86400) });
}

async function safePage(main) {
  const page = h('div', { class: 'safe' }, h('p', { class: 'body' }, S.loading));
  main.append(page);
  const [safe, activity] = await Promise.all([api('safe'), api('background').catch(() => null)])
    .catch(error => (page.replaceChildren(fault(error.message)), []));
  if (!safe) return;
  // `attention` only on what asks for a person, always with its words.
  const top = safe.attention.length
    ? safe.attention.map(item => h('div', { class: 'attention' }, h('i', { class: 'signal' }), h('span', {}, item.text)))
    : safe.devices.length > 0 && h('p', { class: 'all-home' }, S.allHome);
  const device = one => h('li', {},
    h('div', {}, h('div', { class: 'name' }, one.label), h('div', { class: 'caption' }, one.kind === 'card' ? phrase('cardInNas', { photos: counted(one.count) }) : S.sendHome)),
    h('span', { class: 'caption' + (one.late ? ' late' : '') }, ago(one.sent)));
  const disk = one => h('div', { class: 'disk' },
    h('div', { class: 'big' }, diskSize(one.free)),
    h('div', { class: 'caption' }, phrase('freeOf', { total: diskSize(one.total), names: one.names.join(', ') })),
    h('div', { class: 'meter' }, h('div', { class: 'fill', style: filled(one.total ? 1 - one.free / one.total : 0) })));
  const night = activity && activity.night;
  // What this account may do, and who does the rest (P14).
  page.replaceChildren(h('p', { class: 'body role' }, safe.role), ...[top].flat().filter(Boolean), h('div', { class: 'boxes' },
    h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.devices), safe.devices.length
      ? h('ul', {}, safe.devices.map(device))
      : h('p', { class: 'body' }, S.devicesEmpty)),
    h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.storage), safe.disks.map(disk)),
    night && h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.lastNight),
      h('ul', { class: 'done' }, night.done.slice(0, 4).map(line => h('li', {}, line))),
      h('button', { class: 'chip', onclick: () => go('/activity') }, S.activity)),
    h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.numaHome), h('div', { class: 'big' }, safe.version),
      // The licences, where the version is (and where an Updates tab will hold them): P15, a visible route.
      h('a', { class: 'caption', href: '/licences', target: '_blank', rel: 'noopener' }, S.licences))));
}
