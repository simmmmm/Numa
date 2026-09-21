#!/usr/bin/env python3
"""Every document about Numa, on one page — the photographer's, 21 September:
"house everything in that artifact, so we have good documentation of all of it".

    python3 dev/docs_site.py

The sources stay where they are and stay the source: `docs/` and the README
in git, the inventory, the plans and the tickets in `fable-tickets/`. This
reads them all and writes one page to `fable-tickets/site/index.html`, with
the inventory's screenshots beside it as JPEGs and the README's pictures
under `files/`. Publish that page to the
artifact named in `docs/AGENT_GUIDE.md` (§5) after every round of docs.

Needs python-markdown and Pillow, which this machine has.
"""
import html
import pathlib
import re
import shutil
import sys

import markdown
from markdown.extensions.toc import slugify as plain_slug
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
TICKETS = ROOT / "fable-tickets"
OUT = TICKETS / "site"

# The shelves of the navigation, in order. A missing file is left out, so a
# checkout without fable-tickets still builds the half it has.
GROUPS = [
    ("Numa", [TICKETS / "overview" / "overview.md", ROOT / "README.md"]),
    ("Reference", [ROOT / "docs" / "FEATURES.md", ROOT / "docs" / "ENGINEERING.md", ROOT / "docs" / "TESTPLAN.md"]),
    ("Working on Numa", [ROOT / "docs" / "AGENT_GUIDE.md", ROOT / "docs" / "SPLIT_PLAN.md",
                         ROOT / "docs" / "CONTROL_AUDIT.md", ROOT / "docs" / "roadmap-edit-screen.md"]),
    ("Plans", [TICKETS / "numa-parity-brief.md", TICKETS / "PANEL_PLAN.md"]),
    ("Tickets", sorted(TICKETS.glob("FT-*.md"), reverse=True)),
]

# FEATURES' status marks and the inventory's, as chips that say it in words.
MARKS = [("✅", "ok", "✓"), ("🟡", "part", "◐"), ("◻️", "plan", "○"), ("❌", "miss", "✕")]
STATUS = [
    ("net gebouwd", "ok"), ("werkt", "ok"), ("gedeeltelijk", "part"), ("in aanbouw", "part"),
    ("uit het paneel", "part"), ("niet begonnen", "miss"), ("ontbreekt", "miss"),
    ("gepland", "plan"), ("geparkeerd", "plan"), ("geschrapt", "gone"),
]


def slug_of(path: pathlib.Path) -> str:
    return re.sub(r"[^a-z0-9]+", "-", path.stem.lower()).strip("-")


def screenshots(source: pathlib.Path) -> None:
    """The inventory's PNGs as JPEGs of at most 1600 px, beside the page."""
    shots = source.parent / "shots"
    if not shots.is_dir():
        return
    (OUT / "shots").mkdir(parents=True, exist_ok=True)
    for png in sorted(shots.glob("*.png")):
        jpg = OUT / "shots" / (png.stem + ".jpg")
        if jpg.exists() and jpg.stat().st_mtime >= png.stat().st_mtime:
            continue
        image = Image.open(png).convert("RGB")
        if image.width > 1600:
            image = image.resize((1600, round(image.height * 1600 / image.width)), Image.LANCZOS)
        image.save(jpg, quality=84, optimize=True, progressive=True)


