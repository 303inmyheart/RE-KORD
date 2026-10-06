# Contributing to RE-KORD

Thanks for helping. This page explains how to report problems, propose changes and get a
pull request merged.

## Reporting bugs and requesting features

- **Bugs**: open a [GitHub issue](https://github.com/Creiv/RE-KORD/issues/new/choose) with
  the bug report template. Include the RE-KORD version, the platform and the report from
  *Settings › System › Diagnostics › "Copy report"*.
- **Feature ideas**: use the feature request template, or discuss them first on
  [r/RE_KORD](https://www.reddit.com/r/RE_KORD/).
- **Security vulnerabilities**: do **not** open a public issue. Follow
  [SECURITY.md](SECURITY.md).

## Workflow

1. For anything larger than a small fix, open an issue first so the approach can be agreed
   before you write code.
2. Fork the repository and create a branch from `main` (`fix/long-mp3-seek`,
   `feat/studio-batch-covers`, ...).
3. Set up the project: [docs/development.md](docs/development.md).
4. Make the change, with tests where it makes sense.
5. Run the [checks](#checks) locally.
6. Open a pull request against `main` and fill in the template.

Keep pull requests focused: one fix or feature per pull request is far easier to review than
a mix.

## Checks

CI runs all of these and must be green before a merge:

```bash
node scripts/version.mjs check
cargo fmt --all --check
cargo clippy --workspace --exclude rekord-client --all-targets --locked -- -D warnings
cargo test --workspace --exclude rekord-client --locked
pnpm -r --if-present check
pnpm -r --if-present test
pnpm build:ui
```

If you touched the Tauri shell (`apps/client-shell/src-tauri`):

```bash
cargo clippy -p rekord-client --locked -- -D warnings
cargo clippy -p rekord-client --features hub --locked -- -D warnings
```

If you touched Android code, build an APK (`pnpm android:build`) and test it on a device;
[docs/ANDROID.md](docs/ANDROID.md) lists what needs checking by hand.

## Code style

**General**

- Code, comments, commit messages and documentation are in **English**.
- Comments explain *why*, not *what*. Match the style of the surrounding code.
- Do not add dependencies lightly; explain new ones in the pull request.

**Rust** (`crates/`, `apps/server`, `apps/client-shell/src-tauri`)

- Formatting is `rustfmt` with the repository's `rustfmt.toml`; clippy runs with
  `-D warnings`.
- Use `anyhow` for application errors. HTTP handlers answer with the JSON envelope and a
  stable `snake_case` error code; document new codes in [docs/API.md](docs/API.md#error-codes).
- Never block an async worker: file system scans and external processes go through
  `spawn_blocking` or Tokio's process API.
- New routes that write to the library or the host must use the permission checks in
  `crates/core/src/perm.rs` (see [SECURITY.md](SECURITY.md)).
- Integration tests go in `crates/core/tests/`.

**Svelte and TypeScript** (`apps/client-ui`, `apps/server-ui`, `packages/ui`)

- Svelte 5 with runes, TypeScript, and `svelte-check` with no errors.
- Reusable visual elements belong in `packages/ui` or in an app's `src/components`; views
  stay thin and orchestrate layout and state.
- Logic that can be tested without the DOM goes in `src/lib/*.ts`, with a
  `*.test.mjs` next to it (Node's built-in test runner).
- Keep performance on WebKitGTK in mind: avoid continuous animations, large blurs and
  unbounded canvases.

## Translations (i18n)

RE-KORD ships in **Italian** (`it`, the reference and fallback), **English** (`en`) and
**German** (`de`). The rules:

- **Never hard-code user-facing text.** Every string goes through `t("key")` (or
  `tp("key", n)` for plurals).
- **Parity**: when you add or change a key, update the `it`, `en` and `de` files of the same
  table in the same pull request. `pnpm test` fails when key sets differ, when
  `{{placeholders}}` differ from English, when a value is empty, or when code uses a key that
  does not exist.
- Keep plural pairs (`key.one` / `key.other`) together in every language.
- If you cannot write good German or Italian, add your best attempt and say so in the pull
  request; a maintainer or translator will review it.
- German follows the style guide in [docs/TRANSLATIONS.md](docs/TRANSLATIONS.md#german-style-guide)
  (informal "du", product names unchanged).

## Commits

- Write the subject in the imperative, under about 72 characters, optionally prefixed by
  the area: `hub: protect curated dates on rescan`, `client: fix queue drag on touch`,
  `platform: sign Android release builds`, `docs: ...`.
- Explain the reason for the change in the body when it is not obvious.
- One logical change per commit where practical. Do not commit generated files
  (`dist/`, `target/`, `release/`).

## Pull requests

- Fill in the pull request template: what changed, why, how you tested it.
- Link the issue it closes (`Closes #123`).
- Add screenshots or a short recording for visible changes, on desktop and on a phone if the
  layout is affected.
- Update the documentation and [CHANGELOG.md](CHANGELOG.md) when behaviour changes.
- Do not bump the version; maintainers do that at release time
  ([release checklist](docs/development.md#release-checklist)).

