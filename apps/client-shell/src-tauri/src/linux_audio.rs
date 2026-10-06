//! One persistent audio output on Linux.
//!
//! WebKitGTK gives every `<audio>` element its own GStreamer pipeline, and
//! each pipeline opens its own PipeWire/PulseAudio stream: a track change
//! destroyed one stream and created another, a pause corked it. Each of those
//! wakes the sound server, WirePlumber, any effects host (EasyEffects) and the
//! shell's volume/mixer UI, which the user felt as the whole desktop freezing
//! for a moment at every pause, resume and track change. Electron (the legacy
//! app) never did that: Chromium mixes everything into one long-lived stream.
//!
//! WebKitGTK can do the same: with `WEBKIT_GST_ENABLE_AUDIO_MIXER=1` all media
//! elements and Web Audio feed one shared `audiomixer` pipeline, so there is a
//! single output stream for the whole app. It needs GStreamer's `inter`
//! plugin (gst-plugins-bad) and `audiomixer` (gst-plugins-base); WebKit falls
//! back to per-element sinks on its own when they are missing, but the page
//! must know whether the mixer is on (it then keeps the shared stream warm
//! across pauses, see `audioKeepAlive.ts` in the client), so the shell only
//! turns it on when it can find both plugins.

use std::env;
use std::path::{Path, PathBuf};

const MIXER_ENV: &str = "WEBKIT_GST_ENABLE_AUDIO_MIXER";
const REQUIRED_PLUGINS: [&str; 2] = ["libgstinter.so", "libgstaudiomixer.so"];

/// Turns the WebKit audio mixer on when GStreamer can provide it. Returns
/// whether it is on. Must run before the first webview is created (WebKit
/// reads the variable once) and before any other thread starts.
pub fn enable_shared_output() -> bool {
    if let Some(value) = env::var_os(MIXER_ENV) {
        // The user decided (`0` turns it off for debugging).
        return value == "1";
    }
    let dirs = plugin_dirs();
    if !REQUIRED_PLUGINS.iter().all(|p| find_plugin(&dirs, p)) {
        return false;
    }
    // Single-threaded here: called first thing in `run()`.
    env::set_var(MIXER_ENV, "1");
    true
}

/// Script run in the page before its own code, telling the client the mixer is on.
pub fn page_flag_script() -> &'static str {
    "window.__REKORD_SHARED_AUDIO_OUTPUT__ = true;"
}

fn find_plugin(dirs: &[PathBuf], file: &str) -> bool {
    dirs.iter().any(|d| d.join(file).is_file())
}

/// Where GStreamer looks for plugins: the documented environment variables
/// first (AppImage sets them to its bundled copy), then the distribution's
/// usual system directories.
fn plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in [
        "GST_PLUGIN_PATH_1_0",
        "GST_PLUGIN_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GST_PLUGIN_SYSTEM_PATH",
    ] {
        if let Some(value) = env::var_os(var) {
            dirs.extend(env::split_paths(&value).filter(|p| !p.as_os_str().is_empty()));
        }
    }
    if let Some(appdir) = env::var_os("APPDIR") {
        let appdir = Path::new(&appdir);
        dirs.push(appdir.join("usr/lib/gstreamer-1.0"));
        dirs.push(appdir.join(format!(
            "usr/lib/{}-linux-gnu/gstreamer-1.0",
            env::consts::ARCH
        )));
    }
    let arch = env::consts::ARCH;
    for dir in [
        format!("/usr/lib/{arch}-linux-gnu/gstreamer-1.0"),
        "/usr/lib64/gstreamer-1.0".to_string(),
        "/usr/lib/gstreamer-1.0".to_string(),
        "/usr/local/lib/gstreamer-1.0".to_string(),
    ] {
        dirs.push(PathBuf::from(dir));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_plugin_in_any_listed_dir() {
        let tmp = env::temp_dir().join(format!("rekord-gst-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("libgstinter.so"), b"").unwrap();
        let dirs = vec![PathBuf::from("/nonexistent"), tmp.clone()];
        assert!(find_plugin(&dirs, "libgstinter.so"));
        assert!(!find_plugin(&dirs, "libgstaudiomixer.so"));
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
