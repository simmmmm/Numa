// HOME-034: Numa Home's first start, gone through once by the admin who made
// the first account, right after making it: the account, the photographs
// found in the photos folder, accounts for the household, and the address
// for the phones. The server keeps the step, so a reload comes back to it;
// Skip on any step, or Done on the last, ends it on Today.
'use strict';

const FIRST_STEPS = ['account', 'photographs', 'household', 'phones'];
const enc = encodeURIComponent;
const capital = text => text.charAt(0).toUpperCase() + text.slice(1);
// The camera's own clock, read as Numa reads it: as if it were UTC.
const monthOf = new Intl.DateTimeFormat(S.locale, { month: 'short', year: 'numeric', timeZone: 'UTC' });
const yearOf = seconds => new Date(seconds * 1000).getUTCFullYear();

function firstStart(step) {
  titled(S.setUp);
  const at = FIRST_STEPS.indexOf(step);
  const steps = h('ol', { class: 'steps', 'aria-label': S.setUp }, FIRST_STEPS.map((one, n) =>
    h('li', { class: n < at ? 'done' : n === at ? 'on' : null, 'aria-current': n === at ? 'step' : null },
      h('span', { class: 'n' }, String(n + 1)), h('span', { class: 'label' }, S[one + 'Step']))));
  const page = h('div', { class: 'first-start' }, h('div', { class: 'top' }, h('img', { class: 'mark', src: '/mark.png', alt: S.numaHome }), steps));
  app.replaceChildren(unreachable, page);
  ({ account: accountStep, photographs: photographsStep, household: householdStep, phones: phonesStep })[step](page);
}

// Back, Skip and the one white button, at the foot of every step.
function stepFoot({ back, skip = true, next, label = S.continue, note }) {
  return h('div', { class: 'foot' },
    back ? h('button', { class: 'chip', type: 'button', onclick: () => moveOn(back) }, S.back) : h('span'),
    h('div', { class: 'go' }, note && h('span', { class: 'caption note' }, note),
      skip && h('button', { class: 'chip', type: 'button', onclick: () => moveOn('done') }, S.skip),
      h('button', { class: 'primary', type: next ? 'button' : 'submit', onclick: next }, label)));
}

async function moveOn(step, out) {
  try { await api('start', { method: 'PUT', body: JSON.stringify({ step, out }) }); } catch (error) { return toast(error.message); }
  state.me.first_start = step === 'done' ? null : step;
  if (state.me.first_start) return firstStart(step);
  state.libraries = null;
  history.replaceState(null, '', '#/today');
  render();
}

// 1. The first account, an admin, as the gate made it before.
function accountStep(page) {
  const name = h('input', { type: 'text', autocomplete: 'username', placeholder: S.name, required: true, autocapitalize: 'words' });
  const password = h('input', { type: 'password', autocomplete: 'new-password', placeholder: S.passwordHint, required: true, minlength: 8 });
  const said = h('p', { class: 'said', role: 'status' });
  page.append(h('form', { class: 'step', onsubmit: async event => {
    event.preventDefault();
    said.textContent = '';
    try {
      await api('setup', { method: 'POST', body: JSON.stringify({ name: name.value.trim(), password: password.value }) });
      await signInAs(name.value.trim(), password.value);
    } catch (error) { said.textContent = error.message; }
  } }, h('h1', {}, S.yourAccount), h('p', { class: 'body' }, S.makeFirst), h('div', { class: 'fields' }, name, password), said,
  stepFoot({ skip: false, note: S.nextPhotographs })));
  name.focus();
}

// 2. Every library found, ticked; the counts and the estimate fill in as
// each library is read, one at a time, as a NAS's disks read best.
async function photographsStep(page) {
  page.append(h('h1', {}, S.yourPhotographs), h('p', { class: 'body' }, S.photographsText));
  let found;
  try { found = await api('start/libraries'); } catch (error) { return page.append(fault(error.message)); }
  const rows = found.map(library => ({ ...library, ticked: !library.out, read: null }));
  const line = h('b');
  const paint = () => {
    const chosen = rows.filter(row => row.ticked);
    const photos = chosen.reduce((sum, row) => sum + (row.read?.photos || 0), 0);
    const seconds = chosen.reduce((sum, row) => sum + (row.read?.seconds || 0), 0);
    const counting = chosen.some(row => !row.read);
    line.textContent = !chosen.length ? S.noneChosen : counting && !photos ? S.countingPhotos
      : capital(phrase(counting ? 'readSoFar' : 'readTime', { time: duration(seconds), photos: counted(photos) }));
  };
  const leave = () => moveOn('household', rows.filter(row => !row.ticked).map(row => row.folder));
  page.append(rows.length ? h('div', { class: 'libs' }, rows.map(row => libraryRow(row, paint))) : h('p', { class: 'body' }, S.noFoldersYet),
    rows.length > 0 && h('div', { class: 'estimate' }, line, h('p', { class: 'caption' }, S.estimateText)),
    stepFoot({ next: leave, note: S.nextHousehold }));
  paint();
  for (const row of rows) {
    if (!page.isConnected) return;
    row.read = await api('start/count?folder=' + enc(row.folder), { quiet: true }).catch(() => ({ photos: 0, seconds: 0, show: [], failed: true }));
    row.fill();
    paint();
  }
}

