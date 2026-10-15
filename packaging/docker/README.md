# Numa Home in Docker

Numa Home is one container: the server, its pages and Numa's renderer. It
keeps three folders:

| in the container | what |
|---|---|
| `/photos` | the archive — its libraries found as Numa has them: a folder with a `.numa` inside, else each trip in `<year>/<trip>`, else each folder in it; everyone in the house sees them. Numa keeps a `.numa` folder inside each (ratings, edits, names, thumbnails), so it must be writable. Numa never moves or changes the photographs. |
| `/sent` | what each person sends home, under `/sent/<name>/<year>/<date>/` |
| `/data` | the accounts, Numa's catalog and thumbnail cache, and the models (about 520 MB, fetched on the first start) |
| `/cards` | where the NAS mounts an SD card or a USB drive (read only, `rslave`): a card with a `DCIM` puts an Import bar on every page, and its photographs go into the archive, checked |

The pages are at `http://<nas>:8283`. The first visit makes the first account.

## Build the image

From the repository's root, with BuildKit (`docker buildx`):

```sh
docker build -f packaging/docker/Dockerfile --build-arg JOBS=4 -t numa-home .
docker save numa-home:latest -o numa-home.tar
```

`JOBS` is how many compilers run at once; each wants a gigabyte or two.

## On a UGREEN NAS (UGOS Pro)

1. Copy `numa-home.tar` to the NAS, for instance into a shared folder.
2. Docker → Image → Add Image → Import from local, and choose the file.
3. Make the folders: your photographs' shared folder (say `/volume1/Photos`)
   and `/volume1/docker/numa-home/sent` and `.../data`.
4. Docker → Project → Create, paste `compose.yaml` and change the folders
   on the left of each `:` to yours (and the cards' `source` to where UGOS
   mounts a card). Deploy.
5. Open `http://<nas>:8283` and make the first account.

Or without a project: Docker → Container → Create from `numa-home:latest`,
port 8283 to 8283, the three folders as above, restart policy
"unless stopped".

The Intel graphics can be handed to it by uncommenting `devices` in
`compose.yaml`; everything works without it, on the processor.

The background work (scanning, thumbnails, faces, words) takes at most half
the processor at a low priority. By night, from 01:00 to 06:00 in the
container's time zone, it tidies up: it tries again what failed, reads every
library again, checks that every file still reads (against its import
receipt where there is one), empties the trash's photographs older than 30
days, prepares Tidy Up, compacts the catalogs and keeps the thumbnail cache
within 8 GB. Set `TZ` in `compose.yaml` to your time zone
(`Europe/Amsterdam`, `America/New_York`); without it the night is UTC's.
`NUMA_HOME_NIGHT=2-5` moves the hours, `NUMA_HOME_CACHE_GB` the cache's
size. The Activity page says each morning what was done and what needs you. Outside the house, use the NAS's own reverse
proxy or Tailscale; Numa Home speaks plain HTTP.
