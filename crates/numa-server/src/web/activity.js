'use strict';

const megabytes = bytes => `${grouped(Math.round(bytes / 1e6))} MB`;
function duration(seconds) {
  if (seconds < 90) return S.underTwo;
  if (seconds < 90 * 60) return phrase('aboutMinutes', { n: Math.round(seconds / 60) });
  return phrase('aboutHours', { n: Math.round(seconds / 3600) });
}

function share(activity) {
  const busy = activity.jobs.filter(job => ['running', 'waiting', 'paused'].includes(job.state) && job.total > 0);
  const total = busy.reduce((sum, job) => sum + job.total, 0);
  if (total) return busy.reduce((sum, job) => sum + job.done, 0) / total;
  const fetching = activity.downloads.filter(item => item.state === 'fetching' && item.total > 0);
  const bytes = fetching.reduce((sum, item) => sum + item.total, 0);
  return bytes ? fetching.reduce((sum, item) => sum + item.done, 0) / bytes : null;
}

const SHORT = { scan: S.scanning, thumbnails: S.thumbnails, analyse: S.faces, words: S.words, things: S.things, night: S.night, import: S.importing, copy: S.copying };
function badgeWord(activity) {
  if (activity.paused) return S.paused;
  const running = activity.jobs.find(job => job.state === 'running');
  if (running) return SHORT[running.job] || running.name;
  const fetching = activity.downloads.find(item => item.state === 'fetching');
  if (fetching) return fetching.name === S.map ? S.map : S.models;
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

  const map = activity && route().path === '/places' && mapProgress(activity);
  clearTimeout(activityTimer);
  activityTimer = setTimeout(activityBadge, map ? 500 : route().path === '/activity' || activity?.busy || !activity ? 2000 : 30000);
}

function badgeButton() {
  return h('button', { class: 'chip activity', hidden: true, 'aria-label': S.activity, onclick: () => go('/activity') });
}

function activityPage(main) {
  const said = h('span', { class: 'said' });
  const page = h('div', { class: 'activity-page' });
  const toggle = h('button', { class: 'primary', hidden: true, onclick: async () => {
    try { await api('background/' + (page.paused ? 'resume' : 'pause'), { method: 'POST' }); activityBadge(); } catch (error) { said.textContent = error.message; }
  } });

  page.append(h('div', { class: 'head' }, h('div', {},
    h('p', { class: 'body' }, S.activityText)), toggle),
  said, h('span', { class: 'said late' }), h('div', { class: 'list' }, h('p', { class: 'body' }, S.loading)));
  main.append(page);
  if (state.me.admin) signInsBox(page);
  activityBadge();
}

function drawActivity(activity, error) {
  const page = document.querySelector('.activity-page');
  if (!page) return;

  const late = page.querySelector('.late');
  late.textContent = error && page.querySelector('.job') ? error.message : '';
  if (!activity) return error && page.querySelector('.list').replaceChildren(fault(error.message));

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
      h('span', { class: 'caption' }, item.total ? phrase('ofTotal', { done: megabytes(item.done), total: megabytes(item.total) }) : '')),
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

const SAFE = ['/safe', '/accounts', '/devices', '/cameras', '/reach', '/activity'];
function safeHead(path) {
  const tabs = [[S.safeAtHome, '/safe'], state.me.admin && [S.accounts, '/accounts'], [S.devices, '/devices'], state.me.admin && [S.cameras, '/cameras'], state.me.admin && [S.reachable, '/reach'],
    [S.activity, '/activity']].filter(Boolean);
  if (!tabs.some(([, to]) => to === path)) return null;
  return h('div', { class: 'safe-head' }, h('h1', {}, S.settings),
    h('div', { class: 'chips' }, tabs.map(([label, to]) => h('button', { class: 'chip' + (to === path ? ' chosen' : ''),
      'aria-current': to === path ? 'page' : null, onclick: () => go(to) }, label))));
}

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
  const [safe, activity, rights] = await Promise.all([api('safe'), api('background').catch(() => null), state.me.admin ? api('rights').catch(() => null) : null])
    .catch(error => (page.replaceChildren(fault(error.message)), []));
  if (!safe) return;

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

  page.replaceChildren(h('p', { class: 'body role' }, safe.role), ...[top].flat().filter(Boolean), h('div', { class: 'boxes' },
    h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.devices), safe.devices.length
      ? h('ul', {}, safe.devices.map(device))
      : h('p', { class: 'body' }, S.devicesEmpty)),
    h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.storage), safe.disks.map(disk)),
    secondCopy(safe.copy),
    rights && homeRights(rights),
    night && h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.lastNight),
      h('ul', { class: 'done' }, night.done.slice(0, 4).map(line => h('li', {}, line))),
      h('button', { class: 'chip', onclick: () => go('/activity') }, S.activity)),
    h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.numaHome), h('div', { class: 'big' }, safe.version),

      h('a', { class: 'caption', href: '/licences', target: '_blank', rel: 'noopener' }, S.licences))));
}

