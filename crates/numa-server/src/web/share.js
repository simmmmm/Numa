'use strict';

async function shareDialog(what) {
  const days = h('select', { name: 'days', 'aria-label': S.openFor },
    [[7, S.openWeek], [30, S.openMonth], [90, S.openThreeMonths], [365, S.openYear]].map(([value, label]) => h('option', { value, selected: value === 30 }, label)));
  const asked = await dialog(S.shareProofs, S.shareText, [
    h('input', { type: 'text', name: 'title', placeholder: S.shareTitle, value: what.name || '', maxlength: 80, autocomplete: 'off', 'aria-label': S.shareTitle }),
    h('input', { type: 'text', name: 'watermark', placeholder: S.watermarkText, value: `© ${state.me.name}`, maxlength: 60, autocomplete: 'off', 'aria-label': S.watermarkText }),
    days,
  ], S.makeLink);
  if (!asked) return;
  try {
    const made = await api('shares', { method: 'POST', body: JSON.stringify({
      ...(what.album ? { album: what.album } : { ids: what.ids }), title: asked.title, watermark: asked.watermark, days: Number(asked.days) }) });
    showLink(made.link, made.base);
  } catch (error) { toast(error.message); }
}

function showLink(link, base, text = S.linkOnce) {
  const url = (base || location.origin) + link;
  const field = h('input', { type: 'text', readonly: true, value: url, 'aria-label': S.proofLink, onfocus: event => event.target.select() });
  const box = h('dialog', { class: 'ask', onclose: () => { box.remove(); if (route().path === '/proofs') render(); } }, h('form', { method: 'dialog' },
    h('h2', {}, S.linkMade), h('p', { class: 'body' }, text), field,
    h('div', { class: 'buttons' },
      h('button', { class: 'chip', type: 'submit' }, S.done),
      h('button', { class: 'primary', type: 'button', onclick: async () => {
        field.select();
        try { await navigator.clipboard.writeText(url); } catch (_) { document.execCommand('copy'); }
        toast(S.linkCopied);
      } }, S.copyLink))));
  document.body.append(box);
  box.showModal();
  field.select();
}

const dayLong = seconds => new Date(seconds * 1000).toLocaleDateString(S.locale, { day: 'numeric', month: 'long', year: 'numeric' });

async function proofsPage(main) {
  let list, collects;
  try { [list, collects] = await Promise.all([api('shares'), api('collects')]); } catch (error) { return main.append(empty(S.proofs, error.message, true)); }
  if (!main.isConnected) return;
  const file = h('input', { type: 'file', accept: 'image/*', hidden: true, onchange: () => whose(file) });
  const top = h('div', { class: 'top' }, h('div', {}, h('h1', {}, S.proofs), h('p', { class: 'body' }, S.proofsText)),
    h('div', { class: 'act' }, h('button', { class: 'chip', onclick: () => collectDialog({}) }, S.collectEll), h('button', { class: 'chip', onclick: () => file.click() }, S.whoseEll), file));
  if (!list.length && !collects.length) return main.append(h('div', { class: 'import-page' }, top), empty(S.noProofs, S.noProofsText));
  const act = (label, run) => h('button', { class: 'chip small', onclick: run }, label);
  const statusOf = share => share.revoked ? S.revoked : share.open ? phrase('openUntil', { date: dayLong(share.expires) }) : S.ended;
  const links = share => share.open && [
    act(S.newLink, async () => {
      if (!await ask(S.newLinkQ, S.newLinkText, S.newLink)) return;
      try { const made = await api(`shares/${share.id}/link`, { method: 'POST' }); showLink(made.link, made.base, share.kind === 'collect' ? S.collectLinkOnce : S.linkOnce); } catch (error) { toast(error.message); }
    }),
    act(S.revoke, async () => {
      if (!await ask(phrase('revokeQ', { title: share.title }), S.revokeText, S.revoke)) return;
      try { await api(`shares/${share.id}/revoke`, { method: 'POST' }); render(); } catch (error) { toast(error.message); }
    })];
  main.append(h('div', { class: 'import-page' }, top, list.map(share => h('div', { class: 'shoot' },
    h('div', { class: 'head' }, h('span', { class: 'name' }, share.title),
      h('span', { class: 'caption' }, [counted(share.count), phrase('chosenN', { n: grouped(share.picks.length) }), statusOf(share), share.watermark].join(' · ')),
      h('div', { class: 'to' }, links(share))),
    share.picks.length > 0 && h('div', { class: 'grid' }, share.picks.map(id => h('div', { class: 'tile ticked' },
      h('img', { class: 'in', src: thumb(id, 256), loading: 'lazy', alt: '' })))))),
    collects.length > 0 && h('h2', { class: 'section-label' }, S.guestUploads),
    collects.map(link => h('div', { class: 'shoot' },
      h('div', { class: 'head' }, h('span', { class: 'name' }, link.title),
        h('span', { class: 'caption' }, [phrase('waitingN', { n: grouped(link.waiting) }), phrase('acceptedN', { n: grouped(link.accepted) }), statusOf(link)].join(' · ')),
        h('div', { class: 'to' }, link.waiting > 0 && act(S.review, () => reviewDialog(link)), links({ ...link, kind: 'collect' })))))));
}

