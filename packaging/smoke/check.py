import os
import pathlib
import re
import sys

from PIL import Image

name, out, release, table = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3], sys.argv[4]


def read(file):
    path = out / file
    return path.read_text(errors="replace").strip() if path.exists() else ""


def last_error(text):
    lines = [line.strip() for line in text.splitlines() if line.strip() and not line.startswith("+ ")]
    pattern = re.compile(r"error|^E:|failed|not found|no match|cannot|unable|nothing provides|no answer", re.I)
    found = [line for line in lines if pattern.search(line)]
    return (found or lines or ["no output"])[-1][:300]


stage, why, status, log = read("stage"), read("why"), read("exit"), read("self-test.log")
package = read("package")
version = re.search(r"^self-test: Numa (\S+)", log, re.M)
route = re.search(r"^self-test: decoded on: (.+)", log, re.M)
problems = []

if not status:
    problems.append(f"{stage}: {why}")
elif package and package.split()[0].split("-")[0] != release:
    problems.append(f"installed numa {package}, the release is {release}")
elif stage != "start and develop":
    problems.append(f"{stage}: {why or last_error(read('log.txt'))}")
elif not version:
    problems.append(f"did not start: {last_error(log)}")
else:
    if version.group(1) != release:
        problems.append(f"installed {version.group(1)}, the release is {release}")
    problems += [f"panic: {line.strip()[:300]}" for line in log.splitlines() if "panicked at" in line]
    toasts = re.findall(r"^self-test: toast: (.+)", log, re.M)
    for line in re.findall(r"^self-test: (.+: FAILED: .+)", log, re.M):
        problems.append(line + (f" ({toasts[-1]})" if toasts else ""))
    for row in pathlib.Path(table).read_text().splitlines():
        file, _, _, width, height = row.split("\t")
        jpeg = out / f"{pathlib.Path(file).stem} - edited #001.jpg"
        if not jpeg.exists():
            problems.append(f"{file}: no JPEG")
            continue
        try:
            with Image.open(jpeg) as image:
                image.load()
                size, kind = image.size, image.format
        except Exception as err:
            problems.append(f"{file}: the JPEG does not decode: {err}")
            continue
        if kind != "JPEG" or size != (int(width), int(height)):
            problems.append(f"{file}: {kind} {size[0]}×{size[1]}, expected JPEG {width}×{height}")
    if status != "0" and not problems:
        problems.append(f"exited with {status}: {last_error(log)}")

detail = "; ".join(problems) or "installed, started, both raws exported"
mark = "❌" if problems else "✅"
row = f"| {name} | {mark} | {version.group(1) if version else '–'} | {route.group(1) if route else '–'} | {detail} |"
(out / "result.md").write_text(row + "\n")
print(row)
if problems and os.environ.get("GITHUB_ACTIONS"):
    print(f"::error title={name}::{detail}")
sys.exit(1 if problems else 0)
