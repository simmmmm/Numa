# Numa Home (HOME)

Immich's idea, Numa's eyes: the household's photographs on its own NAS
(target: UGREEN DXP4800 Pro — i3-1315U 6c/8t, 8 GB, UHD 64 EU, x86_64,
UGOS Pro with Docker), in a browser and from the Numa apps. Raws shown as
Numa develops them, edits included; faces (YuNet + SFace) and words
(SigLIP 2) from the models the apps already use. One container, one binary,
SQLite. Immich needs four containers (server, ML, Postgres, Redis).

Working names: **Numa Home** (the server), **Send Home** (the action in the
apps). Every user-visible string with either name lives in one place per
client, so renaming is one edit.

## Decided (14 October)

- A household: accounts from day one (the photographer, Frederique, …).
- Photographs come in three ways: the archive already on the NAS, export from
  Numa, and Send Home from the apps (iPhone: its Photos library).
- Export to Home is a choice per export: **As JPEG** or **Original and Edits**.
- The names stay: **Numa Home** and **Send Home** (the photographer, 14 October).
- And back again (14 October): a library on Home can be imported into Numa
  as a **Numa Home library** — the originals on this computer, edited as any
  other, and kept in step with Home both ways (HOME-008).
- The server is the only writer of the catalogs under `/photos` and `/sent`.
  SQLite over SMB from a PC while the server writes the same file corrupts
  it, so the desktop talks to Home through the API, never through the share.