async function whose(input) {
  const chosen = input.files[0];
  input.value = '';
  if (!chosen) return;
  try {
    const found = await api('shares/whose', { method: 'POST', body: chosen, headers: { 'Content-Type': chosen.type || 'application/octet-stream' } });
    await ask(phrase('whoseFound', { title: found.title }),
      phrase('whoseText', { date: dayLong(found.made), watermark: found.watermark, state: found.revoked ? S.revoked : found.open ? phrase('openUntil', { date: dayLong(found.expires) }) : S.ended }),
      S.done);
  } catch (error) { toast(error.message); }
}

async function collectDialog(what) {
  const count = h('select', { name: 'count', 'aria-label': S.collectUpTo }, [50, 200, 500, 1000].map(n => h('option', { value: n, selected: n === 200 }, phrase('upToN', { n }))));
  const days = h('select', { name: 'days', 'aria-label': S.collectOpenFor }, [1, 3, 7, 14, 30].map(n => h('option', { value: n, selected: n === 7 }, phrase('collectDays', { n }))));
  const asked = await dialog(S.collect, S.collectText, [
    h('input', { type: 'text', name: 'title', placeholder: S.shareTitle, value: what.name || '', maxlength: 80, autocomplete: 'off', 'aria-label': S.shareTitle }),
    h('input', { type: 'text', name: 'by', placeholder: S.guestByName, value: state.me.name, maxlength: 40, autocomplete: 'off', 'aria-label': S.guestByName }),
    count, days,
  ], S.makeLink);
  if (!asked) return;
  try {
    const made = await api('collects', { method: 'POST', body: JSON.stringify({ title: asked.title, by: asked.by, count: Number(asked.count), days: Number(asked.days), album: what.album }) });
    showLink(made.link, made.base, S.collectLinkOnce);
  } catch (error) { toast(error.message); }
}

async function reviewDialog(link) {
  let waiting;
  try { waiting = await api(`collects/${link.id}/files`); } catch (error) { return toast(error.message); }
  const chosen = new Set();
  const grid = h('div', { class: 'grid review' });
  const accept = h('button', { class: 'primary', type: 'button' }, S.acceptChosen);
  const reject = h('button', { class: 'chip', type: 'button' }, S.rejectChosen);
  const paint = () => { accept.disabled = reject.disabled = chosen.size === 0; };
  const tile = file => {
    const box = h('button', { class: 'tile', type: 'button', 'aria-pressed': 'false', title: [file.name, file.guest && phrase('fromGuest', { name: file.guest })].filter(Boolean).join(' · '),
      onclick: () => { const on = !chosen.has(file.id); on ? chosen.add(file.id) : chosen.delete(file.id); box.classList.toggle('ticked', on); box.setAttribute('aria-pressed', String(on)); paint(); } },
      h('img', { class: 'in', src: `/api/v1/collects/${link.id}/files/${file.id}/thumb`, loading: 'lazy', alt: file.name }));
    return box;
  };
  waiting.forEach(file => grid.append(tile(file)));
  paint();
  const decide = async (what, said) => {
    const files = [...chosen];
    try {
      const done = await api(`collects/${link.id}/${what}`, { method: 'POST', body: JSON.stringify({ files }) });
      toast(phrase(said, { n: grouped(done.accepted ?? done.rejected) }));
      box.close();
      render();
    } catch (error) { toast(error.message); }
  };
  accept.addEventListener('click', () => decide('accept', 'acceptedPhotos'));
  reject.addEventListener('click', () => decide('reject', 'rejectedPhotos'));
  const all = h('button', { class: 'chip', type: 'button', onclick: () => { grid.querySelectorAll('.tile').forEach(t => { if (!t.classList.contains('ticked')) t.click(); }); } }, S.chooseAll);
  const box = h('dialog', { class: 'ask review', onclose: () => box.remove() }, h('form', { method: 'dialog' },
    h('h2', {}, link.title), h('p', { class: 'body' }, S.acceptText.replace('{folder}', link.folder)), grid,
    h('div', { class: 'buttons' }, all, reject, accept)));
  document.body.append(box);
  box.showModal();
}

async function shareAlbumDialog(album) {
  let given;
  try { given = await api(`albums/${encodeURIComponent(album.key)}/people`); } catch (error) { return toast(error.message); }
  const level = new Map(given.people.map(one => [one.account.toLowerCase(), one.level]));
  const rows = given.candidates.map(name => h('div', { class: 'grant' }, h('span', {}, name),
    h('select', { name: name, 'aria-label': name }, [['', S.notShared], ['view', S.mayLook], ['add', S.mayAdd]].map(([value, label]) =>
      h('option', { value, selected: (level.get(name.toLowerCase()) || '') === value }, label)))));
  const asked = await dialog(S.shareAlbum, rows.length ? S.shareAlbumText : S.noPeopleToShare, [rows.length > 0 && h('div', { class: 'grants' }, rows)].filter(Boolean), S.done);
  if (!asked || !rows.length) return;
  try {
    await api(`albums/${encodeURIComponent(album.key)}/people`, { method: 'PUT', body: JSON.stringify({ people: Object.fromEntries(Object.entries(asked).filter(([, value]) => value)) }) });
    navLists.album = null;
    toast(S.albumShared);
  } catch (error) { toast(error.message); }
}
