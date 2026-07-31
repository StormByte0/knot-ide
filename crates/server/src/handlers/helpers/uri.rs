//! URI normalization.

use url::Url;

/// Normalize a file:// URI to a canonical form.
///
/// **Root cause of duplicate passage errors**: On Windows, `Url::from_file_path()`
/// produces URIs like `file:///d:/path` (unencoded colon in the drive letter),
/// while VS Code sends URIs like `file:///d%3A/path` (colon percent-encoded).
/// These are semantically equivalent but have different serializations, causing
/// `HashMap<Url, _>` to treat them as different keys. The same file then gets
/// stored twice — once from workspace indexing and once from `did_open` — each
/// containing the same passages, which triggers duplicate passage name errors.
///
/// The fix: convert file URIs to a file path and back, which produces a
/// consistent serialization regardless of the input encoding. Non-file URIs
/// are returned unchanged.
pub(crate) fn normalize_file_uri(uri: &Url) -> Url {
    // Only normalize file:// URIs
    if uri.scheme() != "file" {
        return uri.clone();
    }

    // Try to convert to a file path and back. This produces a consistent
    // URI encoding (e.g., `file:///d:/path` on Windows) regardless of
    // whether the input had percent-encoded colons (`%3A`) or not.
    match uri.to_file_path() {
        Ok(path) => match Url::from_file_path(&path) {
            Ok(normalized) => normalized,
            Err(_) => uri.clone(), // Fallback: return as-is
        },
        Err(_) => uri.clone(), // Not a valid file path — return as-is
    }
}

/// Check whether a file URI is located inside the workspace root.
///
/// Used by `did_open` to decide whether a newly-opened file should be
/// inserted into the workspace graph (Bug #4). Files outside the workspace
/// root are parsed for syntax highlighting and per-file LSP features, but
/// their passages do NOT enter the workspace graph — preventing an
/// unrelated `.twee` file opened for reference from polluting the project
/// with phantom passages (which would cause spurious "duplicate passage
/// name" diagnostics and stale broken-link reports).
///
/// Both `file_uri` and `workspace_root` must be `file://` URIs. If either
/// cannot be converted to a file path, returns `false` (treat as
/// out-of-workspace — safer default).
pub(crate) fn is_uri_in_workspace(file_uri: &Url, workspace_root: &Url) -> bool {
    // Both must be file:// URIs
    if file_uri.scheme() != "file" || workspace_root.scheme() != "file" {
        return false;
    }

    let file_path = match file_uri.to_file_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let root_path = match workspace_root.to_file_path() {
        Ok(p) => p,
        Err(_) => return false,
    };

    // `starts_with` on PathBuf returns true if `file_path` is `root_path`
    // itself or any descendant of it. This is exactly the "is inside the
    // workspace" check we need.
    file_path.starts_with(&root_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_file_uri_idempotent() {
        let uri = Url::parse("file:///home/user/project/story.tw").unwrap();
        let normalized = normalize_file_uri(&uri);
        assert_eq!(uri, normalized);
    }

    #[test]
    fn test_is_uri_in_workspace_self() {
        let root = Url::parse("file:///project/").unwrap();
        // A file directly under the root
        let f = Url::parse("file:///project/story.tw").unwrap();
        assert!(is_uri_in_workspace(&f, &root));
    }

    #[test]
    fn test_is_uri_in_workspace_subdir() {
        let root = Url::parse("file:///project/").unwrap();
        let f = Url::parse("file:///project/subdir/other.tw").unwrap();
        assert!(is_uri_in_workspace(&f, &root));
    }

    #[test]
    fn test_is_uri_in_workspace_outside() {
        let root = Url::parse("file:///project/").unwrap();
        let f = Url::parse("file:///other-project/reference.tw").unwrap();
        assert!(!is_uri_in_workspace(&f, &root));
    }

    #[test]
    fn test_is_uri_in_workspace_sibling_dir() {
        let root = Url::parse("file:///home/user/myproject/").unwrap();
        // A file in /home/user/otherproject/ — same parent, different project
        let f = Url::parse("file:///home/user/otherproject/x.tw").unwrap();
        assert!(!is_uri_in_workspace(&f, &root));
    }

    #[test]
    fn test_is_uri_in_workspace_non_file_scheme() {
        let root = Url::parse("file:///project/").unwrap();
        let f = Url::parse("untitled://Untitled-1").unwrap();
        assert!(!is_uri_in_workspace(&f, &root));
    }
}
