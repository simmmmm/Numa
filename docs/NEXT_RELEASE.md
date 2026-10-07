# Next release: 0.37.0

What goes into the next general build and is not in `main` or Numa-mac `mac`
yet. Before `dev/publish.sh` each item is merged, checked, and its line struck
here; the version bump comes after that (docs/AGENT_GUIDE.md), and after the
bump `data/THIRD_PARTY_LICENSES.txt` is generated again and `dev/check.sh` run.

0.36.0 went out on 7 October. What went into it is in this file's history
(`git log -p docs/NEXT_RELEASE.md`).

The version is 0.37.0 in `Cargo.toml` and the metainfo — never released, so
no bump for what joins it.

## In

- ~~**Photo info as a panel (UX-010)**~~ — merged 7 October: Linux `main`,
  the guide on numa-site `main`. The
  photographer: the info that `I` opened as a popover is a panel at the
  editor's left, folded open and shut by `I` or the info button, and once
  open it stays open — across photographs, the library and a restart — until
  it is shut.
  - Release note: "The photo info is a panel at the left of the editor: press
    I or the info button beside the photograph's name to fold it open or
    shut. Once open, it stays open until you shut it."
  - The Mac and the iPad's Classic editor follow (Numa-mac `mac` cf1c6ab6,
    the photographer: "Ik wil dat overal natuurlijk"); the iPhone keeps its
    sheet. Swift uncompiled until TestFlight.
- ~~**AgX tone mapping (RENDER-026)**~~ — merged 7 October: Linux `main`,
  the guide on numa-site `main`. The photographer: "Kun je ook AGX tone
  mapping toevoegen", "ook voor 0.37.0". Looks › Tone mapping, Camera | AgX,
  per photograph, for raws.
  - Release note: "Tone mapping on Looks: next to the camera's own curve,
    AgX, which takes very bright colours to white as film does, so lamps,
    neon and sunsets keep their colour."
  - On iPhone, iPad and Mac too, under the camera profile (Numa-mac `mac`
    918c39ea). Swift uncompiled until TestFlight.

## Guide

- The iPhone's mask screenshot (`shots/iphone/editor-mask-chosen`) still shows
  the old toolbar; the screenshot round after 0.35.0 replaces it.
