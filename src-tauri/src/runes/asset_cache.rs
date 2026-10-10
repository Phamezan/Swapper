//! A tiny on-disk cache of the client's static SVGs (positions and rank
//! crests) and the pro teams' logo PNGs, so a restart without League or
//! CommunityDragon still shows icons.
//!
//! [`super::roles`] and [`super::ranks`] fetch the same plugin's assets from the
//! LCU first and CommunityDragon second; [`super::teams`] fetches from the
//! probuildstats CDN. When neither answers — which happens in-game, and while a
//! mirror is down — the last good bytes are read from
//! `%LOCALAPPDATA%\Swapper\icons` instead of rendering a blank.
//!
//! Roles and ranks are a fixed whitelist, so no caller-supplied string can name
//! a file. Team names are not, so their on-disk stem is sanitized to alphanumeric
//! characters and capped, and an empty result is rejected. Either way, only a
//! validated stem is ever turned into a path.
//!
//! Every operation is best-effort: a missing, unreadable, empty or oversized
//! file reads as a miss, and a failed write is ignored, because this is a cache
//! and must never block the feature.

use std::path::{Path, PathBuf};

/// The longest cached team stem. Real slugs are short; this is just a bound.
const MAX_TEAM_STEM: usize = 64;

/// Which asset family a cached file belongs to. The variant also selects the
/// on-disk subfolder and the validation applied to the name, so an unvalidated
/// name can never name a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Role,
    Rank,
    Team,
}

impl Kind {
    fn folder(self) -> &'static str {
        match self {
            Kind::Role => "role",
            Kind::Rank => "rank",
            Kind::Team => "team",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Kind::Team => "png",
            _ => "svg",
        }
    }

    /// The on-disk file stem for `name`, or `None` when the name may not be
    /// cached. Roles and ranks are a fixed whitelist; a team slug is sanitized
    /// so no caller string reaches the path unsanitised.
    fn file_stem(self, name: &str) -> Option<String> {
        match self {
            Kind::Role => matches!(name, "top" | "jungle" | "middle" | "bottom" | "utility")
                .then(|| name.to_string()),
            Kind::Rank => matches!(
                name,
                "unranked" | "gold" | "platinum" | "emerald" | "diamond" | "master" | "challenger"
            )
            .then(|| name.to_string()),
            Kind::Team => team_stem(name),
        }
    }
}

/// A safe on-disk stem for a team slug: alphanumeric characters only (accents
/// are alphanumeric in Unicode, so they survive), capped, and never empty.
fn team_stem(name: &str) -> Option<String> {
    let stem: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(MAX_TEAM_STEM)
        .collect();
    (!stem.is_empty()).then_some(stem)
}

/// A cached file larger than this is ignored: a valid crest or position SVG is
/// only a few kilobytes.
const MAX_FILE_BYTES: u64 = 512 * 1024;

/// The on-disk path for a cacheable asset, or `None` when `name` may not be
/// cached or `LOCALAPPDATA` is unavailable (the cache is then skipped).
pub(crate) fn path(kind: Kind, name: &str) -> Option<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")?;
    path_in(Path::new(&root), kind, name)
}

/// Builds the path under an explicit root. Split out so a test can pin the
/// validation and round-trip a file without touching the real data folder.
fn path_in(root: &Path, kind: Kind, name: &str) -> Option<PathBuf> {
    let stem = kind.file_stem(name)?;
    Some(
        root.join("Swapper")
            .join("icons")
            .join(kind.folder())
            .join(format!("{stem}.{}", kind.extension())),
    )
}

/// The last good bytes written for this asset, if any.
pub(crate) fn read(kind: Kind, name: &str) -> Option<Vec<u8>> {
    load(&path(kind, name)?)
}

/// Persists freshly fetched bytes. Failures are ignored, because the fetch that
/// produced them already succeeded and the icon is already on screen.
pub(crate) fn write(kind: Kind, name: &str, bytes: &[u8]) {
    if let Some(path) = path(kind, name) {
        store(&path, bytes);
    }
}

fn load(path: &Path) -> Option<Vec<u8>> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

fn store(path: &Path, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let _ = std::fs::write(path, bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("swapper-icons-{tag}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn whitelisted_name_round_trips_on_disk() {
        let root = temp_root("roundtrip");
        let path = path_in(&root, Kind::Role, "jungle").expect("whitelisted role");
        store(&path, b"<svg>jungle</svg>");
        assert_eq!(load(&path).as_deref(), Some(&b"<svg>jungle</svg>"[..]));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_or_traversing_names_never_resolve_to_a_path() {
        let root = Path::new("C:\\icons");
        assert!(path_in(root, Kind::Role, "top").is_some());
        assert!(path_in(root, Kind::Rank, "emerald").is_some());
        // No arbitrary paths or traversal survive.
        assert!(path_in(root, Kind::Role, "../../secret").is_none());
        assert!(path_in(root, Kind::Rank, "bogus").is_none());
        assert!(path_in(root, Kind::Role, "").is_none());
    }

    #[test]
    fn team_slugs_are_sanitized_to_a_safe_stem() {
        let root = Path::new("C:\\icons");
        // Punctuation and separators are dropped; the stem stays inside the folder.
        assert_eq!(
            path_in(root, Kind::Team, "gen.g").unwrap().file_name().unwrap(),
            "geng.png"
        );
        assert_eq!(
            path_in(root, Kind::Team, "kabum!eports").unwrap().file_name().unwrap(),
            "kabumeports.png"
        );
        // Accents are alphanumeric, so they survive the sanitizer.
        assert_eq!(
            path_in(root, Kind::Team, "movistarkoifénix").unwrap().file_name().unwrap(),
            "movistarkoifénix.png"
        );
        // Traversal and separators are stripped, so the file stays in the team
        // folder with a plain alphanumeric name.
        let traversing = path_in(root, Kind::Team, "../../secret").expect("sanitized, not rejected");
        assert_eq!(traversing.file_name().unwrap(), "secret.png");
        assert_eq!(
            traversing.parent().unwrap(),
            root.join("Swapper").join("icons").join("team")
        );
        assert_eq!(
            path_in(root, Kind::Team, "..\\..\\secret").unwrap().file_name().unwrap(),
            "secret.png"
        );
        // A slug with nothing alphanumeric left is rejected outright.
        assert!(path_in(root, Kind::Team, "").is_none());
        assert!(path_in(root, Kind::Team, "...").is_none());
        // The stem is capped so an absurd name cannot create a huge filename.
        let long = path_in(root, Kind::Team, &"a".repeat(500)).unwrap();
        assert_eq!(long.file_name().unwrap().to_string_lossy().len(), MAX_TEAM_STEM + 4);
    }
}
