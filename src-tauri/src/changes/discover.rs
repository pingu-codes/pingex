//! Which repositories a thread works in: its own directory, or every member
//! of its multi-project workspace (the hub itself is a folder of links, not a
//! repository). Each resolves to its worktree's top level, once.

use std::path::Path;

use turso::Database;

use crate::git::run::{run_git, READ_TIMEOUT};
use crate::storage;
use crate::util::host::Host;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DirSource {
    pub(super) dir: String,
    /// The workspace member alias, when the directory is one.
    pub(super) alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Repo {
    /// The worktree's top level, as a host path.
    pub(super) root: String,
    pub(super) label: String,
}

/// The directories to read for `thread_id`. `cwd`, when the caller knows it,
/// wins over the stored one (a Codex thread's summary can lag its start).
pub(super) async fn thread_dirs(
    database: &Database,
    thread_id: &str,
    cwd: Option<String>,
) -> Result<Vec<DirSource>, String> {
    if let Some(workspace) = storage::workspace_for_thread(database, thread_id).await? {
        let members = storage::read_workspace_members(database, &workspace).await?;
        return Ok(members
            .into_iter()
            .map(|member| DirSource {
                dir: member.effective_path,
                alias: Some(member.alias),
            })
            .collect());
    }
    let cwd = match cwd.filter(|cwd| !cwd.is_empty()) {
        Some(cwd) => Some(cwd),
        None => storage::read_thread_cwd(database, thread_id).await?,
    };
    Ok(cwd
        .map(|dir| vec![DirSource { dir, alias: None }])
        .unwrap_or_default())
}

/// Each directory's worktree top level, in order, without repeats. A
/// directory that is not in a repository (or no longer exists) is skipped.
pub(super) fn resolve_roots(host: &Host, dirs: &[DirSource]) -> Vec<Repo> {
    let mut repos: Vec<Repo> = Vec::new();
    for source in dirs {
        if !host.is_dir(&source.dir) {
            continue;
        }
        let Some(root) = top_level(host, &source.dir) else {
            continue;
        };
        if repos.iter().any(|repo| repo.root == root) {
            continue;
        }
        let label = source
            .alias
            .clone()
            .unwrap_or_else(|| folder_name(&root).to_string());
        repos.push(Repo { root, label });
    }
    repos
}

pub(super) fn top_level(host: &Host, dir: &str) -> Option<String> {
    let output = run_git(
        host,
        Path::new(dir),
        &["rev-parse", "--path-format=absolute", "--show-toplevel"],
        READ_TIMEOUT,
    )
    .ok()?;
    let root = output.stdout.trim();
    (output.ok && !root.is_empty()).then(|| root.to_string())
}

fn folder_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repo::TestRepo;

    #[test]
    fn subdirectories_resolve_to_one_root_and_plain_folders_are_skipped() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        let sub = fixture.repo.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let plain = tempfile::tempdir().unwrap();
        let dir = |path: &Path, alias: Option<&str>| DirSource {
            dir: path.to_str().unwrap().to_string(),
            alias: alias.map(str::to_string),
        };
        let repos = resolve_roots(
            &Host::Native,
            &[
                dir(plain.path(), None),
                dir(&sub, Some("api")),
                dir(&fixture.repo, None),
            ],
        );
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].label, "api");
        assert_eq!(
            std::fs::canonicalize(&repos[0].root).unwrap(),
            std::fs::canonicalize(&fixture.repo).unwrap()
        );
    }

    #[test]
    fn folder_names_ignore_trailing_separators() {
        assert_eq!(folder_name("/a/b/repo/"), "repo");
        assert_eq!(folder_name("C:\\code\\repo"), "repo");
    }
}
