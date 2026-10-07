# Optional modules

RE-KORD has a small extension point for optional modules. One shipping module uses it:
**Podcast e notizie** (podcasts, news bulletins and live radio), off by default.

## Podcast e notizie (`podcasts`)

- **Switch**: admin panel › *Podcasts & news* (`PUT /api/v1/podcasts/admin/settings`,
  machine operation), persisted in `settings.json` → `podcasts` (`enabled`,
  `cacheTtlMinutes`). It is not a manifest flag: it is toggled at runtime.
- **Discovery**: `GET /api/v1/health` → `modules` contains `"podcasts"` while it is on;
  `GET /api/v1/modules` lists it with its state.
- **Off costs nothing**: the client endpoints answer 404 `podcasts_disabled`; no task,
  timer or request runs on the hub. Clients show no navigation entry and no card, and
  never download the module's code: its views are lazy chunks, kept out of the service
  worker's precache, loaded only when the hub reports the module.
- **On**: sources are fetched on demand (when a client opens the card or the section),
  cached with a TTL, never polled. Audio is proxied by the hub (Range, SSRF-checked,
  restricted to configured episodes). See [API.md](API.md#podcasts--news-optional-module)
  and the [user guide](user-guide.md#podcasts--news).
- **Code**: hub `crates/core/src/podcasts/`; client `src/lib/externalItems.ts` (how the
  player queues external items), `src/lib/hubModules.svelte.ts` (the check),
  `src/lib/podcasts/`, `src/components/podcasts/`, `src/views/PodcastsView.svelte`; admin
  `src/views/PodcastsPanel.svelte`.

## Manifest modules

### Status in 5.0

Plectr, Sonic Nebula, Studio and themes are **built into the client** and always
available. The module flags described below do **not** turn them on or off.

The hub reads the manifest, exposes the module registry through the `rekord-plugin-api`
crate (`crates/plugin-api`), lists it at `GET /api/v1/modules` and reports the enabled
ones in `GET /api/v1/health` (`modules`). Nothing else consumes the flags today. The folders under `modules/` hold
placeholder READMEs only.

### Manifest

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
