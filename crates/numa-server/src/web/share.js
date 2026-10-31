// HOME-043: proof galleries, the owner's side — Share as Proofs… on a
// choice or an album, the Proofs page with every link, what each client
// chose, New Link and Revoke, and Whose Proof Is This?… for a copy found
// elsewhere. The client's page is proof.js.
'use strict';

// `what` is `{ ids }` or `{ album, name }`.
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
    showLink(made.link);
  } catch (error) { toast(error.message); }
}

// The link, shown this once: Home keeps only its digest. A browser on plain
// http has no clipboard API, so the field is chosen for a copy by hand too.
function showLink(link) {
  const url = location.origin + link;
  const field = h('input', { type: 'text', readonly: true, value: url, 'aria-label': S.proofLink, onfocus: event => event.target.select() });
  const box = h('dialog', { class: 'ask', onclose: () => { box.remove(); if (route().path === '/proofs') render(); } }, h('form', { method: 'dialog' },
    h('h2', {}, S.linkMade), h('p', { class: 'body' }, S.linkOnce), field,
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
  let list;
  try { list = await api('shares'); } catch (error) { return main.append(empty(S.proofs, error.message, true)); }
  if (!main.isConnected) return;
  const file = h('input', { type: 'file', accept: 'image/*', hidden: true, onchange: () => whose(file) });
  const top = h('div', { class: 'top' }, h('div', {}, h('h1', {}, S.proofs), h('p', { class: 'body' }, S.proofsText)),
    h('div', { class: 'act' }, h('button', { class: 'chip', onclick: () => file.click() }, S.whoseEll), file));
  if (!list.length) return main.append(h('div', { class: 'import-page' }, top), empty(S.noProofs, S.noProofsText));
  main.append(h('div', { class: 'import-page' }, top, list.map(share => {
    const status = share.revoked ? S.revoked : share.open ? phrase('openUntil', { date: dayLong(share.expires) }) : S.ended;
    const act = (label, run) => h('button', { class: 'chip small', onclick: run }, label);
    return h('div', { class: 'shoot' },
      h('div', { class: 'head' }, h('span', { class: 'name' }, share.title),
        h('span', { class: 'caption' }, [counted(share.count), phrase('chosenN', { n: grouped(share.picks.length) }), status, share.watermark].join(' · ')),
        share.open && h('div', { class: 'to' },
          act(S.newLink, async () => {
            if (!await ask(S.newLinkQ, S.newLinkText, S.newLink)) return;
            try { showLink((await api(`shares/${share.id}/link`, { method: 'POST' })).link); } catch (error) { toast(error.message); }
          }),
          act(S.revoke, async () => {
            if (!await ask(phrase('revokeQ', { title: share.title }), S.revokeText, S.revoke)) return;
            try { await api(`shares/${share.id}/revoke`, { method: 'POST' }); render(); } catch (error) { toast(error.message); }
          }))),
      share.picks.length > 0 && h('div', { class: 'grid' }, share.picks.map(id => h('div', { class: 'tile ticked' },
        h('img', { class: 'in', src: thumb(id, 256), loading: 'lazy', alt: '' })))));
  })));
}

// Whose Proof Is This?…: a copy found elsewhere, read for its mark.
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
