# RE-KORD Android client

The Android app is the **client shell** (Tauri 2) bundling `client-ui` locally.
It does **not** load the server SPA in a WebView.

## Architecture

1. Run **rekord-server** on LAN (or through the tunnel: *Settings › Network › Remote access*, or `REKORD_PUBLIC_URL`).
2. Install the RE-KORD client APK.
3. First launch asks for the hub: type `192.168.x.x` + port, or scan the QR shown by the
   hub panel (see below).
4. Pick an account, then browse library / favorites / playlists; audio streams from `/media`.

## First launch

`src/components/ConnectScreen.svelte`, gated by `src/lib/connect.svelte.ts`. Two steps —
address, then account — mirroring the old `electron/connect.html`.

The screen only appears when nobody answers. On startup the gate probes, in order, the
saved base URL (kept as is: an unreachable saved hub is the session's reconnect loop to
handle, with the UI up), the page origin, and `http://127.0.0.1:7420`. A browser served by
the hub therefore never sees it, and a desktop shell running next to its hub connects on
its own. Only the APK, where the origin is the app itself, lands on the form. *Settings ›
Network › Change hub* reopens it by hand, which is the way back when the hub's IP changes.

The probe is `GET /api/v1/health` followed by `GET /api/v1/accounts`, and it insists on
`service: "RE-KORD"`: a captive portal or another server on port 7420 answers 200 to
anything, and without that check the flow would close on an address that is not a hub.

Addresses are parsed in `src/lib/hubAddress.ts` (unit tests in `hubAddress.test.mjs`):
bare IP → `http` + port 7420, an `https` host keeps no port, any path in the QR is dropped
down to the origin.

## QR pairing

The hub draws the QR, the phone reads it.

- **Hub side**: `/admin` › *Network › Local network access* shows a QR of the LAN URL, and
  *Access from outside* one of the tunnel URL while it runs. The client's *Settings › Network* panel
  shows the same code (tunnel when up, LAN otherwise). Payload is the plain URL, no token.
- **Phone side**: `@tauri-apps/plugin-barcode-scanner` (`tauri-plugin-barcode-scanner` under
  `cfg(any(target_os = "android", target_os = "ios"))` in `src-tauri/Cargo.toml`, plugin
  registered under `#[cfg(mobile)]`, `barcode-scanner:default` in
  `capabilities/mobile.json`). The plugin's manifest merges `CAMERA` and `VIBRATE` into the
  APK; verify with `aapt2 dump badging`.

`src/lib/qrScan.ts` imports the plugin dynamically and hides the button unless
`checkPermissions()` answers, so the browser and the desktop shell never show a camera
button that cannot work. Camera permission is asked on tap, not at startup. Scanning runs
full screen (`windowed: false`): the windowed mode draws the camera behind the WebView and
would need the whole page transparent.

## Prerequisites

- Android SDK + NDK, JDK 17+
- Tauri 2 mobile prerequisites: https://v2.tauri.app/start/prerequisites/
- `minSdkVersion`: 26 (see `apps/client-shell/src-tauri/tauri.conf.json`)

`scripts/lib/android-env.sh` locates the SDK (`ANDROID_HOME`, `ANDROID_SDK_ROOT`,
`~/Android/Sdk`, `~/Library/Android/sdk`), picks the highest NDK under `$ANDROID_HOME/ndk`
unless `NDK_HOME` is set, checks `javac >= 17` and installs the missing Rust Android
targets. Every check fails with what to install, before Gradle starts.

## Build

```bash
pnpm pack:android               # release/android/RE-KORD-Client-<v>-android-arm64.apk
                                # (optimized; your keystore if present, else the debug key)
pnpm android:build              # debug APK, arm64, signed with the debug key
pnpm android:build --install    # …and push it to the attached device via adb
pnpm android:apk                # release, one APK per ABI (needs a keystore)
pnpm android:dev                # tauri android dev on device / emulator
pnpm android:init               # toolchain check; regenerates gen/android if deleted
```

