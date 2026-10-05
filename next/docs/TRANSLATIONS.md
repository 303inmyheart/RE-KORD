# Translations

RE-KORD ships in **Italian** (`it`, the reference and fallback language), **English** (`en`)
and **German** (`de`). This page explains where the strings live and how to add or maintain
a locale.

## Credits

The German translation is based on the work of **[@knoellix](https://github.com/knoellix)**,
who translated the whole legacy React app (about 1,290 keys) in
[PR #93](https://github.com/Creiv/RE-KORD/pull/93), *Add German (de) locale support*.

The Svelte client uses different keys from the legacy app, so the strings were carried over
by matching their English text: every string that still has an equivalent in the new UI keeps
knoellix's wording. Strings added after the legacy app were translated following the same
conventions (below). Thank you, knoellix!

## Where the strings live

| Surface | Tables | Loader |
| --- | --- | --- |
| Client (`apps/client-ui`) | `src/locales/{it,en,de}.json` (base) plus `src/locales/<area>/{it,en,de}.json` (fragments: `core`, `ui`, `plectr`, `nebula`, `cast`, `platform`, `studio`, `hub`, `i18n-sweep`, …) | `src/lib/i18n.svelte.ts` |
| Hub panel (`apps/server-ui`) | `src/locales/{it,en,de}.json` | `src/lib/i18n.svelte.ts`, locale helpers in `src/lib/locale.ts` |
| Android notification (`apps/client-shell/src-tauri/gen/android/…/res`) | `values/strings.xml` (English), `values-it/`, `values-de/` | Android picks the system language |

Tables are flat `key → string` maps. Placeholders use `{{name}}` and must appear unchanged in
every language. Plurals use `key.one` / `key.other` (picked with `Intl.PluralRules`, so a
language with more categories can add `key.few`, `key.many`, …); call them with `tp("key", n)`.

In the client, Italian is bundled in the entry chunk and is the fallback for any missing key;
every other language is a lazy chunk loaded the first time it is selected. The hub panel
bundles all its tables.

## Adding a locale

Taking a hypothetical French (`fr`) as an example:

1. **Client locale list.** In `apps/client-ui/src/lib/userPrefs.ts` add `"fr"` to `AppLocale`,
   `APP_LOCALES` and `normalizeLocale`. Browser detection (`browserLocale`) picks it up from
   `APP_LOCALES`.
2. **Client loader.** In `apps/client-ui/src/lib/i18n.svelte.ts` add an entry to
   `LAZY_TABLES` (base table + `import.meta.glob("../locales/*/fr.json")`) and the `Intl` tag
   to `INTL_TAGS` (e.g. `fr-FR`). Number, date, relative-time and plural formatting all go
   through that tag.
3. **Client validation of synced / imported settings.** `session.svelte.ts` (hub user-state)
   and `legacyImport.ts` whitelist the accepted locale codes: add `"fr"` there.
4. **Pickers.** Add an option to Settings › Interface (`components/settings/InterfacePanel.svelte`,
   label key `settings.langFr` in every base table) and to the connect screen
   (`components/ConnectScreen.svelte`, label written in its own language).
5. **Hub panel.** Add `"fr"` to `AdminLocale` / `ADMIN_LOCALES` and `DEFAULT_INTL_TAGS` in
   `apps/server-ui/src/lib/locale.ts`, import the table in `src/lib/i18n.svelte.ts`, and add
   the `lang.fr` label to every panel table.
6. **Tables.** Create `fr.json` next to **every** `en.json` (client base, every client fragment,
   hub panel). Start from a copy of `en.json` and translate the values; keep the key order.
7. **Tests.** Add `"fr"` to `LANGS` / `FOLLOW_EN` in `apps/client-ui/src/lib/i18nTables.test.mjs`,
   and extend `apps/server-ui/src/lib/i18n.test.mjs` the way `de` is checked there.
8. **Android (optional).** Add `res/values-fr/strings.xml` with the notification strings.

The hub itself does not validate the UI locale: the user-state `settings.locale` is stored as
sent by the client.

## Maintaining a locale

- When you add a key, add it to the `it`, `en` and `de` files of the same table in the same
  change. `pnpm test` fails if a table's key sets differ, if `{{placeholders}}` differ from
  English, if a value is empty, or if the code uses a literal key that does not exist.
- Keep plural pairs together: if `en` has `x.one` / `x.other`, so must every other language.
- Run `pnpm -r --if-present test` and `pnpm build:ui`, then switch the UI to the language
  (Settings › Interface › Language, and the selector in the hub panel header) to check for
  untranslated or overflowing text.

## German style guide

These conventions come from PR #93 and apply to new German strings too.

- **Address the user with “du”** (informal), never “Sie”: *Wähle einen Ordner*, *deine
  Bibliothek*.
- **Product and feature names stay as they are:** RE-KORD, Plectr, Dashboard, Studio,
  Smart Radio, Smart Shuffle, DiscoWall, Nebula, Discover, and the theme names.
- **Some music and game terms stay in English by design:** mood names such as *Soulful / Groovy*;
  in the Plectr HUD *Score*, *Combo*, *Perfect / Good / Early / Late / Miss*, *Full Combo*;
  also *Shuffle*, *Playlist*, *Preview*, *Release*, *Single*, *Cover*, *Sleep-Timer*,
  *Crossfade*, *Visualizer*, *Backup*, *Log*, *Token*.
- **Terminology:** track → *Titel* (*Song* in Plectr), artist → *Künstler*, library →
  *Bibliothek*, queue → *Warteschlange*, favorites → *Favoriten*, lyrics → *Songtext*,
  mood → *Stimmung*, plays → *Wiedergaben*, account → *Konto*, profile → *Profil*,
  music folder → *Musikordner*, hub panel → *Hub-Panel*, remote access → *Fernzugriff*,
  remote admin → *Fernverwaltung*, machine operations → *Systemvorgänge*, the account named
  “Default” → *Konto Default* (the generic “default account” is *Standardkonto*).
- **Typography:** German quotes „…“, a space before units and percent signs (*15 MB*,
  *30 %*), `z. B.` with a space, ellipsis `…` for ongoing actions (*Wird geladen…*).
