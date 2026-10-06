//! Process environment fixes for the Linux AppImage, applied first thing in
//! `run()`, before GTK, WebKit or any other thread starts.
//!
//! 1. **Native Wayland.** The AppImage's GTK hook exports `GDK_BACKEND=x11`
//!    unconditionally, so on a Wayland session the app ran under Xwayland.
//!    On a fractionally scaled desktop every frame then went through Xwayland
//!    and was rescaled by the compositor: measured on a 2× panel, Xwayland
//!    alone took 35% of a core while a track played. When a Wayland display
//!    exists we ask GTK for Wayland first (X11 as fallback). Opt out with
//!    `REKORD_GDK_BACKEND=x11` (or any value, used as `GDK_BACKEND`).
//!
//! 2. **No GStreamer rescan at every launch.** The AppImage's GStreamer hook
//!    points the plugin path into the random `/tmp/.mount_*` directory and
//!    disables plugin-scanner reuse, so GStreamer rebuilt its registry on
//!    every start (about 3 s with the old bundle). Here the plugin directory
//!    is reached through a stable symlink in the user's cache and the
//!    registry is a persistent file next to it, one per app version: the
//!    first launch of a version scans once, later launches read the cache
//!    (~15 ms). Opt out with `REKORD_GST_CACHE=0`.
//!
//! Outside an AppImage (the `.deb`, a source build) nothing changes.

use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub fn apply() {
    let Some(appdir) = env::var_os("APPDIR").filter(|_| env::var_os("APPIMAGE").is_some()) else {
        return;
    };
    prefer_wayland();
    cache_gstreamer_registry(Path::new(&appdir));
}

fn prefer_wayland() {
    if let Some(choice) = env::var_os("REKORD_GDK_BACKEND").filter(|v| !v.is_empty()) {
        env::set_var("GDK_BACKEND", choice);
        return;
    }
    if env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty()) {
        env::set_var("GDK_BACKEND", "wayland,x11");
    }
}

fn cache_gstreamer_registry(appdir: &Path) {
    if env::var_os("REKORD_GST_CACHE").is_some_and(|v| v == "0") {
        return;
    }
    let plugins = appdir.join("usr/lib/gstreamer-1.0");
    if !plugins.is_dir() {
        return;
    }
    let Some(dir) = cache_dir().map(|c| {
        c.join(format!(
            "gst-{}-{}",
            env!("CARGO_PKG_VERSION"),
            env::consts::ARCH
        ))
    }) else {
        return;
    };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let link = dir.join("plugins");
    // Another instance of this version may be running from its own mount:
    // its web process still loads plugins through the link, so it is only
    // retargeted when no instance holds the lock (the second launch exits
    // through the single-instance plugin anyway). The lock lives as long as
    // this process.
    let Ok(lock) = File::create(dir.join("instance.lock")) else {
        return;
    };
    if lock.try_lock().is_err() {
        return;
    }
    if !retarget(&link, &plugins) {
        return;
    }
    std::mem::forget(lock);
    env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", &link);
    env::set_var("GST_PLUGIN_PATH_1_0", &link);
    env::set_var("GST_REGISTRY_1_0", dir.join("registry.bin"));
    env::set_var("GST_REGISTRY_REUSE_PLUGIN_SCANNER", "yes");
}

/// Point `link` at `target`, replacing an older link atomically.
fn retarget(link: &Path, target: &Path) -> bool {
    if fs::read_link(link).is_ok_and(|current| current == target) {
        return true;
    }
    let tmp = link.with_extension("new");
    let _ = fs::remove_file(&tmp);
    if std::os::unix::fs::symlink(target, &tmp).is_err() {
        return false;
    }
    fs::rename(&tmp, link).is_ok()
}

fn cache_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("re-kord"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retarget_replaces_the_link() {
        let tmp = env::temp_dir().join(format!("rekord-env-{}", std::process::id()));
        let (a, b) = (tmp.join("a"), tmp.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        let link = tmp.join("plugins");
        assert!(retarget(&link, &a));
        assert_eq!(fs::read_link(&link).unwrap(), a);
        assert!(retarget(&link, &b));
        assert_eq!(fs::read_link(&link).unwrap(), b);
        assert!(retarget(&link, &b));
        fs::remove_dir_all(&tmp).unwrap();
    }
}