`./scripts/android-build.sh --help` lists the flags (`--release`, `--debug`, `--apk`,
`--aab`, `--split`, `--install`, `--targets aarch64,armv7,i686,x86_64`). Default is a
debug arm64 APK: arm64 is every phone in circulation, and each extra ABI is another
Rust compile. Artifacts land in
`apps/client-shell/src-tauri/gen/android/app/build/outputs/`.

## The native project is versioned

`apps/client-shell/src-tauri/gen/android` is **in git** (the root `.gitignore` entry is
`gen/*` plus a `!gen/android/` exception — with a trailing slash git would not descend
into `gen/` and the exception would never be read). We patch that project, and
`tauri android init` rewrites it from scratch on every machine:

- **Cleartext HTTP in release builds** (`app/build.gradle.kts`). The hub answers at an
  address like `http://192.168.1.20:7420`; a private IP has no certificate to offer, and
  Android's default (`usesCleartextTraffic=false` outside debug) would leave the release
  APK unable to reach any hub.
- **Release signing** from `gen/android/keystore.properties`, absent from git. If the
  file is missing the release is signed with the SDK debug key: still optimized and
  installable (like the legacy APK), but not publishable, and moving to the real key later
  means uninstalling once.

The Tauri-generated pieces stay out of git thanks to the `.gitignore` files inside
`gen/android` (`jniLibs/**/*.so`, `assets/tauri.conf.json`, `tauri.build.gradle.kts`,
`tauri.settings.gradle`, `src/main/**/generated`, `build/`); the CLI rewrites them on
every build, so a fresh clone builds without running `android init`.

## Signing a release APK

One keystore, kept forever: updates only install over the same key.

```bash
cd apps/client-shell/src-tauri/gen/android
keytool -genkey -v -keystore rekord.jks -keyalg RSA -keysize 2048 -validity 10000 -alias rekord
cat > keystore.properties <<'PROPS'
storeFile=rekord.jks
storePassword=…
keyAlias=rekord
keyPassword=…
PROPS
```

`*.jks`, `*.keystore` and `keystore.properties` are ignored inside `gen/android`.

### Coming from the legacy APK: uninstall first

The legacy (Capacitor) APKs were distributed **signed with the debug key**. Android only
installs an update over an app signed with the *same* key, so the first RE-KORD 5 release APK
(signed with `rekord.jks`) is refused with `INSTALL_FAILED_UPDATE_INCOMPATIBLE` / "App not
installed". Uninstall the old RE-KORD first (`adb uninstall app.rekord.client`, or from the
launcher). Nothing is lost that matters: favourites, playlists and preferences live on the
hub; the phone only remembers the hub address and the chosen account, which the first
launch asks again (or reads from the QR). From then on keep the same keystore forever.

## MediaSession / background playback

The web client wires `navigator.mediaSession` in `src/lib/mediaSession.ts`: metadata with
three real artwork variants (`?size=128`, `?size=256`, original), `playbackState`,
`setPositionState`, and handlers for play/pause/stop, previous/next and seek
(absolute plus ±offset). Shuffle, repeat, favourite and shuffle-exclude are registered
under the non-standard action names some browsers ship, which no desktop browser
accepts today.

A native shell that owns the notification does not need the Media Session API: it
reaches the same bridge by dispatching a DOM event in the WebView.

```js
window.dispatchEvent(
  new CustomEvent("rekord:media-action", { detail: { action: "toggleshuffle" } }),
);
// actions: play, pause, stop, nexttrack, previoustrack, seekto (detail.value = seconds),
// seekby (detail.value = delta), toggleshuffle, togglerepeat, togglelike, dislike
```

### Background playback on Android

The Android WebView has no Media Session API — `navigator.mediaSession` is simply absent —
so on the phone that whole file would talk to nobody. The three setters therefore also push
the same state to the shell through `src/lib/nativeMedia.ts`, which looks for
`window.RekordMediaNative` and does nothing when it is not there (browser, desktop). The
snapshot (title, artist, album, artwork URL, playing, duration, position) is coalesced over
80 ms, because metadata, transport state and position arrive as three separate calls on
every track change.