def render(path: pathlib.Path) -> tuple[str, str, str, list]:
    """One document: its slug, its title, its HTML and its headings."""
    slug = slug_of(path)
    text = path.read_text()
    # The inventory's own contents list gives way to the page's navigation.
    text = re.sub(r"\*\*Inhoud\*\*\n\n(?:\d\..*\n)+", "", text)
    first = re.search(r"^#{1,2} (.+)$", text, re.M)
    title = first.group(1).strip() if first else path.stem
    md = markdown.Markdown(
        extensions=["tables", "toc", "attr_list", "sane_lists", "fenced_code"],
        extension_configs={"toc": {"toc_depth": "2-3", "slugify": lambda value, sep: f"{slug}--" + plain_slug(value, sep)}},
    )
    body = md.convert(text)
    # Anchors inside a document point inside it.
    body = re.sub(r'href="#(?!' + re.escape(slug) + r'--)([^"]+)"', lambda m: f'href="#{slug}--{m.group(1)}"', body)
    body = figures(body)
    body = local_images(body, path)
    body = chips(body)
    body = body.replace("<table>", '<div class="table"><table>').replace("</table>", "</table></div>")
    body = re.sub(r'<a href="(https?://[^"]+)">', r'<a href="\1" target="_blank" rel="noopener">', body)
    headings = [(token["id"], token["name"]) for token in flatten(md.toc_tokens)]
    return slug, title, body, headings


def flatten(tokens):
    for token in tokens:
        yield token
        yield from flatten(token["children"])


def figures(body: str) -> str:
    """An image and the italic caption under it, as a figure."""
    def figure(match):
        alt, src, caption = match.group(1), match.group(2), match.group(3)
        jpg = "shots/" + pathlib.Path(src).stem + ".jpg"
        size = ""
        if (OUT / jpg).exists():
            w, h = Image.open(OUT / jpg).size
            size = f' width="{w}" height="{h}"'
            kind = "wide" if w >= 1200 else ("strip" if w / h > 4 else "narrow")
        else:
            kind = "wide"
        return (f'<figure class="{kind}"><img src="{jpg}" alt="{alt}"{size} loading="lazy">'
                f'<figcaption>{caption}</figcaption></figure>')
    return re.sub(r'<p><img alt="([^"]*)" src="(shots/[^"]+)" />(?:</p>\s*<p>|\s*)<em>(.*?)</em></p>', figure, body, flags=re.S)


def local_images(body: str, path: pathlib.Path) -> str:
    """Any other picture a document shows from the repository — the README's
    screenshots and logo — copied under `files/`, so the page stands alone."""
    def local(match):
        source = (path.parent / match.group(1)).resolve()
        if not source.is_file() or ROOT not in source.parents:
            return match.group(0)
        where = "files/" + source.relative_to(ROOT).as_posix()
        copy = OUT / where
        if not copy.exists() or copy.stat().st_mtime < source.stat().st_mtime:
            copy.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, copy)
        return f'src="{where}"'
    return re.sub(r'src="(?!https?:|data:|shots/)([^"]+)"', local, body)


def chips(body: str) -> str:
    def status_cell(match):
        cell = match.group(1)
        for word, kind in STATUS:
            for lead in (f"<strong>{word}</strong>", word):
                if cell.startswith(lead):
                    return f'<td><span class="chip {kind}">{word}</span>{cell[len(lead):]}</td>'
        return match.group(0)
    body = re.sub(r"<td>(.*?)</td>", status_cell, body, flags=re.S)
    for mark, kind, sign in MARKS:
        body = body.replace(mark, f'<span class="chip {kind}" title="{kind}">{sign}</span>')
    return body


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    screenshots(TICKETS / "overview" / "overview.md")
    nav, sections, index = [], [], []
    for group, paths in GROUPS:
        items = []
        for path in paths:
            if not path.exists():
                continue
            slug, title, body, headings = render(path)
            where = path.relative_to(ROOT)
            items.append(f'<li><a href="#{slug}" data-doc="{slug}">{html.escape(title)}</a></li>')
            hidden = "" if not sections else " hidden"
            sections.append(f'<article id="{slug}" class="doc"{hidden}><p class="source">{where}</p>{body}</article>')
            index.extend({"doc": slug, "id": id_, "title": html.unescape(name), "in": title} for id_, name in headings)
        if items:
            nav.append(f'<p class="group">{group}</p><ul>{"".join(items)}</ul>')
    page = TEMPLATE.replace("{{NAV}}", "".join(nav)).replace("{{DOCS}}", "".join(sections))
    import json
    page = page.replace("{{INDEX}}", json.dumps(index, ensure_ascii=False).replace("</", "<\\/"))
    (OUT / "index.html").write_text(page)
    print(f"{OUT / 'index.html'}: {len(sections)} documents, {len(page) // 1024} kB")


