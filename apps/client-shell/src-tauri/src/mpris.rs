//! Native media controls on Linux: an MPRIS player on the session bus.
//!
//! WebKitGTK does not publish the page's Media Session as an MPRIS player, so
//! GNOME's media widget, shell extensions and the media keys saw nothing (or
//! lost the track). The page already sends its now-playing state to the
//! Android shell through `nativeMedia.ts`; on desktop the same snapshot comes
//! here through the `media_update` / `media_clear` commands, and the player's
//! methods (Play, Pause, Next, Seek…) go back to the page as the
//! `rekord:media-action` event it already handles.
//!
//! Cost: nothing until something plays (the bus connection is opened on the
//! first track and closed when the queue empties), and D-Bus traffic only on
//! real changes: metadata and playback status when they change, `Seeked`
//! only when the position jumps. Position is never pushed while playing; the
//! shell extrapolates it from `Position` and `Rate`.
//!
//! On other desktops the commands exist and do nothing.

use serde::Deserialize;

/// What the page reports (`nativeMedia.ts` snapshot, plus a local art file).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// `file://` URL of a small cached cover, or empty.
    pub art_url: String,
    pub playing: bool,
    pub duration_ms: u64,
    pub position_ms: u64,
}

#[cfg(target_os = "linux")]
pub use imp::*;

#[cfg(not(target_os = "linux"))]
mod imp_none {
    use super::NowPlaying;

    #[derive(Default)]
    pub struct MediaState;

    #[tauri::command]
    pub async fn media_update(now: NowPlaying) -> Result<(), String> {
        let _ = now;
        Ok(())
    }

    #[tauri::command]
    pub async fn media_clear() -> Result<(), String> {
        Ok(())
    }

    #[tauri::command]
    pub async fn media_art(request: tauri::ipc::Request<'_>) -> Result<String, String> {
        let _ = request;
        Ok(String::new())
    }
}
#[cfg(not(target_os = "linux"))]
pub use imp_none::*;

#[cfg(target_os = "linux")]
mod imp {
    use super::NowPlaying;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::Instant;
    use tauri::async_runtime::Mutex;
    use tauri::{AppHandle, Manager, State};
    use zbus::object_server::SignalEmitter;
    use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
    use zbus::{connection, interface, Connection};

    const BUS_NAME: &str = "org.mpris.MediaPlayer2.rekord";
    const PATH: &str = "/org/mpris/MediaPlayer2";
    /// A reported position further than this from the extrapolated one is a seek.
    const SEEK_TOLERANCE_US: i64 = 1_500_000;
    /// Cached covers kept for the media widget.
    const ART_KEEP: usize = 64;

    #[derive(Default)]
    pub struct MediaState {
        conn: Mutex<Option<Connection>>,
    }