The Kotlin side lives in `gen/android`, all of it in versioned files:

- `RekordMedia.kt` — the `@JavascriptInterface` object bound in `MainActivity.onWebViewCreate`,
  plus `RekordMediaBridge`, which sends commands back by dispatching `rekord:media-action`
  in the page. No new command channel: the notification speaks the same language as the
  lock screen on desktop. Each command resumes the WebView first, because a paused track in
  the background leaves it suspended and `play` would reach a sleeping player.
- `RekordMediaService.kt` — foreground service (`mediaPlayback`) holding a
  `MediaSessionCompat`. It has **no player**: the audio stays in the WebView. It exists for
  the two things a page cannot do, keep the process alive with the screen off and own a
  system media session, from which the notification, the lock-screen controls and the
  headset button come. Artwork is the cached 256 thumbnail, fetched on a worker thread and
  redrawn when it lands. Media3 is the modern replacement for `MediaSessionCompat` but wants
  a `Player` implementation, and here the player is an `<audio>` tag on the far side of a
  JavaScript bridge.
- `MainActivity.kt` — `WryActivity.onPause()` pauses the WebView, and a paused WebView
  stops the audio, so while a track is playing the WebView is resumed right after. The
  foreground service is what makes that safe: the process is not frozen.

Service lifecycle: it starts on the first track that plays (Android 12+ only allows a
foreground service to start from the foreground, which is where the first play happens), it
drops out of foreground on pause while keeping the notification, and it stops when the queue
empties, when the notification is dismissed or when the activity dies. That last one is not
a detail: the audio lives in the WebView, so closing the app from Recents ends playback and
the notification must go with it.

Permissions in the manifest: `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`
(required as a permission from Android 14) and `POST_NOTIFICATIONS`, asked on the first
track rather than at startup — first there is something to show, then we ask to show it. If
it is refused the music still plays, without controls. `proguard-rekord.pro` keeps the
`@JavascriptInterface` methods: nothing in Java calls them, and R8 would drop them.

Pairing goes through the first-launch screen above; there is no PWA install path on the
APK (the PWA exists only for the browser client served by the hub).

### Back button

`MainActivity` sets `handleBackNavigation = false` (WryActivity's own handler finishes the
activity at the bottom of the history, killing the WebView and the music with it) and
installs its own `OnBackPressedCallback`:

1. if the WebView can go back, `goBack()` — the client pushes a history entry for every
   view change and every modal/sheet it opens, so Back closes the sheet, then returns to
   the previous view;
2. at the bottom of the history, `moveTaskToBack(true)`: the app goes to the background
   exactly like Home, the activity is **not** destroyed and playback continues (the
   foreground service keeps the process alive).

`onDestroy` therefore only runs when the activity really dies (removed from Recents, process
reclaimed); only then does it stop the media service — the WebView, and the audio in it, is
gone. Configuration changes do not destroy it (`configChanges` in the manifest).

### Orientation

Phones (`smallestScreenWidthDp < 600`) are locked to portrait, as in the legacy app; tablets and TVs
rotate freely. Decided in `MainActivity.applyOrientationPolicy()` before `super.onCreate`, so
there is no rotate-then-snap on startup.

### Audio focus, headphones, delivery of commands

`RekordMediaService` owns the audio focus on behalf of the WebView (which does not request
it on its own):

| Event | Action |
|-------|--------|
| a track starts (state `playing` from the page, or Play from the notification) | `requestAudioFocus(GAIN)`, `USAGE_MEDIA` / `CONTENT_TYPE_MUSIC`, delayed gain accepted |
| `AUDIOFOCUS_LOSS` (another music app) | `pause`, focus abandoned, no auto-resume |
| `AUDIOFOCUS_LOSS_TRANSIENT` (call, navigator prompt) | `pause`, remembers it was playing |
| `AUDIOFOCUS_GAIN` after a transient loss | `play` if it was playing |
| `AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK` | nothing: from Android 8 the system ducks by itself (`setWillPauseWhenDucked(false)`) |
| `ACTION_AUDIO_BECOMING_NOISY` (headphones unplugged, BT off) | `pause` (receiver registered only while playing) |
| service destroyed / stop | focus abandoned, receiver removed |