TEMPLATE = r"""<title>Numa-inventaris</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:ital,wght@0,400;0,500;0,600;0,700;1,400&family=Source+Serif+4:ital,opsz,wght@0,8..60,400;0,8..60,600;1,8..60,400&display=swap">
<style>
:root {
  --ground: #f4f6f8; --paper: #ffffff; --ink: #1a1e24; --ink-soft: #454d58; --muted: #6a7482;
  --rule: #dde2e8; --rule-strong: #c6cdd6; --accent: #2a6bcb; --accent-soft: #e6effb;
  --ok: #23734a; --ok-bg: #e3f3ea; --part: #8a5a00; --part-bg: #fbf0d9; --miss: #a4291f; --miss-bg: #fbe5e2;
  --plan: #4b5563; --plan-bg: #eceff3; --gone: #6b6f76; --gone-bg: #eeeeef; --quote: #eef3fa; --code: #f0f3f7;
  --shadow: 0 1px 2px rgba(20, 30, 45, .06), 0 4px 14px rgba(20, 30, 45, .06);
  --sans: "IBM Plex Sans", system-ui, -apple-system, "Segoe UI", sans-serif;
  --serif: "Source Serif 4", Georgia, "Times New Roman", serif;
  --mono: "IBM Plex Mono", ui-monospace, "SFMono-Regular", Menlo, monospace;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --ground: #131619; --paper: #1b1f24; --ink: #e5e9ee; --ink-soft: #c2c9d2; --muted: #93a0ae;
    --rule: #2c323a; --rule-strong: #3a424c; --accent: #7eb1ff; --accent-soft: #1d2a3d;
    --ok: #7fd6a4; --ok-bg: #173226; --part: #f0c46a; --part-bg: #362a12; --miss: #ff9a8f; --miss-bg: #3b1d1a;
    --plan: #b7c0cc; --plan-bg: #262c33; --gone: #a1a6ad; --gone-bg: #25282c; --quote: #1c2531; --code: #20262d;
    --shadow: 0 1px 2px rgba(0, 0, 0, .3), 0 6px 18px rgba(0, 0, 0, .25);
  }
}
:root[data-theme="dark"] {
  --ground: #131619; --paper: #1b1f24; --ink: #e5e9ee; --ink-soft: #c2c9d2; --muted: #93a0ae;
  --rule: #2c323a; --rule-strong: #3a424c; --accent: #7eb1ff; --accent-soft: #1d2a3d;
  --ok: #7fd6a4; --ok-bg: #173226; --part: #f0c46a; --part-bg: #362a12; --miss: #ff9a8f; --miss-bg: #3b1d1a;
  --plan: #b7c0cc; --plan-bg: #262c33; --gone: #a1a6ad; --gone-bg: #25282c; --quote: #1c2531; --code: #20262d;
  --shadow: 0 1px 2px rgba(0, 0, 0, .3), 0 6px 18px rgba(0, 0, 0, .25);
}
* { box-sizing: border-box; }
body { background: var(--ground); color: var(--ink); font-family: var(--serif); font-size: 17px; line-height: 1.6; padding-inline: 16px; padding-block: 0 64px; }
.page { max-width: 1180px; margin-inline: auto; }
header.masthead { border-bottom: 1px solid var(--accent); padding-block: 28px 16px; margin-bottom: 24px; display: flex; flex-wrap: wrap; gap: 12px 24px; align-items: end; justify-content: space-between; }
.eyebrow { font-family: var(--sans); font-size: 12px; font-weight: 600; letter-spacing: .12em; text-transform: uppercase; color: var(--accent); margin: 0 0 6px; }
.masthead h1 { font-family: var(--sans); font-weight: 700; font-size: clamp(26px, 4vw, 36px); line-height: 1.1; margin: 0; }
#search { font: 15px var(--sans); color: var(--ink); background: var(--paper); border: 1px solid var(--rule-strong); border-radius: 8px; padding: 8px 12px; width: min(100%, 320px); }
#search:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.layout { display: grid; grid-template-columns: 250px minmax(0, 1fr); gap: 44px; align-items: start; }
@media (max-width: 900px) { .layout { grid-template-columns: minmax(0, 1fr); gap: 8px; } }
nav.contents { position: sticky; top: calc(env(safe-area-inset-top, 0px) + 16px); max-height: calc(100vh - 32px); overflow-y: auto; font-family: var(--sans); font-size: 13.5px; line-height: 1.4; padding-bottom: 16px; }
@media (max-width: 900px) { nav.contents { position: static; max-height: 42vh; background: var(--paper); border: 1px solid var(--rule); border-radius: 10px; padding: 12px 14px; margin-bottom: 16px; } }
nav.contents .group { font-size: 11.5px; font-weight: 600; letter-spacing: .12em; text-transform: uppercase; color: var(--muted); margin: 16px 0 6px; }
nav.contents .group:first-child { margin-top: 0; }
nav.contents ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
nav.contents a { display: block; color: var(--ink-soft); text-decoration: none; padding: 4px 8px; border-radius: 6px; }
nav.contents a:hover, nav.contents a:focus-visible { background: var(--accent-soft); color: var(--accent); }
nav.contents a[aria-current="page"] { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
#results { display: grid; gap: 2px; }
#results a small { display: block; color: var(--muted); font-size: 12px; }
main { min-width: 0; }
.doc > * { max-width: 76ch; }
.doc > figure.wide, .doc > .table, .doc > pre { max-width: none; }
.source { font-family: var(--mono); font-size: 12.5px; color: var(--muted); margin: 0 0 6px; }
h1, h2, h3, h4 { font-family: var(--sans); text-wrap: balance; scroll-margin-top: 16px; }
.doc h1 { font-size: 30px; line-height: 1.15; margin: 0 0 18px; }
.doc h2 { font-size: 24px; line-height: 1.2; margin: 44px 0 14px; padding-top: 16px; border-top: 1px solid var(--rule-strong); }
.doc h3 { font-size: 19px; margin: 32px 0 10px; }
.doc h4 { font-size: 13px; font-weight: 600; letter-spacing: .1em; text-transform: uppercase; color: var(--accent); margin: 28px 0 8px; }
p { margin: 0 0 14px; }
ul, ol { padding-left: 1.3em; margin: 0 0 16px; }
li { margin-bottom: 6px; }
li::marker { color: var(--muted); }
a { color: var(--accent); text-underline-offset: 2px; }
code { font-family: var(--mono); font-size: .84em; background: var(--code); padding: .08em .35em; border-radius: 4px; overflow-wrap: anywhere; }
pre { background: var(--code); border: 1px solid var(--rule); border-radius: 8px; padding: 12px 14px; overflow-x: auto; font-size: 14px; line-height: 1.45; }
pre code { background: none; padding: 0; overflow-wrap: normal; }
hr { border: 0; border-top: 1px solid var(--rule); margin: 28px 0; }
blockquote { margin: 0 0 20px; padding: 12px 16px; background: var(--quote); border-left: 3px solid var(--accent); border-radius: 0 8px 8px 0; font-family: var(--sans); font-size: 14.5px; line-height: 1.55; color: var(--ink-soft); }
blockquote p:last-child { margin: 0; }
.table { overflow-x: auto; margin: 8px 0 24px; background: var(--paper); border: 1px solid var(--rule); border-radius: 10px; box-shadow: var(--shadow); }
table { border-collapse: collapse; width: 100%; font-family: var(--sans); font-size: 14px; line-height: 1.5; }
th { text-align: left; font-weight: 600; font-size: 12px; letter-spacing: .06em; text-transform: uppercase; color: var(--muted); padding: 10px 14px; border-bottom: 1px solid var(--rule-strong); white-space: nowrap; }
td { padding: 9px 14px; border-bottom: 1px solid var(--rule); vertical-align: top; }
tr:last-child td { border-bottom: 0; }
td:first-child { font-weight: 500; }
th:empty { display: none; }
.chip { display: inline-block; font-family: var(--sans); font-size: 11.5px; font-weight: 600; letter-spacing: .03em; line-height: 1; padding: 4px 7px 5px; border-radius: 999px; margin-right: 6px; white-space: nowrap; vertical-align: 1px; }
.chip.ok { color: var(--ok); background: var(--ok-bg); }
.chip.part { color: var(--part); background: var(--part-bg); }
.chip.miss { color: var(--miss); background: var(--miss-bg); }
.chip.plan { color: var(--plan); background: var(--plan-bg); }
.chip.gone { color: var(--gone); background: var(--gone-bg); text-decoration: line-through; }
figure { margin: 20px 0 28px; }
figure img { display: block; height: auto; max-width: 100%; border-radius: 8px; border: 1px solid var(--rule-strong); box-shadow: var(--shadow); background: #1e1e1e; }
figure.wide img, figure.strip img { width: 100%; }
figure.narrow img { max-height: 620px; width: auto; }
figcaption { font-family: var(--sans); font-size: 13.5px; line-height: 1.5; color: var(--muted); margin-top: 10px; max-width: 76ch; }
</style>

<div class="page">
  <header class="masthead">
    <div><p class="eyebrow">Numa · alle documentatie</p><h1>Numa-inventaris</h1></div>
    <input id="search" type="search" placeholder="Zoek in alle documenten" aria-label="Zoek in alle documenten">
  </header>
  <div class="layout">
    <nav class="contents" aria-label="Documenten">
      <div id="results" hidden></div>
      <div id="shelves">{{NAV}}</div>
    </nav>
    <main>{{DOCS}}</main>
  </div>
</div>
<script id="index" type="application/json">{{INDEX}}</script>
<script>
(function () {
  const docs = [...document.querySelectorAll("article.doc")];
  const links = [...document.querySelectorAll("nav a[data-doc]")];
  const index = JSON.parse(document.getElementById("index").textContent);
  function show() {
    const hash = decodeURIComponent(location.hash.slice(1));
    const slug = hash.split("--")[0] || docs[0].id;
    const doc = docs.find(d => d.id === slug) || docs[0];
    docs.forEach(d => { d.hidden = d !== doc; });
    links.forEach(a => a.dataset.doc === doc.id ? a.setAttribute("aria-current", "page") : a.removeAttribute("aria-current"));
    const target = hash.includes("--") && document.getElementById(hash);
    if (target) target.scrollIntoView(); else window.scrollTo(0, 0);
  }
  window.addEventListener("hashchange", show);
  show();
  const search = document.getElementById("search"), results = document.getElementById("results"), shelves = document.getElementById("shelves");
  search.addEventListener("input", () => {
    const q = search.value.trim().toLowerCase();
    if (q.length < 2) { results.hidden = true; shelves.hidden = false; return; }
    const hits = index.filter(h => h.title.toLowerCase().includes(q) || h.in.toLowerCase().includes(q)).slice(0, 40);
    results.replaceChildren(...hits.map(h => {
      const a = document.createElement("a");
      a.href = "#" + h.id;
      a.textContent = h.title;
      const small = document.createElement("small");
      small.textContent = h.in;
      a.append(small);
      return a;
    }));
    if (!hits.length) results.textContent = "Niets gevonden in de koppen.";
    results.hidden = false; shelves.hidden = true;
  });
})();
</script>
"""

if __name__ == "__main__":
    sys.exit(main())