function libraryRow(row, paint) {
  const box = h('input', { type: 'checkbox', checked: row.ticked, 'aria-label': row.name, onchange: () => { row.ticked = box.checked; facts(); paint(); } });
  const stack = h('div', { class: 'stack' }, h('div', { class: 'print' }), h('div', { class: 'print' }), h('div', { class: 'print' }));
  const caption = h('span', { class: 'caption' });
  const span = h('span', { class: 'caption when' });
  const facts = () => {
    const read = row.read;
    const count = !read ? S.counting : read.failed ? S.notCounted : counted(read.photos);
    caption.textContent = [row.folder !== row.name && row.folder.split('/').join(' / '), row.numa && S.madeInNuma, count,
      read && !read.failed && read.photos > 0 && read.first == null && S.noDates, !row.ticked && S.leftOutForNow].filter(Boolean).join(' · ');
  };
  row.fill = () => {
    facts();
    const { first, last, show } = row.read;
    if (first != null) {
      const [from, to] = [monthOf.format(first * 1000), monthOf.format(last * 1000)];
      span.textContent = from === to ? from : yearOf(first) === yearOf(last) ? String(yearOf(first)) : `${yearOf(first)} – ${yearOf(last)}`;
    }
    // The first taken in front, as on Today.
    show.forEach((file, at) => stack.children[2 - at].append(h('img', { src: `/api/v1/start/thumb?folder=${enc(row.folder)}&file=${enc(file)}`, alt: '', loading: 'lazy' })));
  };
  facts();
  return h('label', { class: 'lib' }, box, stack, h('span', { class: 'what' }, h('span', { class: 'name' }, row.name), caption), span);
}

// 3. Accounts for the others in the house; none is fine.
async function householdStep(page) {
  const name = h('input', { type: 'text', placeholder: S.name, required: true, autocomplete: 'off', autocapitalize: 'words' });
  const password = h('input', { type: 'password', placeholder: S.passwordHint, required: true, minlength: 8, autocomplete: 'new-password' });
  const admin = h('input', { type: 'checkbox' });
  const said = h('p', { class: 'said', role: 'status' });
  const made = h('ul', { class: 'made' });
  const list = async () => {
    const users = await api('users', { quiet: true }).catch(() => []);
    made.replaceChildren(...users.filter(user => user.name !== state.me.name).map(user =>
      h('li', {}, h('span', {}, user.name), user.admin && h('span', { class: 'caption' }, S.admin))));
  };
  page.append(h('h1', {}, S.theHousehold), h('p', { class: 'body' }, S.householdText), made,
    h('form', { class: 'step', onsubmit: async event => {
      event.preventDefault();
      said.textContent = '';
      try {
        await api('users', { method: 'POST', body: JSON.stringify({ name: name.value.trim(), password: password.value, admin: admin.checked }) });
        [name.value, password.value, admin.checked] = ['', '', false];
        await list();
        name.focus();
      } catch (error) { said.textContent = error.message; }
    } }, h('div', { class: 'section-label' }, S.newAccount), h('div', { class: 'fields' }, name, password),
    h('label', { class: 'check' }, admin, S.canMake), h('button', { class: 'chip', type: 'submit' }, S.add), said),
    stepFoot({ back: 'photographs', next: () => moveOn('phones'), note: S.nextPhones }));
  await list();
}

// 4. The address as the browser reached Numa Home, as text and as a QR code.
function phonesStep(page) {
  const address = location.origin;
  page.append(h('h1', {}, S.thePhones), h('p', { class: 'body' }, S.phonesText),
    h('div', { class: 'phone' }, h('img', { class: 'qr', src: '/api/v1/start/qr?address=' + enc(address), alt: phrase('qrOf', { address }) }),
      h('div', { class: 'where' }, h('div', { class: 'section-label' }, S.address), h('div', { class: 'address' }, address), h('p', { class: 'caption' }, S.inTheApp))),
    stepFoot({ back: 'household', skip: false, next: () => moveOn('done'), label: S.done }));
}