All of these reach the player through the same `rekord:media-action` event as the
notification buttons. `RekordMediaBridge` delivers each command with a 5 s partial wake lock
(`WAKE_LOCK` permission) and retries every 250 ms, up to 12 times, until the page answers:
the injected script returns `true` only when `window.__rekordNativeMediaReady` is set
(`nativeMedia.ts` raises it after the first state reaches the shell, i.e. the player is
mounted and listening). A newer command replaces a pending one. Same approach as the legacy
`MainActivity.java`.

### Files saved from the page

Backups, profile and theme exports are built in memory and "downloaded" through
`<a download href="blob:…">`, which the Android WebView ignores. `src/lib/platform/downloads.ts`
intercepts those clicks inside the shell and streams the bytes in base64 chunks (384 KiB) to
`RekordFiles.kt` (`window.RekordFilesNative`): on Android 10+ the file goes to **Download**
through `MediaStore` (no permission), on 8–9 to the app's private
`Android/data/app.rekord.client/files/Download`. A toast shows where it landed.

### Google Cast

The WebView has no Cast extension, so the web sender (`src/lib/cast/googleCast.ts`) stays
hidden in the app. The shell carries a native sender instead, ported from the legacy
`RekordCastManager`:

- `RekordCast.kt`: Google Cast SDK (`play-services-cast-framework` 22.0.0, the legacy app's version,
  plus `androidx.mediarouter` 1.6.0). `RekordCastOptionsProvider`, declared in the manifest
  with the `OPTIONS_PROVIDER_CLASS_NAME` meta-data and kept by `proguard-rekord.pro`, selects
  the **Default Media Receiver** and turns off the Cast SDK's own notification and
  MediaSession, so the same track never shows up in two notifications.
- `window.RekordCastNative` (`RekordCastJs`) has `getStatus()`, `requestSession()`
  (androidx `MediaRouteChooserDialog`), `endSession(stop)`, `load(json)` (URL, MIME,
  title/artist/album, cover, start position, autoplay), `play`, `pause`, `seek` and `stop`.
- Everything comes back as the DOM event `rekord:cast`, the same channel `RekordMediaBridge`
  uses for the notification:
  - `status`: cast state, device name, player state, idle reason, position, duration and
    content id, at about 1 Hz while playing;
  - `session`: `cancelled`, `startFailed` with an SDK code, or `unavailable`;
  - `result`: the outcome of a load, matched by id.
- `src/lib/cast/androidCast.ts` implements `CastBackend` over this bridge, and
  `androidCastStatus.ts` maps the raw SDK names to `CastStatus` (tested in
  `castAndroid.test.mjs`). When the bridge exists, `castController` picks this backend on its
  own. The Cast button, the player hand-off (`player.setRemoteOutput`) and queue advance
  on `FINISHED` are the same as on the web.
- **Media URLs**: same as on the web (`castMedia.ts`). In the app the hub is already a LAN
  address (`http://192.168.x.y:7420`), so the receiver uses it as is. Flac, ogg, opus and wav
  go through `/api/v1/transcode/…?format=mp3` when the hub's health reports `transcode`.
  Without the transcoder they are sent as they are, and the receiver may refuse them.
