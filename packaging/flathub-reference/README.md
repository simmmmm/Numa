# Flathub reference — written with AI

**Everything in this folder except `cargo-sources.json` was written by Claude
(AI) on 22 September 2026.** Flathub does not accept manifests that are
AI-generated or AI-assisted, disclosed or not
([requirements, Generative AI policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy)).
This is a working reference for how Numa builds the way Flathub builds. It is
not the manifest to submit.

`cargo-sources.json` is tool output: `flatpak-cargo-generator` run on
`Cargo.lock`.

## How it differs from `packaging/flatpak/`

The build has no network.

- **The crates.** They come from `cargo-sources.json`, and cargo runs with
  `--offline`. Regenerate the file whenever `Cargo.lock` changes:
  ```sh
  flatpak run --filesystem=$PWD --command=flatpak-cargo-generator \
      org.flatpak.Builder Cargo.lock -o packaging/flathub-reference/cargo-sources.json
  ```
- **ONNX Runtime.**
  - It is Microsoft's own release archive for 1.28.0, the version `ort`
    2.0.0-rc.13 is built against. `ort` links it as a shared library
    (`ORT_LIB_PATH=/app/lib`, `ORT_PREFER_DYNAMIC_LINK=1`) instead of
    downloading pyke's build.
  - Fotema, a Rust and GTK photo app already on Flathub, ships ONNX Runtime
    the same way (`flathub/app.fotema.Fotema`, `modules/libonnxruntime.json`).
    That is a precedent, not a promise: the rule is "built from source", with
    exceptions at the reviewers' discretion.
- **The GPU plugin.** It is not in Microsoft's archive and stays a download
  from Preferences. Built from ONNX Runtime's source with WebGPU instead, it
  would go in `/app/lib`, where Numa looks first (`numa_infer::gpu_plugin`).
- **The source.** Here it is the working tree. On Flathub it has to be a public
  URL at a fixed commit: `https://github.com/simmmmm/Numa.git`, after
  `dev/publish.sh --push`. The `cargo-sources.json` must then be generated from
  that commit's `Cargo.lock`.

## Checked

- Built with no network: 620 crates as sources, a 52 s release build.
- The binary needs `libonnxruntime.so.1`, which `/app/lib` has.
- Run in the sandbox against a copy of the catalogue:
  - the found masks appear, so the models run on Microsoft's ONNX Runtime;
  - the downloaded WebGPU plugin loads into it and the masks go to the card.

## Build it

```sh
flatpak run org.flatpak.Builder --force-clean --disable-rofiles-fuse \
    --state-dir=target/flathub-ref/state --repo=target/flathub-ref/repo \
    target/flathub-ref/build packaging/flathub-reference/com.tijmen.Numa.yml
```
