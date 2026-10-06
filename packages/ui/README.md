# `@rekord/ui`

Shared RE-KORD UI components (client and hub panel). Use them for every
UI control: no raw markup repeated in the views.

```svelte
<script>
  import { Button, Panel, TextInput } from "@rekord/ui";
</script>

<Panel title="Example">
  <TextInput bind:value={name} />
  <Button onclick={save}>Save</Button>
</Panel>
```

CSS: `@import "@rekord/ui/styles/tokens.css"` (brings in fonts, themes and
`base.css`) and `@import "@rekord/ui/styles/controls.css"`.

## Design system

### Fonts

Inter (variable) and JetBrains Mono are **self-hosted** (`styles/fonts.css`,
`@fontsource*` packages): no network requests at runtime, compatible with
the Tauri CSP `font-src 'self' data:`.

- All text in Inter (`--rk-font`).
- Mono (`--rk-mono`) **only** for file paths and code (LRC, errors).
- Times, counts, scores: Inter with tabular figures — `.rk-num` class or
  `font-variant-numeric: tabular-nums`.

### Type scale (7 steps, nothing below 0.75rem)

| token        | rem    | use                                         |
| ------------ | ------ | ------------------------------------------- |
| `--rk-fs-1`  | 0.75   | eyebrow, badges, captions                   |
| `--rk-fs-2`  | 0.8125 | secondary text, meta, small buttons         |
| `--rk-fs-3`  | 0.9375 | body (`body`)                               |
| `--rk-fs-4`  | 1.0625 | lead, row titles, subtitles                 |
| `--rk-fs-5`  | 1.25   | section/page titles (`--rk-fs-title`)       |
| `--rk-fs-6`  | 1.5    | hero titles, metric values                  |
| `--rk-fs-7`  | 2      | display (splash)                            |

The old names (`--rk-fs-4xs` … `--rk-fs-3xl`) remain as aliases and map to the
nearest step.

### Uppercase

Only `.rk-eyebrow` (the small line above a title). Buttons, tabs, metric
labels, notices: sentence case.

### Status colours

`--rk-success`, `--rk-warning`, `--rk-danger` (+ `-soft` for chips/banners),
`--rk-danger-solid` / `--rk-on-danger` for solid fills. Light themes
darken them on their own. Never hard-coded hex values (`#f59e0b`) in the views.

### Buttons

- `variant="primary" tone="danger"` = **solid red, white text** (destructive
  confirmations). The other `tone="danger"` uses recolour only text and border.
- Disabled: a single recipe for all (flat `surface-3` surface,
  dimmed text). Do not override it with `opacity`.
- `.rk-link` (`<a>` or `<button>`): a link inside text; `.rk-link--quiet`
  for breadcrumbs and artist · album rows. An action is a button, not a link.

### Radii

`--rk-radius-card` (cards, panels), `--rk-radius-control` (buttons, fields),
`--rk-radius-chip` (pills, chips), `--rk-radius-sheet` (floating dock, sheets,
dialogs).

### Focus

A global `:focus-visible` ring (`base.css`) for everything that is
focusable. Do not set `outline: none` without an alternative.

### Effects on WebKitGTK

The client sets `data-rk-lowfx` on `<html>` on Tauri-Linux / WebKitGTK
(`apps/client-ui/src/lib/platformCaps.ts`): no infinite animations, no
`backdrop-filter`. The shared styles respect it (static `Skeleton`).

## Components

### Primitives

`Button`, `TextInput`, `Select`, `IconButton`, `NavButton`, `Banner`, `Panel`,
`Field`, `ActionRow`, `SearchBar`, `BrandMark`, `BrandLogo`, `PageHeader`,
`StatList`, `Modal`, `HeroCard`, `SectionHeader`, `MediaTile`, `QrCodeImg`.

### `CoverArt`

```svelte
<CoverArt kind="track" src={coverUrlOrNull} size="md" />
<CoverArt kind="album" title={album.name} src={album.has_cover ? url : null} size="tile" />
<CoverArt kind="artist" title="Bring Me the Horizon" src={null} size="tile" />  <!-- "BH" -->
<CoverArt kind="genre" srcs={[url1, url2, url3]} size="tile" />                <!-- 1/2/3/4 mosaic -->
```