- **While casting**: local decks are paused (the player's remote output). The notification
  keeps mirroring the player, which now follows the receiver, so it shows the cast track,
  its play state and "On <device>". Its buttons, the headphone button and the lock screen
  control the Chromecast through the usual `rekord:media-action`.
  `RekordMediaService` drops audio focus and the "becoming noisy" receiver: a phone call or
  unplugged headphones must not pause the living-room speaker. It also switches the
  MediaSession to remote volume, so the volume keys move the Chromecast's volume with the
  screen off. With the app open, `MainActivity.dispatchKeyEvent` does the same. The WebView
  stays awake in the background for as long as a session is connected, because the page is
  what moves the queue forward when a track ends.
- **Ending**: disconnecting from the button ends the session and stops the receiver. The
  player resumes locally at the last reported position. If the app is killed while
  casting, the receiver finishes the current track. The SDK resumes the saved session on
  the next launch, and the page then reloads the current track on it.

**Requirements**: Google Play Services on the phone (Cast framework), and the phone, the
Chromecast or Google Home and the hub on the **same LAN**. The hub address must be the
LAN IP, not `localhost`. Without Play Services, or with a version the SDK rejects, the init
reports `supported:false` and the Cast button never appears. Neither case is an error.

**Not verifiable without a device and a Chromecast**: discovery and the device picker, a
real session start, playback of `/media` and transcoded URLs on the receiver, cover art on
the TV or Nest Hub, the 1 Hz progress, queue advance on track end (also with the screen
off), volume keys, the notification while casting, cancel/start-failed toasts, session
resume after an app restart, and behaviour on devices without Play Services, such as
Huawei phones or AOSP TV boxes. Useful while testing:

```bash
adb logcat | grep -iE "RekordCast|CastContext|MediaRouter"
```

### Manifest notes

- `CAMERA` is merged from the barcode-scanner plugin and also declared explicitly; the
  plugin declares `android.hardware.camera.any` as **required**, which would hide the app on
  the Play Store for TVs and camera-less tablets. The manifest overrides it with
  `required="false"` (`tools:replace`), plus `android.hardware.camera` and
  `android.hardware.touchscreen` optional. Verify:
  `aapt2 dump badging app-universal-release.apk | grep feature`.
- `LEANBACK_LAUNCHER` now has an `android:banner` (`res/drawable-xhdpi/tv_banner.png`,
  320×180 dp), required for the Android TV launcher.
- `android:allowBackup="false"`: Android auto-backup would copy the WebView's localStorage
  (hub address, chosen account) to another phone where that hub may not exist. The data
  that matters lives on the hub, which has its own backup.

On device, still to be checked by hand (no emulator on the build machine):

```bash
adb shell dumpsys media_session | grep -A6 rekord     # session and state
adb shell dumpsys activity services app.rekord.client # servizio in foreground
```

Play a track, lock the screen, wait a minute: the sound must not stop, and the notification
buttons must move the player. If the audio dies the moment the app goes to background, the
WebView resume in `MainActivity.onPause` is the place to look.

## Notes

- Verified locally: universal release APK 15 MB, the same size as before the scanner and the
  media notification: the scanner plugin reaches ML Kit through Play Services instead of
  bundling the model, and `androidx.media` is a handful of classes. The flip side is
  that scanning needs Play Services on the phone; without it the QR button fails and the
  address has to be typed, which the first-launch screen allows anyway. `R8` keeps the
  plugin: `app/tauri/barcodescanner/BarcodeScannerPlugin` and the ML Kit registrars are in
  `classes.dex` of the minified build.
- `app.rekord.client`, versionCode from the version Tauri reads from `Cargo.toml` (the
  workspace version; `tauri.conf.json` no longer carries one — 5.0.0 → 5000000), minSdk 26,
  targetSdk 36, `usesCleartextTraffic=true` in debug and release, signed release verified
  with `apksigner verify --print-certs`.
- The WebView's origin is `http://tauri.localhost` (Android) and the hub is another origin:
  the hub's CORS policy explicitly allows `tauri://localhost` and `http(s)://tauri.localhost`
  (see [API.md](API.md#cors)), which is what lets the APK call it at all. A reverse proxy in
  front of the hub must not strip those headers.
- Plectr, Sonic Nebula and DiscoWall are part of the client and work in the app.
- **Cast**: native Google Cast is back (it was in the legacy APK as `RekordCastManager`); see
  [Google Cast](#google-cast).
