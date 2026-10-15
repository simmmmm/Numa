// HOME-011: the timeline's scrubber — a bar on the right, the years and
// months down it as long as the photographs they hold, each library's part
// in its own quiet tint, so a trip is found at a glance. Hover or drag for
// the month and its count; let go and the page is there, its month loaded.
'use strict';

const TINTS = 8;

// Each library's tint, in the order the timeline first meets it (newest
// first), so trips that follow one another differ.
function tintsOf(months) {
  const order = new Map();
  for (const month of months) for (const library of month.libraries || []) if (!order.has(library.id)) order.set(library.id, order.size % TINTS);
  return order;
}
const tint = id => `var(--tint-${state.tints.get(id) ?? 0})`;

function scrubber(months, sections) {
  state.tints = tintsOf(months);
  const total = months.reduce((sum, month) => sum + month.count, 0) || 1;
  const track = h('div', { class: 'scrub-track' });
  const labels = h('div', { class: 'scrub-years' });
  const thumb = h('div', { class: 'scrub-thumb' });
  const hover = h('div', { class: 'scrub-hover', hidden: true });
  const pill = h('div', { class: 'scrub-pill', hidden: true });
  const bar = h('div', { class: 'scrubber', role: 'slider', 'aria-label': 'Timeline', tabindex: '0' }, labels, track, hover, thumb, pill);

  // Where each month starts and ends, 0..1 down the bar: as long as its
  // photographs, but never thinner than FLOOR, so a month of a few — often
  // the oldest — can still be seen, hovered and dragged to.
  const FLOOR = 0.006;
  const shares = months.map(month => Math.max(FLOOR, month.count / total));
  const sum = shares.reduce((a, b) => a + b, 0);
  let at = 0;
  const spans = months.map((month, index) => {
    const span = { month, top: at, bottom: at + shares[index] / sum };
    at = span.bottom;
    return span;
  });
  libraries().catch(() => null);
  for (const span of spans) {
    const libraries = span.month.libraries?.length ? span.month.libraries : [{ id: null, count: span.month.count }];
    track.append(h('div', { class: 'scrub-month', style: { top: span.top * 100 + '%', height: (span.bottom - span.top) * 100 + '%' } },
      libraries.map(library => h('div', { style: { flexGrow: library.count, background: library.id == null ? '' : tint(library.id) } }))));
  }
  // A year's label where its newest month starts, unless too near the last.
  const room = () => track.clientHeight || 1;
  function placeYears() {
    labels.replaceChildren();
    let last = -Infinity;
    for (const span of spans) {
      const year = span.month.month.slice(0, 4);
      if (labels.lastChild?.textContent === year) continue;
      const y = span.top * room();
      if (y - last < 18) continue;
      labels.append(h('div', { class: 'scrub-year', style: { top: y + 'px' } }, year));
      last = y;
    }
  }

  const spanAt = fraction => spans.find(span => fraction < span.bottom) || spans[spans.length - 1];
  const fractionOf = clientY => {
    const box = track.getBoundingClientRect();
    return Math.min(0.99999, Math.max(0, (clientY - box.top) / box.height));
  };
  const headerHeight = () => document.querySelector('header')?.offsetHeight || 0;

  // The library under the pointer: the month's tints stand in its order,
  // each as long as its photographs.
  function libraryAt(span, fraction) {
    const libraries = span.month.libraries || [];
    let into = (fraction - span.top) / Math.max(1e-9, span.bottom - span.top) * span.month.count;
    return libraries.find(library => (into -= library.count) < 0) || libraries[libraries.length - 1];
  }

  function say(fraction) {
    const span = spanAt(fraction);
    const library = libraryAt(span, fraction);
    const known = library && state.libraries?.find(one => one.id === library.id);
    const parts = known
      ? h('div', { class: 'scrub-libraries' }, h('span', {}, h('i', { style: { background: tint(library.id) } }), libraryName(known)))
      : null;
    pill.replaceChildren(h('span', { class: 'section-label' }, counted(span.month.count)), h('span', { class: 'value' }, monthName(span.month.month)), parts);
    pill.hidden = false;
    pill.style.top = fraction * room() + 'px';
  }

  // The page at a point of the bar: the month's section, as far into it.
  function jump(fraction) {
    const span = spanAt(fraction);
    const section = sections[spans.indexOf(span)];
    const into = (fraction - span.top) / Math.max(1e-9, span.bottom - span.top);
    const go = () => scrollTo(0, section.el.getBoundingClientRect().top + scrollY - headerHeight() + into * section.el.offsetHeight);
    go();
    return { section, go };
  }

  // Where the page is, on the bar.
  function follow() {
    if (!bar.isConnected) return removeEventListener('scroll', onScroll);
    if (dragging) return;
    const line = headerHeight() + 1;
    const index = sections.findIndex(section => section.el.getBoundingClientRect().bottom > line);
    if (index < 0) return;
    const box = sections[index].el.getBoundingClientRect();
    const into = Math.min(1, Math.max(0, (line - box.top) / (box.height || 1)));
    const span = spans[index];
    thumb.style.top = (span.top + into * (span.bottom - span.top)) * room() + 'px';
  }
  let ticking = false, quiet;
  function onScroll() {
    if (!ticking) requestAnimationFrame(() => { ticking = false; follow(); });
    ticking = true;
    bar.classList.add('moving');
    clearTimeout(quiet);
    quiet = setTimeout(() => bar.classList.remove('moving'), 1200);
  }
  addEventListener('scroll', onScroll, { passive: true });

  let dragging = false, landed = null;
  bar.addEventListener('pointerdown', event => {
    event.preventDefault();
    try { bar.setPointerCapture(event.pointerId); } catch (_) { /* a synthetic pointer */ }
    dragging = true;
    bar.classList.add('active');
    move(event);
  });
  function move(event) {
    const fraction = fractionOf(event.clientY);
    if (dragging) {
      thumb.style.top = fraction * room() + 'px';
      landed = jump(fraction);
    } else if (event.pointerType === 'mouse') {
      hover.hidden = false;
      hover.style.top = fraction * room() + 'px';
    }
    say(fraction);
  }
  bar.addEventListener('pointermove', move);
  const up = async () => {
    if (!dragging) return;
    dragging = false;
    bar.classList.remove('active');
    pill.hidden = true;
    // Its month and the next two loaded, then there again: what loaded above
    // moved it where the browser does not hold the place (Safari).
    const target = landed;
    if (!target) return;
    const index = sections.indexOf(target.section);
    await Promise.allSettled(sections.slice(Math.max(0, index - 1), index + 2).map(section => section.load()));
    if (!dragging && landed === target) target.go();
  };
  bar.addEventListener('pointerup', up);
  bar.addEventListener('pointercancel', up);
  bar.addEventListener('pointerleave', () => { if (!dragging) { hover.hidden = true; pill.hidden = true; } });
  bar.addEventListener('keydown', event => {
    const by = { PageDown: 1, ArrowDown: 1, PageUp: -1, ArrowUp: -1 }[event.key];
    if (!by) return;
    event.preventDefault();
    const index = sections.findIndex(section => section.el.getBoundingClientRect().bottom > headerHeight() + 2);
    const next = sections[Math.min(sections.length - 1, Math.max(0, index + by))];
    scrollTo(0, next.el.getBoundingClientRect().top + scrollY - headerHeight());
  });

  const place = () => {
    bar.style.top = headerHeight() + 'px';
    placeYears();
    follow();
  };
  new ResizeObserver(place).observe(bar);
  requestAnimationFrame(place);
  return bar;
}
