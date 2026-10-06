# Module manifest

RE-KORD reserves a small extension point for optional modules. It is not used by any
shipping feature yet.

## Status in 5.0

Plectr, Sonic Nebula, Studio and themes are **built into the client** and always
available. The module flags described below do **not** turn them on or off.

The hub reads the manifest, exposes the module registry through the `rekord-plugin-api`
crate (`crates/plugin-api`), lists it at `GET /api/v1/modules` and reports the enabled
ones in `GET /api/v1/health` (`modules`). Nothing else consumes the flags today. The folders under `modules/` hold
placeholder READMEs only.

## Manifest

`modules.manifest.toml`. The hub writes the default to `<data dir>/modules.manifest.toml` on
first start; `--modules-manifest` / `REKORD_MODULES_MANIFEST` point to another file.

```toml
[modules]
plectr = false
web-share = false
nebula = false
studio = false
themes = false
```

## Rules for future modules

1. Do not implement a module without explicit agreement from the maintainers.
2. Enabling a flag alone loads nothing: the module's code and UI must be wired to it.
3. Core features (player, library, favorites, playlists) must never depend on a module.
