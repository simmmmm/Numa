'use strict';

async function whatsNew() {
  if (state.me.first_start || state.me.must_change || document.querySelector('dialog[open]')) return;
  let news;
  try { news = await api('whats-new'); } catch (_) { return; }
  if (!news.show || document.querySelector('dialog[open]')) return;
  const box = h('dialog', { class: 'ask', onclose: () => box.remove() }, h('form', { method: 'dialog' },
    h('h2', {}, phrase('newIn', { version: news.version })),
    h('ul', { class: 'lines' }, news.lines.map(line => h('li', {}, line))),
    h('div', { class: 'buttons' },
      h('button', { class: 'chip', type: 'submit', onclick: () => window.open(news.more, '_blank', 'noopener') }, S.readMore),
      h('button', { class: 'primary', type: 'submit', autofocus: true }, S.continue))));
  document.body.append(box);
  box.showModal();
}
