'use strict';

const token = location.pathname.split('/')[2] || '';
const main = document.querySelector('main');
const AT_ONCE = 2;

function h(tag, props, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props || {})) {
    if (key.startsWith('on')) el.addEventListener(key.slice(2), value);
    else if (value === true) el.setAttribute(key, '');
    else if (value != null && value !== false) el.setAttribute(key, value);
  }
  el.append(...children.flat().filter(child => child != null && child !== false));
  return el;
}

const size = bytes => bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} kB`;

async function start() {
  const answer = await fetch(`/api/v1/c/${token}`).catch(() => null);
  if (!answer?.ok) {
    document.title = S.guestClosed;
    return main.replaceChildren(h('h1', {}, S.guestClosed));
  }
  const link = await answer.json();
  document.title = link.title;
  let left = link.left;
  const kinds = h('p', { class: 'caption' }, S.guestKinds);
  const room = h('p', { class: 'caption' });
  const paintRoom = () => { room.textContent = left > 0 ? phrase('guestLeft', { n: left }) : S.guestFull; add.disabled = left <= 0; };
  const list = h('ul', { class: 'sent' });
  const by = h('input', { type: 'text', maxlength: 40, autocomplete: 'off', placeholder: S.guestName, 'aria-label': S.guestName });
  const picker = h('input', { type: 'file', multiple: true, hidden: true, accept: 'image/jpeg,image/png,image/webp,image/heic,image/heif,image/tiff,.jpg,.jpeg,.png,.webp,.heic,.heif,.tif,.tiff',
    onchange: () => { const files = [...picker.files]; picker.value = ''; send(files); } });
  const add = h('button', { class: 'primary', onclick: () => picker.click() }, S.addPhotos);
  let sent = 0;
  const tally = h('p', { class: 'caption' });

  async function one(file, state) {
    const query = by.value.trim() ? `?by=${encodeURIComponent(by.value.trim())}` : '';
    try {
      const put = await fetch(`/api/v1/c/${token}/files/${encodeURIComponent(file.name)}${query}`, { method: 'PUT', body: file });
      if (put.ok) {
        const said = await put.json();
        if (!said.duplicate) left = Math.max(0, said.left);
        sent += 1;
        state.textContent = S.guestSent.replace('{n}', '✓');
        return;
      }
      const why = await put.json().catch(() => ({}));
      state.classList.add('bad');
      state.textContent = why.error || S.guestFailed;
    } catch (_) {
      state.classList.add('bad');
      state.textContent = S.guestFailed;
    }
  }

  async function send(files) {
    const rows = files.map(file => {
      const state = h('span', { class: 'state' }, size(file.size));
      list.prepend(h('li', {}, h('span', { class: 'name' }, file.name), state));
      return [file, state];
    });
    add.disabled = true;
    let next = 0;
    await Promise.all(Array.from({ length: AT_ONCE }, async () => {
      while (next < rows.length) { const [file, state] = rows[next++]; state.textContent = '…'; await one(file, state); }
    }));
    tally.textContent = phrase('guestSent', { n: sent });
    paintRoom();
  }

  const until = new Date(link.expires * 1000).toLocaleDateString(S.locale, { day: 'numeric', month: 'long', year: 'numeric' });
  paintRoom();
  main.replaceChildren(h('h1', {}, link.title), h('p', { class: 'body' }, phrase('guestHello', { name: link.by })),
    by, add, picker, room, kinds, tally, list,
    h('p', { class: 'caption' }, phrase('guestOpenUntil', { date: until })), h('p', { class: 'caption' }, phrase('guestPrivate', { name: link.by })));
}
start();