- No video for now. No paid services: images are built locally (later on
  the public repo's free GitHub runners, ghcr.io).

## The container

| mount | what | who sees it |
|---|---|---|
| `/photos` | the archive, its libraries found as the desktop has them: a folder with a `.numa` inside is a library (its catalog comes along: ratings, edits, names); else Numa's archive shape `<year>/<name>` makes each `<name>` one (`Fotos/2026/Japan`, as Import lays them out); else every folder directly under it. Must be writable for the catalogs. | everyone |
| `/cards` | where UGOS mounts an SD card or a USB drive, bound with `rslave` so a card put in later shows up (HOME-009). Read only | — |
| `/sent` | `/sent/<account>/<YYYY>/<YYYY-MM-DD>/<file>`; `/sent/<account>` is that account's library | that account |
| `/data` | `server.db` (accounts, sessions, what was sent), Numa's own catalog and caches (`XDG_DATA_HOME`, `XDG_CACHE_HOME` point here), `models/` | — |

Port 2283 is Immich's; Home uses **8283**. Plain HTTP on the LAN; outside
the house through the NAS's reverse proxy or Tailscale, not ours to build.
`/dev/dri` may be passed for the iGPU; everything must work without it.

Background, in this order, one job at a time and never more than half the
cores (a NAS serves files first): scan (on start, every 15 minutes),
thumbnails ahead, Analyse (faces, sharpness, bursts — the desktop's Analyse),
words (LIB-028), then the Things Numa Saw worked out from the words (LIB-029,
in memory, never written to the catalog). People are grouped when asked, as
the desktop groups them. Each round is planned before it starts, so the
Activity page (HOME-010) has every job's count; an admin can pause it all. A Send Home puts its one file into
the catalog itself (a scan of one file, which adds and never prunes) and
nudges the rest, so the photographs it brought are done first; a whole walk
of the archive after every file of an iPhone's library would keep the disks
busy for nothing. Models are fetched on
first start into `/data/models` from the same links the Linux app uses, never
baked into the image.

## API v1

JSON unless said. Auth: the `numa_session` cookie (HttpOnly, SameSite=Strict)
for the browser, `Authorization: Bearer <token>` for the apps. Every endpoint
but `setup`/`login`/`status` needs one; 401 otherwise. Errors:
`{"error": "<sentence a person can read>"}`.

| | |
|---|---|
| `GET /api/v1/status` | `{"version", "setup": bool}` — `setup` true while there is no account yet |
| `POST /api/v1/setup` `{name, password}` | the first account, admin; 409 once one exists |
| `POST /api/v1/login` `{name, password, device?}` | `{token, name, admin}` + the cookie |
| `POST /api/v1/logout` | ends this session |
| `GET /api/v1/me` | `{name, admin}` |
| `GET/POST /api/v1/users` | list (every account sees the names, for By; `admin` true only to an admin); make `{name, password, admin}`, admin |
| `GET /api/v1/libraries` | `[{id, name, count, mine}]` — the ones this account sees |
| `POST /api/v1/libraries` `{name, year?}` or `{folder}` | HOME-016/018: a new folder (any account: HOME-018's Keep This Library at Numa Home makes one); `{folder}` admin: `/photos/[<year>/]<name>` made a library, or a folder already under `/photos` (one of `folders`) taken in now. 201 `{id, name, count, mine}`; 409 when the folder exists |
| `GET /api/v1/libraries/folders` | admin: `[{folder}]`, folders under `/photos` that are libraries by their shape but not yet looked at (put there since the last scan) |
| `PUT /api/v1/libraries/<id>` `{name}` | HOME-016: the library's folder renamed in its place (numa-io `rename_library_folder`); admin, archive libraries only — a Sent Home library is named after its account |
| `POST /api/v1/photos/remove` `{ids}` | HOME-017: out of the catalog (`remove_photo`), the files stay; each kept in `server.db` (`removed`: library, path) so no scan brings it back. `{removed}` |
| `POST /api/v1/photos/delete` `{ids}` | HOME-017: each file to `<library>/.numa/trash/<n>/`, its stars, flag, edits, albums and photographer kept in `server.db` (`trash`), out of the catalog. Emptied after each scan of what is 30 days old. `{trashed}` |
| `POST /api/v1/photos/move` `{ids, library, dir?}` | HOME-017: the files into that library, in `dir` inside it (made); within one library the scan follows the file and keeps its row, into another it is given what it carried. `{moved}` |
| `POST /api/v1/photos/album` `{ids, album or name, inside}` | HOME-017: into an album (`name` makes a new one) or out of it (`set_in_album`). `{album}` |
| `POST /api/v1/photos/marks` `{ids, rating?, flag?}` | HOME-017: the same stars or flag for every one (`set_ratings`, `set_flags`). `{updated}` |
| `POST /api/v1/photos/by` `{ids, account}` | HOME-012: the photographer said by hand, `null` to give it back to the camera; kept in `server.db` (`authors`) and first in `by`'s order. 204 |
| `GET /api/v1/photos/zip?ids=1,2,3` | HOME-017: the originals as one stored zip, `<library>/<path>` each |
| `GET /api/v1/trash` | HOME-017: `[{id, library, name, path, at, left}]` of the libraries this account may change, newest first; `left` in days |
| `GET /api/v1/trash/<id>/thumb` | its embedded JPEG |
| `POST /api/v1/trash/restore` `{ids}` | back where each was (a number added if the name is taken), with what it carried. `{restored}` |

Removing, deleting, moving and renaming are an admin's, or an account's in
its own Sent Home library; a shared library's deletes are an admin's. A
JPEG or HIF a raw stands for on the timeline goes with it. Albums, stars,
flags, By and the zip are anyone's who sees the photographs, as a single
photograph's marks always were.
| `GET /api/v1/timeline?library=&person=&album=&q=&thing=&by=` | `[{month: "2026-03", count, libraries: [{id, count}]}]`, newest first; `libraries` is how many of the month's photographs each library holds, most first (HOME-011: the scrubber's tints); `thing` one of `things` (LIB-029); `by` an account's name, or `Unknown` (HOME-012) |
| `GET /api/v1/timeline/<YYYY-MM>?…same filters` | `[{id, taken, aspect, rating, edited}]`, newest first |
| `GET /api/v1/photos/<id>` | `{id, name, taken, camera, lens, exposure, size, people, albums, rating, flag, edited, library, by, chosen}` — `by` the account that took it (HOME-012) or null, `chosen` whether that was said by hand |
| `GET /api/v1/photos/<id>/thumb?edge=256\|512` | JPEG, edits applied, at least that long edge: the server answers from the two sizes Numa keeps, the library's own 320 and the cache's 640, so the archive's own thumbnails are used and nothing is made twice. `ETag`, revalidated |
| `GET /api/v1/photos/<id>/preview` | JPEG, long edge 2048, edits applied |
| `GET /api/v1/photos/<id>/original` | the file as it is |
| `GET /api/v1/photos/<id>/jpeg` | full size JPEG, edits applied (a download) |
| `GET /api/v1/people` | `[{name, count, portrait: <photo id>}]` |
| `GET /api/v1/card` | HOME-009: `null`, or the card in the NAS: `{card, camera, photos, new, first, last, shoots: [{first, last, count, photos, library: <id> or null, folder: <relative to /photos>}], importing: {done, total} or null, done: {said, problems} or null}`. `shoots` holds every photograph on the card, the ones already imported too; `count` is a shoot's new ones, `photos` all of them |
| `GET /api/v1/card/photos` | `{card, shoots: [[{n, name, taken, new, raw}]]}`, in `shoots`' order; `n` names a photograph to `thumb` and `import` |
| `GET /api/v1/card/thumb/<n>` | the camera's embedded JPEG, as Look First reads it (IO-030) |
| `POST /api/v1/card/import` `{shoots: [{library} or {name}, photos?: [n]]}` | one per shoot: into that library, or a new folder of that name beside the suggested one; `photos` the ticked ones (left out: its new ones); 202, then `card` says how far |
| `GET /api/v1/faces` | LIB-014: the faces nobody has named yet, grouped as the People dialog groups them (seen in two photographs or more): `[{faces: [<face id>], photos, portraits: [{face, photo}]}]` |
| `GET /api/v1/faces/<id>/portrait` | the face's own small JPEG, made by Analyse; 404 where there is none (show `photo`'s thumbnail) |
| `POST /api/v1/faces/name` `{faces, name}` / `POST /api/v1/faces/ignore` `{faces}` | name a group, or set it aside as nobody to name |
| `GET /api/v1/albums` | `[{key, name, count}]` |
| `GET /api/v1/by` | HOME-012: `[{account, count}]`, who took what this account sees, most first; `account` null for Unknown, last. A photograph's account: the one that sent it home; else the account its camera is linked to (`cameras`); else the account its camera's EXIF Artist names (accents and case aside, or its first words) |
| `GET /api/v1/cameras` | admin: `[{camera, make, model, serial, count, account, artist, hint}]` — every camera in the archive by the catalog's key (make, model, body serial; LIB-026), most photographs first; `account` its link, `artist` what its EXIF Artist says (read off a few of its photographs after a scan), `hint` the account that Artist names |
| `PUT /api/v1/cameras` `{camera, account}` | admin: link a camera to an account, `account` null to unlink; kept in `server.db`. 204 |
| `GET /api/v1/things` | LIB-029: `[{thing, count}]`, the everyday things Numa saw in what this account sees, most first; empty until the background has looked |
| `GET /api/v1/search?q=&by=` | `[{id, score}]` — LIB-028's rules: names narrow, the rest is SigLIP; `by` as the timeline's |
| `PUT /api/v1/send/<file name>` | **Send Home.** Body: the file's bytes. Headers: `X-Numa-Sha256` (hex, checked; 400 on mismatch), optional `X-Numa-Taken` (unix seconds, for a file without EXIF). Answers `201 {id, duplicate: false}` or, when this account already sent the same bytes, `200 {id, duplicate: true}` without writing. Name sanitised to one path component. |
| `PUT /api/v1/photos/<id>/edits` | body: Numa's edit stack exactly as `photos.edits` holds it (empty body: no edits); the thumbnails follow it. Answers `{updated}` |
| `GET /api/v1/photos/<id>/edits` | `{edits, updated}` — `edits` null when there are none |
| `PUT /api/v1/photos/<id>/marks` `{rating, flag}` | flag as the catalog's `Flag` (`none`/`pick`/`reject`); answers `{updated}` |
| `GET /api/v1/libraries/<id>/photos?since=<unix>` | for keeping a Numa Home library in step: `[{id, path, size, mtime, taken, rating, flag, updated}]`, `path` relative to the library, every photograph whose file, edits or marks changed after `since` (0: all). Answers with `now` in an `X-Numa-Now` header to pass as the next `since` |
| `PUT /api/v1/send/<file name>?library=<id>&dir=<relative dir>` | Send Home into a shared library, at that folder, instead of the account's own; same answers. `dir` checked as a relative path inside the library |
| `GET /api/v1/tidy` | HOME-019: `{counts: {copies, bursts, unusable, who}, cards: [{key, kind, why, keep: [id], photos: [{id, aspect, place, suggested, notes}]}], pending, accounts}` — the decisions this account may take, copies first; `keep` Numa's suggestion (empty for unusable frames); a `who` card also has `title` (the camera) and `ids` (every photograph of it nobody is known to have taken); `pending` the files of a shared size still to be hashed (the night's) |
| `POST /api/v1/tidy/keep` `{key, keep}` | HOME-019: the card's other photographs (and the JPEG each raw stands for) to their library's trash, the card settled in `server.db` and never asked again; `keep` all of it is Keep All; a `who` card is only settled ("Not Sure"). 409 when the card changed since. `{trashed}` |
| `GET /api/v1/activity` | HOME-010: `{paused, busy, jobs: [{job, name, state, doing, library, done, total, left, ago}], downloads: [{name, state, done, total}]}`. `job` is `scan`, `thumbnails`, `analyse` (faces, sharpness, bursts), `words` or `things`, in the order they run; `state` `running`, `waiting` (its turn comes), `waits` (for a model, said in `doing`), `paused` or `idle`; `doing` a sentence to show; `library` the one in hand, named only to an account that sees it; `done`/`total` this round's photographs; `left` seconds by the pace so far, or null; `ago` seconds since it last finished; while a card is imported a row `import` too. `downloads` the models being fetched, in bytes, `state` `waiting`, `fetching`, `done` or `failed`; `night` (HOME-020) the last night's report or null: `{date, started, finished, done: [sentence], needs: [{text, link}], stopped}`, `needs` (what needs a person, `link` a page's hash route) to an admin only; a job `night` while it runs |
| `POST /api/v1/activity/pause` / `…/resume` | admin: all background work paused or resumed — no photograph is started while paused, the ones in hand finish; kept in `server.db` across restarts. Answers `{paused}` |

A retry of Send Home is always safe (same hash → same answer), which is what
an iPhone sending a Photos library on and off Wi-Fi needs.

## Tidy Up (HOME-019)

The photographer, 15 October: "zoek naar dubbele foto's en maak een soort
acties pagina naar foto's die of dusdanig onscherp zijn dat je er niets aan
hebt of je 4-5x dezelfde foto hebt, laat de gebruiker dan kiezen wat het
beste is". A page (`tidy.js`, from the account menu and Libraries) of cards,
each a set side by side, Numa's pick marked; Keep Best, Keep All, or tap the
frames to keep by hand. Only Numa's own measures, no new heuristics:

- **Exact copies**: the same bytes, in one library or across them. Files are
  grouped by size first and only a shared size is hashed (xxh3-128, kept in
  `server.db`), so a raw and the camera's JPEG of it are never compared; a
  JPEG a raw stands for is not offered on its own. Kept: the one with edits,
  then stars, a Pick, in the archive rather than Sent Home, the shortest
  path. By day only hashes already made are used; the night makes the rest.
- **The same frame**: CULL-002's runs (`burst`) of three or more. Kept: eyes
  not closed (C4), then CULL's best of the run (face sharpness where there
  is a face), then the suggested rating (CULL-004/005), then sharpness.
