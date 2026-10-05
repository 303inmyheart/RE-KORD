# `@rekord/ui`

Componenti grafici condivisi RE-KORD (client e pannello hub). Usarli per ogni
controllo UI: niente markup grezzo ripetuto nelle view.

```svelte
<script>
  import { Button, Panel, TextInput } from "@rekord/ui";
</script>

<Panel title="Esempio">
  <TextInput bind:value={name} />
  <Button onclick={save}>Salva</Button>
</Panel>
```

CSS: `@import "@rekord/ui/styles/tokens.css"` (porta con sé font, temi e
`base.css`) e `@import "@rekord/ui/styles/controls.css"`.

## Design system

### Font

Inter (variabile) e JetBrains Mono sono **self-hosted** (`styles/fonts.css`,
pacchetti `@fontsource*`): nessuna richiesta di rete a runtime, compatibile con
la CSP Tauri `font-src 'self' data:`.

- Tutto il testo in Inter (`--rk-font`).
- Mono (`--rk-mono`) **solo** per percorsi file e codice (LRC, errori).
- Tempi, conteggi, punteggi: Inter con cifre tabulari — classe `.rk-num` o
  `font-variant-numeric: tabular-nums`.

### Scala tipografica (7 gradini, niente sotto 0.75rem)

| token        | rem    | uso                                         |
| ------------ | ------ | ------------------------------------------- |
| `--rk-fs-1`  | 0.75   | eyebrow, badge, didascalie                  |
| `--rk-fs-2`  | 0.8125 | testo secondario, meta, bottoni piccoli     |
| `--rk-fs-3`  | 0.9375 | corpo (`body`)                              |
| `--rk-fs-4`  | 1.0625 | lead, titoli di riga, sottotitoli           |
| `--rk-fs-5`  | 1.25   | titoli di sezione/pagina (`--rk-fs-title`)  |
| `--rk-fs-6`  | 1.5    | titoli hero, valori metrica                 |
| `--rk-fs-7`  | 2      | display (splash)                            |

I vecchi nomi (`--rk-fs-4xs` … `--rk-fs-3xl`) restano come alias e cadono sul
gradino più vicino.

### Maiuscolo

Solo `.rk-eyebrow` (la riga piccola sopra un titolo). Bottoni, tab, etichette
di metrica, avvisi: frase normale.

### Colori di stato

`--rk-success`, `--rk-warning`, `--rk-danger` (+ `-soft` per chip/banner),
`--rk-danger-solid` / `--rk-on-danger` per i riempimenti pieni. I temi chiari
li scuriscono da soli. Mai esadecimali fissi (`#f59e0b`) nelle view.

### Bottoni

- `variant="primary" tone="danger"` = **rosso pieno, testo bianco** (conferme
  distruttive). Il resto dei `tone="danger"` ricolora solo testo e bordo.
- Disabilitato: una sola ricetta per tutti (superficie piatta `surface-3`,
  testo attenuato). Non sovrascriverla con `opacity`.
- `.rk-link` (`<a>` o `<button>`): link dentro un testo; `.rk-link--quiet`
  per briciole e righe artista · album. Un'azione è un bottone, non un link.

### Raggi

`--rk-radius-card` (schede, pannelli), `--rk-radius-control` (bottoni, campi),
`--rk-radius-chip` (pill, chip), `--rk-radius-sheet` (dock flottante, fogli,
dialoghi).

### Focus

Un anello globale `:focus-visible` (`base.css`) per tutto ciò che è
focalizzabile. Non mettere `outline: none` senza un'alternativa.

### Effetti su WebKitGTK

Il client imposta `data-rk-lowfx` su `<html>` su Tauri-Linux / WebKitGTK
(`apps/client-ui/src/lib/platformCaps.ts`): niente animazioni infinite, niente
`backdrop-filter`. Gli stili condivisi lo rispettano (`Skeleton` statico).

## Componenti

### Primitivi

`Button`, `TextInput`, `Select`, `IconButton`, `NavButton`, `Banner`, `Panel`,
`Field`, `ActionRow`, `SearchBar`, `BrandMark`, `BrandLogo`, `PageHeader`,
`StatList`, `Modal`, `HeroCard`, `SectionHeader`, `MediaTile`, `QrCodeImg`.

### `CoverArt`

```svelte
<CoverArt kind="track" src={coverUrlOrNull} size="md" />
<CoverArt kind="album" title={album.name} src={album.has_cover ? url : null} size="tile" />
<CoverArt kind="artist" title="Bring Me the Horizon" src={null} size="tile" />  <!-- "BH" -->
<CoverArt kind="genre" srcs={[url1, url2, url3]} size="tile" />                <!-- mosaico 1/2/3/4 -->
```