    /// Send a media command to the page, which handles it like a media key.
    fn dispatch(app: &AppHandle, action: &str, value: Option<f64>) {
        let detail = match value {
            Some(v) if v.is_finite() => format!("{{\"action\":\"{action}\",\"value\":{v}}}"),
            _ => format!("{{\"action\":\"{action}\"}}"),
        };
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.eval(format!(
                "window.dispatchEvent(new CustomEvent('rekord:media-action',{{detail:{detail}}}))"
            ));
        }
    }

    struct Root {
        app: AppHandle,
        identity: String,
    }

    #[interface(name = "org.mpris.MediaPlayer2")]
    impl Root {
        fn raise(&self) {
            if let Some(window) = self.app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }

        fn quit(&self) {}

        #[zbus(property)]
        fn can_quit(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn can_raise(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn has_track_list(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn identity(&self) -> String {
            self.identity.clone()
        }

        #[zbus(property)]
        fn desktop_entry(&self) -> String {
            self.identity.clone()
        }

        #[zbus(property)]
        fn supported_uri_schemes(&self) -> Vec<String> {
            Vec::new()
        }

        #[zbus(property)]
        fn supported_mime_types(&self) -> Vec<String> {
            Vec::new()
        }
    }

    struct Player {
        app: AppHandle,
        now: NowPlaying,
        /// Bumped on every new track (`mpris:trackid`).
        track: u64,
        /// Position (µs) at `at`, extrapolated while playing.
        position_us: i64,
        at: Instant,
    }

    impl Player {
        fn current_position(&self) -> i64 {
            if !self.now.playing {
                return self.position_us;
            }
            let elapsed = self.at.elapsed().as_micros() as i64;
            let p = self.position_us.saturating_add(elapsed);
            let len = self.now.duration_ms as i64 * 1000;
            if len > 0 {
                p.min(len)
            } else {
                p
            }
        }

        fn track_path(&self) -> OwnedObjectPath {
            ObjectPath::try_from(format!("/app/rekord/track/{}", self.track))
                .map(Into::into)
                .unwrap_or_else(|_| ObjectPath::from_static_str_unchecked("/").into())
        }
    }

    fn owned(v: Value<'_>) -> OwnedValue {
        v.try_to_owned().unwrap_or_else(|_| OwnedValue::from(0u8))
    }

    #[interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Player {
        fn next(&self) {
            dispatch(&self.app, "nexttrack", None);
        }

        fn previous(&self) {
            dispatch(&self.app, "previoustrack", None);
        }

        fn pause(&self) {
            dispatch(&self.app, "pause", None);
        }

        fn play_pause(&self) {
            let action = if self.now.playing { "pause" } else { "play" };
            dispatch(&self.app, action, None);
        }

        fn stop(&self) {
            dispatch(&self.app, "pause", None);
        }

        fn play(&self) {
            dispatch(&self.app, "play", None);
        }

        fn seek(&self, offset: i64) {
            dispatch(&self.app, "seekby", Some(offset as f64 / 1e6));
        }

        fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
            if track_id.as_str() != self.track_path().as_str() || position < 0 {
                return;
            }
            dispatch(&self.app, "seekto", Some(position as f64 / 1e6));
        }

        fn open_uri(&self, _uri: String) {}

        #[zbus(property)]
        fn playback_status(&self) -> String {
            if self.now.playing {
                "Playing"
            } else {
                "Paused"
            }
            .to_string()
        }

        #[zbus(property)]
        fn rate(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn minimum_rate(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn maximum_rate(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn volume(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, OwnedValue> {
            let mut m = HashMap::new();
            m.insert(
                "mpris:trackid".into(),
                owned(Value::from(self.track_path())),
            );
            if self.now.duration_ms > 0 {
                m.insert(
                    "mpris:length".into(),
                    owned(Value::from(self.now.duration_ms as i64 * 1000)),
                );
            }
            m.insert(
                "xesam:title".into(),
                owned(Value::from(self.now.title.as_str())),
            );
            if !self.now.artist.is_empty() {
                m.insert(
                    "xesam:artist".into(),
                    owned(Value::from(vec![self.now.artist.as_str()])),
                );
            }
            if !self.now.album.is_empty() {
                m.insert(
                    "xesam:album".into(),
                    owned(Value::from(self.now.album.as_str())),
                );
            }
            if !self.now.art_url.is_empty() {
                m.insert(
                    "mpris:artUrl".into(),
                    owned(Value::from(self.now.art_url.as_str())),
                );
            }
            m
        }

        #[zbus(property(emits_changed_signal = "false"))]
        fn position(&self) -> i64 {
            self.current_position()
        }

        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_go_previous(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_pause(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_seek(&self) -> bool {
            self.now.duration_ms > 0
        }

        #[zbus(property(emits_changed_signal = "const"))]
        fn can_control(&self) -> bool {
            true
        }

        #[zbus(signal)]
        async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
    }

    async fn open(app: &AppHandle, now: &NowPlaying) -> zbus::Result<Connection> {
        let identity = app
            .config()
            .product_name
            .clone()
            .unwrap_or_else(|| "RE-KORD".into());
        let player = Player {
            app: app.clone(),
            now: now.clone(),
            track: 1,
            position_us: now.position_ms as i64 * 1000,
            at: Instant::now(),
        };
        connection::Builder::session()?
            .name(BUS_NAME)?
            .serve_at(
                PATH,
                Root {
                    app: app.clone(),
                    identity,
                },
            )?
            .serve_at(PATH, player)?
            .build()
            .await
    }

    async fn update(conn: &Connection, now: NowPlaying) -> zbus::Result<()> {
        let iface = conn.object_server().interface::<_, Player>(PATH).await?;
        let emitter = iface.signal_emitter().clone();
        let mut p = iface.get_mut().await;
        let new_track = p.now.title != now.title
            || p.now.artist != now.artist
            || p.now.album != now.album
            || p.now.duration_ms != now.duration_ms;
        let meta_changed = new_track || p.now.art_url != now.art_url;
        let status_changed = p.now.playing != now.playing;
        let reported = now.position_ms as i64 * 1000;
        let jumped = (p.current_position() - reported).abs() > SEEK_TOLERANCE_US;
        if new_track {
            p.track += 1;
        }
        p.now = now;
        p.position_us = reported;
        p.at = Instant::now();
        if meta_changed {
            p.metadata_changed(&emitter).await?;
            p.can_seek_changed(&emitter).await?;
        }
        if status_changed {
            p.playback_status_changed(&emitter).await?;
        }
        if jumped && !new_track {
            Player::seeked(&emitter, reported).await?;
        }
        Ok(())
    }

    #[tauri::command]
    pub async fn media_update(
        app: AppHandle,
        state: State<'_, MediaState>,
        now: NowPlaying,
    ) -> Result<(), String> {
        let mut guard = state.conn.lock().await;
        if guard.is_none() {
            *guard = Some(open(&app, &now).await.map_err(|e| e.to_string())?);
            return Ok(());
        }
        if let Some(conn) = guard.as_ref() {
            update(conn, now).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Nothing to show any more: release the bus name (the widget goes away).
    #[tauri::command]
    pub async fn media_clear(state: State<'_, MediaState>) -> Result<(), String> {
        state.conn.lock().await.take();
        Ok(())
    }

    fn art_dir(app: &AppHandle) -> Option<PathBuf> {
        Some(app.path().app_cache_dir().ok()?.join("media-art"))
    }

    /// Store a cover the page already downloaded (256 px thumbnail) and return
    /// its `file://` URL: the shell's widget loads local files reliably.
    /// The `x-art-key` header names it (album + cover version).
    #[tauri::command]
    pub async fn media_art(
        app: AppHandle,
        request: tauri::ipc::Request<'_>,
    ) -> Result<String, String> {
        let key = request
            .headers()
            .get("x-art-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let name: String = key
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(80)
            .collect();
        if name.is_empty() {
            return Err("missing art key".into());
        }
        let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
            return Err("expected raw bytes".into());
        };
        if bytes.is_empty() || bytes.len() > 2 * 1024 * 1024 {
            return Err("unexpected cover size".into());
        }
        let dir = art_dir(&app).ok_or("no cache dir")?;
        let path = dir.join(format!("{name}.img"));
        if !path.is_file() {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            prune(&dir);
        }
        Ok(format!("file://{}", path.display()))
    }

    /// Keep only the most recent covers.
    fn prune(dir: &std::path::Path) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut files: Vec<_> = rd
            .flatten()
            .map(|e| (e.metadata().and_then(|m| m.modified()).ok(), e.path()))
            .collect();
        if files.len() <= ART_KEEP {
            return;
        }
        files.sort_by_key(|f| std::cmp::Reverse(f.0));
        for (_, path) in files.into_iter().skip(ART_KEEP) {
            let _ = std::fs::remove_file(path);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn now_playing_reads_the_page_snapshot() {
            let now: NowPlaying = serde_json::from_str(
                r#"{"title":"T","artist":"A","album":"B","artUrl":"file:///x.img",
                    "playing":true,"durationMs":180000,"positionMs":1000,"artworkUrl":"http://x"}"#,
            )
            .unwrap();
            assert_eq!(now.title, "T");
            assert!(now.playing);
            assert_eq!(now.duration_ms, 180_000);
            assert_eq!(now.art_url, "file:///x.img");
        }
    }
}