function homeRights(rights) {
  const creator = h('input', { type: 'text', placeholder: S.creator, 'aria-label': S.creator, value: rights.creator, autocomplete: 'off' });
  const copyright = h('input', { type: 'text', placeholder: S.copyright, 'aria-label': S.copyright, value: rights.copyright, autocomplete: 'off' });
  const save = async event => {
    event.preventDefault();
    try {
      await api('rights', { method: 'PUT', body: JSON.stringify({ creator: creator.value, copyright: copyright.value }) });
      toast(S.rightsSaved);
    } catch (error) { toast(error.message); }
  };
  return h('form', { class: 'box', onsubmit: save }, h('div', { class: 'section-label' }, S.inHomesJpegs),
    creator, copyright, h('p', { class: 'caption' }, S.inHomesJpegsText), h('button', { class: 'chip', type: 'submit' }, S.save));
}

const GUIDE_RESTORE = 'https://guide.numa.photo/numa-home#restore';
function secondCopy(copy) {
  const amount = bytes => bytes >= 1e9 ? diskSize(bytes) : megabytes(bytes);
  const clock = seconds => Date.now() / 1000 - seconds < 36 * 3600
    ? new Date(seconds * 1000).toLocaleTimeString(S.locale, { hour: '2-digit', minute: '2-digit' })
    : new Date(seconds * 1000).toLocaleDateString(S.locale, { day: 'numeric', month: 'short' });
  const holds = `${grouped(copy.files)} ${copy.files === 1 ? S.file : S.files}, ${amount(copy.bytes)}`;
  const when = copy.exists && h('div', { class: 'copy-when' },
    h('span', { class: 'big' }, copy.last ? clock(copy.last) : S.notYet),
    h('span', { class: 'caption' }, copy.running ? S.copyingNow : copy.last ? `${ago(copy.last)} · ${holds}` : S.copyTonight));
  const disk = copy.exists && h('li', {}, h('div', {}, h('div', { class: 'name' }, copy.where),
    h('div', { class: 'caption' }, [copy.total && phrase('freeShort', { free: diskSize(copy.free), total: diskSize(copy.total) }), S.onlyAdds].filter(Boolean).join(' · '))));
  const databases = h('li', {}, h('div', {}, h('div', { class: 'name' }, S.databaseCopies),
    h('div', { class: 'caption' }, copy.databases.newest ? phrase(copy.databases.count === 1 ? 'nightlyCopy' : 'nightlyCopies', { n: copy.databases.count, when: clock(copy.databases.newest) }) : S.databasesTonight)));
  const problems = [!copy.complete && copy.exists && copy.problem, copy.databases.problem].filter(Boolean);
  const now = state.me.admin
    ? h('button', { class: 'chip', onclick: async () => { try { await api('copy', { method: 'POST' }); toast(S.copyStarts); } catch (error) { toast(error.message); } } }, S.copyNow)
    : h('button', { class: 'chip', 'aria-disabled': 'true', title: S.copyRefused, onclick: () => toast(S.copyRefused) }, S.copyNow);
  return h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.secondCopy),
    when || h('p', { class: 'body' }, S.noCopyDisk),
    h('ul', {}, disk, databases),
    problems.map(problem => h('p', { class: 'caption' }, problem)),
    h('div', { class: 'copy-acts' }, now, h('a', { class: 'caption', href: GUIDE_RESTORE, target: '_blank', rel: 'noopener' }, S.howToRestore)));
}

async function devicesPage(main) {
  const page = h('div', { class: 'safe devices' }, h('p', { class: 'body' }, S.loading));
  main.append(page);
  const list = await api('sessions').catch(error => (page.replaceChildren(fault(error.message)), null));
  if (!list) return;
  const day = seconds => new Date(seconds * 1000).toLocaleDateString(S.locale, { day: 'numeric', month: 'long', year: 'numeric' });
  const mine = one => one.account.toLowerCase() === state.me.name.toLowerCase();
  const out = async one => {
    try { await api(`sessions/${one.id}`, { method: 'DELETE' }); } catch (error) { return toast(error.message); }
    if (one.this) return signIn();
    toast(phrase('signedOutOf', { device: one.label }));
    render();
  };
  const row = one => h('li', {},
    h('div', {}, h('div', { class: 'name' }, one.label, one.this && h('span', { class: 'caption' }, ` · ${S.thisDevice}`)),
      h('div', { class: 'caption' }, [phrase('signedInOn', { day: day(one.made) }), !one.this && phrase('seenAgo', { ago: ago(one.seen) }),
        one.sent && phrase('sentAgo', { ago: ago(one.sent) }), one.address].filter(Boolean).join(' · '))),
    h('button', { class: 'chip', onclick: () => out(one) }, S.signOut));
  const others = list.filter(one => mine(one) && !one.this).length;
  const elsewhere = others > 0 && h('button', { class: 'primary', onclick: async () => {
    try { await api('sessions/others', { method: 'POST' }); toast(S.signedOutElsewhere); render(); } catch (error) { toast(error.message); }
  } }, S.signOutElsewhere);
  page.replaceChildren(h('p', { class: 'body role' }, state.me.admin ? S.devicesAdminText : S.devicesText),
    h('section', { class: 'box' }, h('ul', {}, [...list.filter(one => one.this), ...list.filter(one => !one.this)].map(row))), elsewhere);
}
