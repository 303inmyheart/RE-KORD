//! yt-dlp / ffmpeg resolution: candidate order, newest version wins, ties go
//! to the higher-priority candidate, the app-data updated copy is preferred
//! once it is newer (studio QA item 1).

#![cfg(unix)]

use rekord_core::tools::{self, Candidate, Tool, ToolContext};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rekord-res-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A fake tool printing `version` for `--version`.
fn fake_tool(path: &Path, version: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\necho {version}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn candidate_order_puts_config_then_app_data_before_bundled_and_path() {
    let base = temp_dir("order");
    let config = base.join("custom/yt-dlp");
    fake_tool(&config, "2001.01.01");
    let data = base.join("data");
    fake_tool(&tools::app_data_ytdlp_path(&data), "2002.02.02");
    let ctx = ToolContext {
        config_path: Some(config.clone()),
        data_dir: Some(data.clone()),
    };
    let list = tools::candidates(Tool::Ytdlp, &ctx);
    assert_eq!(list[0].path, config);
    assert_eq!(list[0].source, "config");
    let app = list.iter().position(|c| c.source == "app_data").unwrap();
    for (i, c) in list.iter().enumerate() {
        if matches!(
            c.source,
            "bundled" | "dev" | "tools_dir" | "path" | "legacy"
        ) {
            assert!(i > app, "{} before app_data", c.source);
        }
    }
    if let Some(p) = list.iter().position(|c| c.source == "path") {
        assert_eq!(p, list.len() - 1, "PATH comes last");
    }
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn newest_version_wins_and_app_data_update_is_preferred() {
    let base = temp_dir("newest");
    let config = base.join("custom/yt-dlp");
    // Older than any real copy (bundled dev 2026.08.19, system PATH): loses.
    fake_tool(&config, "2000.01.01");
    let data = base.join("data");
    let ctx = ToolContext {
        config_path: Some(config.clone()),
        data_dir: Some(data.clone()),
    };
    let before = tools::resolve_blocking(Tool::Ytdlp, &ctx);
    if before.candidates.len() > 1 {
        assert_ne!(before.path, config, "an old configured copy must not win");
    }

    // "Update yt-dlp" installed a newer copy: preferred after invalidation.
    fake_tool(&tools::app_data_ytdlp_path(&data), "2099.12.31");
    tools::invalidate(Tool::Ytdlp);
    let after = tools::resolve_blocking(Tool::Ytdlp, &ctx);
    assert_eq!(after.source, Some("app_data"));
    assert_eq!(after.version.as_deref(), Some("2099.12.31"));
    assert_eq!(after.path, tools::app_data_ytdlp_path(&data));

    // Same version in config and app data: config (higher priority) wins.
    fake_tool(&config, "2099.12.31");
    tools::invalidate(Tool::Ytdlp);
    let tie = tools::resolve_blocking(Tool::Ytdlp, &ctx);
    assert_eq!(tie.source, Some("config"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn broken_candidates_are_skipped() {
    let base = temp_dir("broken");
    let good = base.join("good/yt-dlp");
    fake_tool(&good, "2010.10.10");
    let bad = base.join("bad/yt-dlp");
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "#!/bin/sh\nexit 3\n").unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();
    let r = tools::resolve_with(
        Tool::Ytdlp,
        vec![
            Candidate {
                path: bad.clone(),
                source: "config",
            },
            Candidate {
                path: good.clone(),
                source: "path",
            },
        ],
        |p| tools::probe_version_blocking(Tool::Ytdlp, p),
    );
    assert!(r.available);
    assert_eq!(r.path, good);
    assert!(!r.candidates[0].runs);
    let none = tools::resolve_with(Tool::Ytdlp, vec![], |_| None);
    assert!(!none.available);
    assert_eq!(none.path, PathBuf::from("yt-dlp"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn ffmpeg_versions_compare_numerically() {
    use std::cmp::Ordering;
    assert_eq!(
        tools::compare_versions(
            Some("ffmpeg version n8.1.3-14-g330caae0c1"),
            Some("ffmpeg version 6.1.1-3ubuntu5")
        ),
        Ordering::Greater
    );
    assert_eq!(
        tools::compare_versions(Some("2026.08.19"), Some("2026.8.19")),
        Ordering::Equal
    );
    assert_eq!(
        tools::compare_versions(Some("2026.09.30"), Some("2026.08.19.232423")),
        Ordering::Greater
    );
}
