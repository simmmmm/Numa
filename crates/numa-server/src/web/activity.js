// HOME-010: the background work — a small ring in the header while
// anything runs, and the Activity page it opens: every job in plain words,
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
  return total ? busy.reduce((sum, job) => sum + job.done, 0) / total : null;
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
let activityTimer = null;
async function activityBadge() {
  clearTimeout(activityTimer);
  const badge = document.querySelector('header .activity');
  if (!badge || !state.me) return;
  let activity = null;
  try { activity = await api('activity'); } catch (_) { activity = null; }
  if (!badge.isConnected) return;
  const running = activity?.jobs.find(job => job.state === 'running');
  const show = activity && (activity.busy || activity.paused);
  badge.hidden = !show;
  if (show) {
    const label = activity.paused ? 'Paused' : running ? running.name : 'Waiting';
    badge.replaceChildren(ring(share(activity), activity.paused), h('span', { class: 'what' }, label));
    badge.title = activity.paused ? 'Background work paused' : running ? `${running.doing}…` : 'Background work';
  }
  if (route().path === '/activity' && activity) drawActivity(activity);
  activityTimer = setTimeout(activityBadge, route().path === '/activity' || activity?.busy ? 2000 : 30000);
}

function badgeButton() {
  return h('button', { class: 'chip activity', hidden: true, 'aria-label': 'Activity', onclick: () => go('/activity') });
}

async function activityPage(main) {
  main.append(h('div', { class: 'activity-page' }));
  activityBadge();
}

function drawActivity(activity) {
  const page = document.querySelector('.activity-page');
  if (!page) return;
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
  // The heading and Pause are made once and kept: redrawn every two
  // seconds, a button under the pointer would be gone before the click.
  if (!page.firstChild) {
    const said = h('span', { class: 'said' });
    const toggle = h('button', { class: 'primary', hidden: !state.me.admin, onclick: async () => {
      try { await api('activity/' + (page.paused ? 'resume' : 'pause'), { method: 'POST' }); activityBadge(); } catch (error) { said.textContent = error.message; }
    } });
    page.append(h('div', { class: 'head' }, h('div', {}, h('h1', {}, 'Activity'), h('p', { class: 'body' })), toggle), said, h('div', { class: 'list' }));
  }
  page.paused = activity.paused;
  page.querySelector('.head .primary').textContent = activity.paused ? 'Resume' : 'Pause';
  page.querySelector('.head .body').textContent = activity.paused
    ? 'Paused. Nothing new is started; the photographs in hand were finished. Numa Home still shows and sends everything.'
    : 'What Numa Home does by itself, gently, so the NAS serves the photographs first.';
  page.querySelector('.list').replaceChildren(...[
    h('div', { class: 'section-label' }, 'Background'),
    ...activity.jobs.map(row),
    fetching.length > 0 && h('div', { class: 'section-label' }, 'Models'),
    ...fetching.map(download),
  ].filter(Boolean));
}
