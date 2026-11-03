'use strict';

const reachTime = at => new Date(at * 1000).toLocaleString(S.locale, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });

function signInRows(rows) {
  const result = { ok: S.signInOk, wrong: S.signInWrong, locked: S.signInLocked };
  return rows.map(one => h('li', {}, h('div', {}, h('div', { class: 'name' }, `${one.name || '–'} · ${result[one.result] || one.result}`),
    h('div', { class: 'caption' }, `${reachTime(one.at)} · ${one.address} · ${one.outside ? S.signInFrom : S.signInHere}`)),
  h('i', { class: one.result === 'ok' ? 'signal ok' : 'signal' })));
}

async function reachPage(main) {
  const page = h('div', { class: 'safe' }, h('p', { class: 'body' }, S.loading));
  main.append(page);
  const reach = await api('reach').catch(error => (page.replaceChildren(fault(error.message)), null));
  if (!reach) return;
  const tries = (await api('reach/signins').catch(() => [])).filter(one => one.outside && one.result !== 'ok').slice(0, 10);
  const address = h('input', { type: 'text', inputmode: 'url', placeholder: S.reachAddressHint, 'aria-label': S.reachAddress, value: reach.address || '', autocomplete: 'off', spellcheck: false });
  const confirm = h('input', { type: 'checkbox' });
  const save = h('button', { class: 'chip', type: 'submit' }, S.save);
  const send = async (mode, sure) => {
    try {
      await api('reach', { method: 'PUT', body: JSON.stringify({ address: address.value, mode, confirm: sure }) });
      toast(S.reachSaved);
      render();
    } catch (error) { toast(error.message); }
  };
  const warning = h('div', { class: 'warn' }, h('p', { class: 'body' }, S.reachAllWarning),
    h('p', { class: 'caption' }, reach.seen ? phrase('reachAllChecked', { ago: ago(reach.seen) }) : S.reachAllNeedsCheck),
    reach.mode !== 'all' && h('label', { class: 'check' }, confirm, h('span', {}, S.reachAllConfirm)));
  const choices = [['off', S.reachModeOff, S.reachModeOffText], ['links', S.reachModeLinks, S.reachModeLinksText], ['all', S.reachModeAll, S.reachModeAllText]];
  const radios = choices.map(([mode, title, text]) => {
    const input = h('input', { type: 'radio', name: 'reach-mode', value: mode, checked: reach.mode === mode });
    return h('label', { class: 'check way-in' }, input, h('span', {}, h('span', { class: 'name' }, title), h('span', { class: 'caption' }, text)));
  });
  const chosen = () => radios.find(label => label.querySelector('input').checked).querySelector('input').value;
  const sync = () => {
    const all = chosen() === 'all';
    warning.hidden = !all;
    save.disabled = all && reach.mode !== 'all' && !(confirm.checked && reach.seen);
  };
  radios.forEach(label => label.addEventListener('change', sync));
  confirm.addEventListener('change', sync);
  sync();
  const row = (ok, text) => h('li', {}, h('span', {}, text), h('i', { class: ok ? 'signal ok' : 'signal' }));
  const status = [
    row(reach.mode !== 'off', { off: S.reachClosed, links: S.reachOpen, all: S.reachOpenAll }[reach.mode]),
    row(Boolean(reach.address), reach.address ? phrase('reachAddressIs', { address: reach.address }) : S.reachNoAddress),
    row(reach.proxy && !reach.unnamed, reach.unnamed ? phrase('reachUnnamed', { proxy: reach.unnamed }) : reach.proxy ? S.reachProxyNamed : S.reachNoProxy),
    row(Boolean(reach.seen), reach.seen ? phrase('reachSeen', { ago: ago(reach.seen) }) : S.reachNotSeen),
  ];
  const way = (title, text) => h('li', {}, h('div', {}, h('div', { class: 'name' }, title), h('div', { class: 'caption' }, text)));
  page.replaceChildren(
    h('p', { class: 'body role' }, S.reachText),
    h('div', { class: 'boxes' },
      h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.reachNow), h('ul', {}, status),
        reach.mode === 'all' && h('button', { class: 'chip', onclick: () => send('links', false) }, S.reachCloseNow)),
      h('form', { class: 'box', onsubmit: event => { event.preventDefault(); send(chosen(), confirm.checked); } }, h('div', { class: 'section-label' }, S.reachAddress), address,
        ...radios, warning, save),
      h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.reachCheck),
        h('p', { class: 'body' }, S.reachCheckText),
        reach.address && h('a', { class: 'name', href: reach.address + '/api/v1/reach/ping', target: '_blank', rel: 'noopener' }, reach.address + '/api/v1/reach/ping'),
        h('button', { class: 'chip', onclick: () => render() }, S.reachCheckAgain)),
      h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.reachTries), h('p', { class: 'caption' }, S.reachTriesText),
        tries.length ? h('ul', {}, signInRows(tries)) : h('p', { class: 'body' }, S.reachNoTries), h('p', { class: 'caption' }, S.reachSessions)),
      h('section', { class: 'box' }, h('div', { class: 'section-label' }, S.reachWays), h('ul', {},
        way(S.reachProxyTitle, S.reachProxyText), way(S.reachTailscaleTitle, S.reachTailscaleText),
        way(S.reachTunnelTitle, S.reachTunnelText), way(S.reachWireguardTitle, S.reachWireguardText)))));
}

async function signInsBox(page) {
  const rows = await api('reach/signins').catch(() => null);
  if (!rows || rows.length === 0) return;
  page.append(h('div', { class: 'safe' }, h('div', { class: 'section-label' }, S.signIns), h('ul', {}, signInRows(rows.slice(0, 20)))));
}