- **Unusable**: never a frame with stars, a Pick or edits, nor one in a run
  above. Blank (`Frame::is_blank`, contrast under 0.01: "nearly black/white");
  blown past `BLOWN_FULL` (40 % of the frame, where the suggested rating
  takes its full penalty); or softer than the library's own `Scale::dull`
  (its 5th percentile of sharpness, the face's where there is one; the
  reference 0.15 under 50 measured frames) — "out of focus", ", slow
  shutter" when C6 says the shutter may be why. Six to a card; Delete All.
- **Who took these?**: photographs nobody is known to have taken (HOME-012),
  a card per camera with the accounts to choose (`photos/by` for all of
  them) or Not Sure. Only with two accounts or more.

Cards are an admin's anywhere, an account's in its own Sent Home (as
deleting is); a settled card's key (`copies:`/`bursts:` library and first
path, `unusable:` per photograph, `who:` camera) stays settled.

## The night (HOME-020)

"Zorg sowieso dat numa home 's nachts als een soort conciërge alles probeert
op te lossen." Once a night between 01:00 and 06:00 in the server's time
zone (`TZ` in compose, tzdata in the image; `NUMA_HOME_NIGHT=<from>-<to>`
moves it, `0-24` forces it for a test), the background does in order:
what failed by day tried again (thumbnails and Analyse that failed are left
alone by day; failed model downloads fetched again); every library read
again in full, every file's date re-read; every file read — whole against
its IO-029 receipt where one lists it, its first megabyte otherwise; the
trash's photographs past 30 days emptied; Tidy Up's copies hashed; `VACUUM`
on `server.db`, Numa's catalog and each library's; the thumbnail cache
pruned to `NUMA_HOME_CACHE_GB` (8). By day only what is needed: the
quarter-hourly scan of changed files and new photographs' work. Pause stops
it between files; resumed, it starts again. Past 06:00 it stops and says so.
The report is kept in `server.db` and the Activity page shows it first, as
Last Night: what was done, and what needs a person — an unreadable or
changed file (to its library), a missing library folder (to Libraries), a
model that would not come, Tidy Up's sets — each a link.

