//! A tiny on-disk cache of the client's static SVGs (positions and rank
//! crests), so a restart without League or CommunityDragon still shows icons.
//!
//! Both [`super::roles`] and [`super::ranks`] fetch the same plugin's assets
//! from the LCU first and CommunityDragon second. When neither answers — which
//! happens in-game, and while the mirror is down — the last good bytes are read
//! from `%LOCALAPPDATA%\Swapper\icons` instead of rendering a blank. Only names
//! on the existing whitelists are ever turned into a path, so no caller-supplied
//! string can escape the icons folder.
//!
//! Every operation is best-effort: a missing, unreadable, empty or oversized
//! file reads as a miss, and a failed write is ignored, because this is a cache
//! and must never block the feature.

use std::path::{Path, PathBuf};

/// Which asset family a cached file belongs to. The variant also selects the
/// on-disk subfolder and the whitelist that is re-checked here, so an
/// unvalidated name can never name a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Role,
    Rank,
}

impl Kind {
    fn folder(self) -> &'static str {
        match self {
            Kind::Role => "role",
            Kind::Rank => "rank",
        }
    }

    /// The asset names this kind may cache. These mirror the caller's
    /// `asset_name` whitelist; re-checking here keeps the guarantee that only a
    /// known name reaches the disk.
    fn allows(self, name: &str) -> bool {
        match self {
            Kind::Role => matches!(name, "top" | "jungle" | "middle" | "bottom" | "utility"),
            Kind::Rank => matches!(
                name,
                "unranked" | "gold" | "platinum" | "emerald" | "diamond" | "master" | "challenger"
            ),
        }
    }
}

/// A cached file larger than this is ignored: a valid crest or position SVG is
/// only a few kilobytes.
const MAX_FILE_BYTES: u64 = 512 * 1024;

/// The on-disk path for a whitelisted asset, or `None` when `name` is not on
/// the whitelist or `LOCALAPPDATA` is unavailable (the cache is then skipped).
pub(crate) fn path(kind: Kind, name: &str) -> Option<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")?;
    path_in(Path::new(&root), kind, name)
}

/// Builds the path under an explicit root. Split out so a test can pin the
/// whitelist and round-trip a file without touching the real data folder.
fn path_in(root: &Path, kind: Kind, name: &str) -> Option<PathBuf> {
    if !kind.allows(name) {
        return None;
    }
    Some(
        root.join("Swapper")
            .join("icons")
            .join(kind.folder())
            .join(format!("{name}.svg")),
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
}
