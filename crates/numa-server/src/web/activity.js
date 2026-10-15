// HOME-010: the background work — a small ring and a word in the main
// navigation while anything runs (an import from a card and the models'
// downloads too), and the Activity page it opens: every job in plain words,
// how far, where, how long is left, the models on their way, and Pause.
'use strict';

const megabytes = bytes => `${grouped(Math.round(bytes / 1e6))} MB`;
function duration(seconds) {
  if (seconds < 90) return 'under 2 minutes';
  if (seconds < 90 * 60) return `about ${Math.round(seconds / 60)} minutes`;
  return `about ${Math.round(seconds / 3600)} hours`;
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
const SHORT = { scan: 'Scanning', thumbnails: 'Thumbnails', analyse: 'Faces', words: 'Words', things: 'Things', night: 'Night', import: 'Importing' };
function badgeWord(activity) {
  if (activity.paused) return 'Paused';
  const running = activity.jobs.find(job => job.state === 'running');
  if (running) return SHORT[running.job] || running.name;
  if (activity.downloads.some(item => item.state === 'fetching')) return 'Models';
  return 'Waiting';
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
  activityAsked ??= api('activity', { wait: 15 }).then(activity => ({ activity }), error => ({ error }))
    .finally(() => { activityAsked = null; });
  const { activity, error } = await activityAsked;
  const badge = document.querySelector('header .activity');
  const running = activity?.jobs.find(job => job.state === 'running');
  const show = activity && (activity.busy || activity.paused);
  if (badge && activity) {
    badge.hidden = !show;
    if (show) {
      badge.replaceChildren(ring(share(activity), activity.paused), h('span', { class: 'what' }, badgeWord(activity)));
      const numbers = running?.total ? ` · ${grouped(running.done)} of ${grouped(running.total)}` : '';
      badge.title = activity.paused ? 'Background work paused' : running ? `${running.doing}${numbers}` : 'Background work';
    }
  }
  if (route().path === '/activity') drawActivity(activity, error);
  clearTimeout(activityTimer);
  activityTimer = setTimeout(activityBadge, route().path === '/activity' || activity?.busy || !activity ? 2000 : 30000);
}

function badgeButton() {
  return h('button', { class: 'chip activity', hidden: true, 'aria-label': 'Activity', onclick: () => go('/activity') });
}

// The page's frame at once — heading, Pause, "Loading…" — and the jobs
// when the answer comes. Made once and kept: redrawn every two seconds, a
// button under the pointer would be gone before the click.
function activityPage(main) {
  const said = h('span', { class: 'said' });
  const page = h('div', { class: 'activity-page' });
  const toggle = h('button', { class: 'primary', hidden: true, onclick: async () => {
    try { await api('activity/' + (page.paused ? 'resume' : 'pause'), { method: 'POST' }); activityBadge(); } catch (error) { said.textContent = error.message; }
  } });
  page.append(h('div', { class: 'head' }, h('div', {}, h('h1', {}, 'Activity'),
    h('p', { class: 'body' }, 'What Numa Home does by itself, gently, so the NAS serves the photographs first.')), toggle),
  said, h('span', { class: 'said late' }), h('div', { class: 'list' }, h('p', { class: 'body' }, 'Loading…')));
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
  if (!activity) return error && page.querySelector('.list').replaceChildren(h('p', { class: 'body' }, error.message));
  const row = job => {
    const fraction = job.total ? job.done / job.total : job.state === 'idle' ? 1 : 0;
    const numbers = job.total
      ? [`${grouped(job.done)} of ${counted(job.total)}`, job.left != null && `${duration(job.left)} left`].filter(Boolean).join(' · ')
      : '';
    return h('div', { class: 'job ' + job.state },
      h('div', { class: 'line' }, h('span', { class: 'name' }, job.name), h('span', { class: 'caption' }, numbers)),
      h('div', { class: 'meter' }, h('div', { class: 'fill', style: { width: fraction * 100 + '%' } })),
      h('div', { class: 'caption' }, [job.doing, job.library && job.state === 'running' && `in ${job.library}`].filter(Boolean).join(' ')));
  };
  const fetching = activity.downloads.filter(download => download.state !== 'done');
  const download = item => h('div', { class: 'job ' + (item.state === 'fetching' ? 'running' : item.state) },
    h('div', { class: 'line' }, h('span', { class: 'name' }, item.name),
      h('span', { class: 'caption' }, `${megabytes(item.done)} of ${megabytes(item.total)}`)),
    h('div', { class: 'meter' }, h('div', { class: 'fill', style: { width: (item.total ? item.done / item.total : 0) * 100 + '%' } })),
    h('div', { class: 'caption' }, { waiting: 'Waits its turn', fetching: 'Downloading', failed: 'Could not download; tried again at the next start' }[item.state]));
  page.paused = activity.paused;
  const toggle = page.querySelector('.head .primary');
  toggle.hidden = !state.me.admin;
  toggle.textContent = activity.paused ? 'Resume' : 'Pause';
  page.querySelector('.head .body').textContent = activity.paused
    ? 'Paused. Nothing new is started; the photographs in hand were finished. Numa Home still shows and sends everything.'
    : 'What Numa Home does by itself, gently, so the NAS serves the photographs first.';
  page.querySelector('.list').replaceChildren(...[
    ...lastNight(activity.night),
    h('div', { class: 'section-label' }, 'Background'),
    ...activity.jobs.filter(job => job.job !== 'night' || job.state !== 'idle').map(row),
    fetching.length > 0 && h('div', { class: 'section-label' }, 'Models'),
    ...fetching.map(download),
  ].filter(Boolean));
}

// HOME-020: what the night's round did, and what needs a person, each
// linking to where to act.
function lastNight(night) {
  if (!night) return [];
  const at = seconds => new Date(seconds * 1000).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
  const today = new Date().toLocaleDateString('sv-SE');
  const span = night.finished ? `${at(night.started)}–${at(night.finished)}` : `since ${at(night.started)}`;
  return [
    h('div', { class: 'section-label' }, night.date === today ? 'Last Night' : `The Night of ${night.date}`),
    h('div', { class: 'night' },
      h('p', { class: 'caption' }, span),
      night.stopped && h('p', { class: 'body' }, night.stopped),
      h('ul', { class: 'done' }, night.done.map(line => h('li', {}, line))),
      night.needs.length > 0 && h('div', { class: 'section-label' }, 'Needs a Person'),
      night.needs.length > 0 && h('ul', { class: 'needs' }, night.needs.map(need => h('li', {}, h('a', { href: need.link }, need.text))))),
  ];
}