## Open in Numa (HOME-013)

The photographer, 15 October: "open in numa als het kan". The web's
photograph view has an Open in Numa link, and every client that edits —
Linux now, iPhone, iPad and Mac on Numa-mac — handles it the same way:

    numa://home/open?server=<origin>&photo=<id>

- `server` is the page's `location.origin`, percent-encoded
  (`http%3A%2F%2Fnas.local%3A8283`); `photo` the photograph's id, as the API
  writes it. The order of the two does not matter; a link with either missing,
  a `server` that is not `http://` or `https://`, or an id with anything but
  letters, digits, `-` and `_` is malformed.
- Only for the server this Numa is signed in to, compared as the app compares
  addresses (`nas.local` is `http://nas.local:8283`). Not signed in, or signed
  in somewhere else: refused, nothing asked of any server.
- Already here — the id is in the link table of one of that server's Numa
  Home libraries on this computer (HOME-008), and its file is there: opened
  in the editor, nothing fetched.
- Otherwise: `GET /photos/<id>` for its library, that library's listing for
  its path, and then HOME-008's import for that one photograph — the original
  into the library's Numa Home folder (made and linked if it is the first),
  its edits and marks, its row in the link table so edits go back up — and
  opened in the editor. A library brought this way for the first time holds
  that photograph only; the rest stays on the server, as months left
  unticked do.