- `kind`: `track` (♪), `album` (disc, default), `artist` (initials), `genre`
  (adaptive mosaic, never empty cells).
- `src` accepts `null`: pass `null` when the hub says `has_cover: false`
  (no request, no 404). A broken image falls back to the placeholder
  (`onerror`), never to the browser's "broken image" icon.
- `size`: `xs` 32 · `sm` 40 · `md` 48 · `dock` 56 · `tile` 72 · `lg` 120 · `xl`
  fluid. `alt` makes it meaningful (decorative by default).
- `coverInitials(name)` is exported for whoever needs the same initials.

### `EmptyState`

```svelte
<EmptyState title="No favourites" body="Tap the heart on a track to save it here.">
  {#snippet icon()}<UiIcon name="favorite" />{/snippet}
  {#snippet action()}<Button onclick={goLibrary}>Open the library</Button>{/snippet}
</EmptyState>
```

Props: `title`, `body`, `variant="block" | "inline"`, snippets `icon`,
`action`, `children`. `message` alone = the old dimmed line.

### `Skeleton`

```svelte
<Skeleton variant="row" count={6} />        <!-- track rows -->
<Skeleton variant="tile" count={8} />       <!-- library cards -->
<Skeleton variant="metric" count={4} />     <!-- KPIs -->
<Skeleton variant="text" lines={3} />
<Skeleton width="12rem" height="2rem" />    <!-- block -->
```

Show it **in place of** the empty states while the data arrives (no temporary "0" or "No
items"). Static on WebKitGTK and with "reduce motion". Class
`.rk-skeleton` for custom shapes.

### `Tabs`

```svelte
<Tabs items={[{ id: "artists", label: "Artists", count: 20 }, { id: "genres", label: "Genres" }]}
      active={tab} onselect={(id) => (tab = id)} ariaLabel="Sections" size="md" />
```

A single row, scrolls sideways with an edge fade, never wraps. `size`:
`lg` (page title), `md` (below a title), `sm` (inside a panel).
`even` to spread them out. Arrows = focus, Enter/Space/click = selection.
In the client: `SectionNavTabs` wraps it.

### `Segmented`

```svelte
<Segmented ariaLabel="Sort" value={sort} onchange={(v) => (sort = v)}
  options={[{ value: "name", label: "Name" }, { value: "plays", label: "Plays" }]}>
  {#snippet icon(opt)}<UiIcon name={opt.value === "name" ? "sortByAlpha" : "chart"} />{/snippet}
</Segmented>
```

2–5 mutually exclusive choices, one size everywhere. `block` for the full width;
`iconOnly` on an option keeps the text as the accessible name.

### `Metric`

```svelte
<Metric label="Tracks" value={stats.track_count} />
<Metric label="Quality warnings" value={48} tone="warning" hint="12 without a cover" onclick={openQuality} />
<Metric label="Albums" loading />
```

Label in sentence case, large value with tabular figures. `tone`:
`default | accent | success | warning | danger`. `loading` shows a skeleton
instead of a fake 0. `MetricCard` remains as a deprecated alias.

### `FileDrop`

```svelte
<FileDrop accept="image/*" label="Drag an image here" hint="JPG, PNG or WebP"
  buttonLabel="Choose file" fileName={file?.name} onfiles={(f) => (file = f[0])} />
```

Replaces `<input type="file">` (untranslated native button). Drag and
drop or click/keyboard; filters by `accept`; `multiple`, `compact`.

### `Badge` / `.rk-pill`

```svelte
{#if favCount > 0}
  <Badge tone="favorite" title="3 favourites">{#snippet icon()}<UiIcon name="favorite" />{/snippet}3</Badge>
{/if}
```

Status pill: render it **only when it has a value**. `tone`: `neutral | accent |
accent2 | success | warning | danger | favorite`, or a free `color` (mood).
`iconOnly` + `title` for glyphs. In long lists use the classes
`.rk-pill .rk-pill--warning` directly (fewer components per row).

## Themes

Base look **Midnight** (dark, orange/light-blue accents). `data-theme` on the root
selects the named themes (`themes.css`); `data-theme="server"` for the
hub panel.
