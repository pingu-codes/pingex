use serde::Serialize;
use std::path::Path;

use crate::util::walk::{walker, MAX_WALKED_FILES};

#[derive(Debug, Clone, Serialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileHit {
    /// Path relative to the searched root.
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) score: i64,
    pub(crate) is_dir: bool,
}

/// Case-insensitive subsequence match of `query` against `candidate`.
/// Returns a score where higher is better, or None when `query` is not a
/// subsequence of `candidate`. Contiguous runs, word-boundary hits, and
/// matches close to the end of the path (the file name) score higher.
pub(crate) fn score(query: &str, candidate: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let candidate_lower: Vec<char> = candidate.to_lowercase().chars().collect();
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    if query_lower.len() > candidate_lower.len() {
        return None;
    }
    let mut total: i64 = 0;
    let mut candidate_index = 0usize;
    let mut previous_match: Option<usize> = None;
    for &query_char in &query_lower {
        let mut found = None;
        while candidate_index < candidate_lower.len() {
            if candidate_lower[candidate_index] == query_char {
                found = Some(candidate_index);
                break;
            }
            candidate_index += 1;
        }
        let index = found?;
        let mut gained = 1;
        if previous_match == Some(index.wrapping_sub(1)) {
            gained += 8;
        }
        let at_boundary =
            index == 0 || matches!(candidate_lower[index - 1], '/' | '_' | '-' | '.' | ' ');
        if at_boundary {
            gained += 6;
        }
        total += gained;
        previous_match = Some(index);
        candidate_index = index + 1;
    }
    // Prefer shorter candidates and matches that land in the trailing
    // (file-name) portion of the path.
    total -= (candidate_lower.len() as i64) / 8;
    if let Some(last) = previous_match {
        let tail = candidate_lower.len() - 1 - last;
        total -= (tail as i64) / 4;
    }
    Some(total)
}

/// Walk `root` respecting .gitignore (via the `ignore` crate) and return the
/// best-matching files and folders for `query`, sorted by descending score.
pub(crate) fn search_files(root: &Path, query: &str, limit: usize) -> Vec<FileHit> {
    let mut hits: Vec<FileHit> = Vec::new();
    let mut walked = 0usize;
    for entry in walker(root) {
        let Ok(entry) = entry else { continue };
        if walked >= MAX_WALKED_FILES {
            break;
        }
        walked += 1;
        // The root itself is not a useful mention target.
        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
        if entry.depth() == 0 || !(is_dir || entry.file_type().is_some_and(|kind| kind.is_file())) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().to_string();
        let Some(match_score) = score(query, &relative) else {
            continue;
        };
        let file_name = entry.file_name().to_string_lossy().to_string();
        hits.push(FileHit {
            path: relative,
            file_name,
            score: match_score,
            is_dir,
        });
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    hits.truncate(limit);
    hits
}

/// Walk `root` respecting .gitignore and return every file's path relative to
/// `root`, sorted, capped at MAX_WALKED_FILES entries.
pub(crate) fn list_files(root: &Path) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let mut walked = 0usize;
    for entry in walker(root) {
        let Ok(entry) = entry else { continue };
        if walked >= MAX_WALKED_FILES {
            break;
        }
        walked += 1;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        paths.push(relative.to_string_lossy().to_string());
    }
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scores_subsequence_matches_only() {
        assert!(score("cmps", "src/lib/Composer.svelte").is_some());
        assert!(score("zzz", "src/lib/Composer.svelte").is_none());
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn prefers_contiguous_and_boundary_matches() {
        let exact = score("composer", "src/lib/Composer.svelte").unwrap();
        let scattered = score("composer", "src/common/deep/pto/setter.rs").unwrap_or(i64::MIN);
        assert!(exact > scattered);
        let boundary = score("api", "src/lib/api.ts").unwrap();
        let embedded = score("api", "src/lib/capitals.ts").unwrap();
        assert!(boundary > embedded);
    }

    #[test]
    fn search_respects_gitignore() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        fs::write(root.join("src/main.ts"), "").unwrap();
        fs::write(root.join("node_modules/pkg/main.ts"), "").unwrap();
        // The ignore crate only applies .gitignore inside git repositories.
        fs::create_dir_all(root.join(".git")).unwrap();

        let hits = search_files(root, "main", 10);
        let paths: Vec<_> = hits.iter().map(|hit| hit.path.as_str()).collect();
        assert!(
            paths.contains(&"src/main.ts"),
            "expected src/main.ts in {paths:?}"
        );
        assert!(
            !paths.iter().any(|path| path.contains("node_modules")),
            "gitignored files leaked into {paths:?}"
        );
    }

    #[test]
    fn list_files_returns_sorted_relative_paths_respecting_gitignore() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("src/lib")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        fs::write(root.join("src/lib/api.ts"), "").unwrap();
        fs::write(root.join("src/main.ts"), "").unwrap();
        fs::write(root.join("README.md"), "").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "").unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::write(root.join(".github/workflows/ci.yml"), "").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "").unwrap();

        let paths = list_files(root);
        assert_eq!(
            paths,
            vec![
                ".github/workflows/ci.yml",
                ".gitignore",
                "README.md",
                "src/lib/api.ts",
                "src/main.ts"
            ]
        );
    }

    #[test]
    fn search_includes_folders_but_not_the_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("src/components")).unwrap();
        fs::write(root.join("src/components/Button.svelte"), "").unwrap();

        let hits = search_files(root, "components", 10);
        let folder = hits
            .iter()
            .find(|hit| hit.path == "src/components")
            .expect("folder should be searchable");
        assert!(folder.is_dir);
        assert_eq!(folder.file_name, "components");
        assert!(hits.iter().all(|hit| !hit.path.is_empty()));

        let all = search_files(root, "", 10);
        assert!(all.iter().any(|hit| hit.path == "src" && hit.is_dir));
        assert!(all
            .iter()
            .any(|hit| hit.path == "src/components/Button.svelte" && !hit.is_dir));
    }

    #[test]
    fn search_orders_by_score_and_respects_limit() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/api.ts"), "").unwrap();
        fs::write(root.join("src/apple-pie.md"), "").unwrap();
        fs::write(root.join("README.md"), "").unwrap();

        let hits = search_files(root, "api", 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "src/api.ts");
    }
}