- Anything else — the server refuses the photograph, cannot be reached, the
  link cannot be read — is one plain sentence where the app says things (a
  toast on Linux), the server's own sentence when it gave one.

Linux registers the scheme in its desktop entry (`MimeType=…;x-scheme-handler/numa;`,
`Exec=numa %U`), the AppImage's own menu entry and the Flatpak included; the
photographs' switch in Preferences leaves the scheme alone.

## Steps

1. **Server** (HOME-001–005): `crates/numa-server`, the Dockerfile and
   `packaging/docker/compose.yaml`, the pages.
2. **Linux** (HOME-006): Preferences → Numa Home (address, sign in; the token
   kept in the catalog's settings, never the password); the export dialog's
   destination with the two choices; progress in the busy toast (UX-015);
   uploads by `curl -T`, as the app already fetches with `curl`.
3. **Numa Home libraries** (HOME-008), Linux: the Import dialog's source
   Numa Home — a library on the server, its months with counts and sizes,
   tick what to bring — downloaded to `~/Pictures/Numa Home/<library>/`
   with the server's folders, edits, ratings and flags, and opened as a
   library marked Numa Home in the picker. Kept in step while open and every
   few minutes: new photographs and changed edits or marks come down, local
   edits, marks and new files go up (Send Home, Original and Edits). Deletes
   never travel either way. When both sides changed one photograph, the
   later `updated` wins.
   **Kept at Home from the Import** (HOME-018, the photographer 15 October):
   Import's switch Keep This Library at Numa Home links the library the
   import went into — the server's library made in its first round
   (`POST /libraries`), so a round cut short makes it in the next — and
   HOME-008's rounds send it up. When: Preferences › Keep in Step, At Night
   (01:00–06:00 local, the default) or While Open, and Only on Unmetered
   Networks (GNetworkMonitor's `network-metered`, on by default). Linux has
   no background service, so only while Numa is open; Sync Now in the
   library's menu goes regardless. The Apple clients can do it in a
   background task instead.
4. **The photograph, handled** (HOME-004, the photographer 15 October:
   "eenvoudige tooling bij de foto, dubbelklik zoomt in etc."): double-click
   or double-tap zooms to 1:1 where it was clicked, the full-size JPEG
   swapped in; wheel and pinch zoom, drag to pan, Escape back. The desktop's
   keys where they mean the same: arrows, 0–5, P, X, U, I for the info.
   Stars and flags through `marks`, the household's as the catalog's are.
   People: name the faces nobody has named yet, here too, as LIB-014 does.
5. **A card in the NAS** (HOME-009), the photographer 15 October: "als ik
   een sd kaart in mijn nas stop dat die herkend wordt net als bij de numa
   desktop applicaties". A card with a `DCIM` under `/cards` puts a bar on
   every page: the camera, how many new photographs, over how many days.
   Import there is the desktop's Import (numa-io `import`, C7): what is
   already in a library stays on the card, the rest split into shoots
   where the camera was put down for two days, each shoot offered the
   library whose dates it falls in or a new `<year>/<YYYY-MM-DD>` folder
   beside the others, with a name to give it ("Japan"). Not year/month/day
   folders: a trip of three weeks would be twenty-one folders in two
   months, and the archive is one folder per trip. Copied the checked way
   (IO-027: hashed, read back), the camera's stars kept (IO-031), the
   receipt written (IO-029), then "Every photograph is on the NAS and
   checked; the card can be ejected" — ejecting is UGOS's.
6. **iPhone, iPad, Mac** (HOME-007): Numa-mac, after 1 and 2 are tried on the
   NAS.
7. Later, when asked: sharing between accounts and links for people outside,
   a map, On This Day, video. Models measured before any is changed: a
   larger SigLIP 2 than the apps' base-224 on the NAS only (it has nights
   and no battery; Immich offers SO400M); a face model better than SFace
   whose licence allows shipping it (Immich's InsightFace buffalo_l does
   not: no redistribution, no commercial use); text in photographs (OCR,
   PP-OCRv5, Apache-2.0).