- `kind`: `track` (♪), `album` (disco, default), `artist` (iniziali), `genre`
  (mosaico adattivo, mai celle vuote).
- `src` accetta `null`: passare `null` quando l'hub dice `has_cover: false`
  (nessuna richiesta, nessun 404). Un'immagine rotta ricade sul segnaposto
  (`onerror`), mai sull'icona "immagine rotta" del browser.
- `size`: `xs` 32 · `sm` 40 · `md` 48 · `dock` 56 · `tile` 72 · `lg` 120 · `xl`
  fluido. `alt` la rende significativa (default decorativa).
- `coverInitials(name)` esportata per chi serve le stesse iniziali.

### `EmptyState`

```svelte
<EmptyState title="Nessun preferito" body="Tocca il cuore su un brano per salvarlo qui.">
  {#snippet icon()}<UiIcon name="favorite" />{/snippet}
  {#snippet action()}<Button onclick={goLibrary}>Apri la libreria</Button>{/snippet}
</EmptyState>
```

Props: `title`, `body`, `variant="block" | "inline"`, snippet `icon`,
`action`, `children`. Solo `message` = vecchia riga attenuata.

### `Skeleton`

```svelte
<Skeleton variant="row" count={6} />        <!-- righe brano -->
<Skeleton variant="tile" count={8} />       <!-- card libreria -->
<Skeleton variant="metric" count={4} />     <!-- KPI -->
<Skeleton variant="text" lines={3} />
<Skeleton width="12rem" height="2rem" />    <!-- blocco -->
```

Mostrarlo **al posto** dei vuoti mentre i dati arrivano (niente "0" o "Nessun
elemento" provvisori). Statico su WebKitGTK e con "riduci movimento". Classe
`.rk-skeleton` per forme proprie.

### `Tabs`

```svelte
<Tabs items={[{ id: "artists", label: "Artisti", count: 20 }, { id: "genres", label: "Generi" }]}
      active={tab} onselect={(id) => (tab = id)} ariaLabel="Sezioni" size="md" />
```

Una riga sola, scorre di lato con dissolvenza ai bordi, mai a capo. `size`:
`lg` (titolo pagina), `md` (sotto un titolo), `sm` (dentro un pannello).
`even` per distribuirli. Frecce = focus, Invio/Spazio/click = selezione.
Nel client: `SectionNavTabs` la avvolge.

### `Segmented`

```svelte
<Segmented ariaLabel="Ordina" value={sort} onchange={(v) => (sort = v)}
  options={[{ value: "name", label: "Nome" }, { value: "plays", label: "Ascolti" }]}>
  {#snippet icon(opt)}<UiIcon name={opt.value === "name" ? "sortByAlpha" : "chart"} />{/snippet}
</Segmented>
```

2–5 scelte esclusive, una sola misura ovunque. `block` per tutta la larghezza;
`iconOnly` su un'opzione tiene il testo come nome accessibile.

### `Metric`

```svelte
<Metric label="Brani" value={stats.track_count} />
<Metric label="Avvisi qualità" value={48} tone="warning" hint="12 senza copertina" onclick={openQuality} />
<Metric label="Album" loading />
```

Etichetta in frase normale, valore grande con cifre tabulari. `tone`:
`default | accent | success | warning | danger`. `loading` mostra uno skeleton
invece di un falso 0. `MetricCard` resta come alias deprecato.

### `FileDrop`

```svelte
<FileDrop accept="image/*" label="Trascina qui un'immagine" hint="JPG, PNG o WebP"
  buttonLabel="Scegli file" fileName={file?.name} onfiles={(f) => (file = f[0])} />
```

Sostituisce `<input type="file">` (bottone nativo non tradotto). Trascina e
rilascia o click/tastiera; filtra per `accept`; `multiple`, `compact`.

### `Badge` / `.rk-pill`

```svelte
{#if favCount > 0}
  <Badge tone="favorite" title="3 preferiti">{#snippet icon()}<UiIcon name="favorite" />{/snippet}3</Badge>
{/if}
```

Pill di stato: renderla **solo se ha un valore**. `tone`: `neutral | accent |
accent2 | success | warning | danger | favorite`, oppure `color` libero (mood).
`iconOnly` + `title` per i glifi. In liste lunghe usare direttamente le classi
`.rk-pill .rk-pill--warning` (meno componenti per riga).

## Temi

Look base **Midnight** (dark, accenti arancio/azzurro). `data-theme` sul root
seleziona i temi nominati (`themes.css`); `data-theme="server"` per il
pannello hub.
