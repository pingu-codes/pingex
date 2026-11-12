//! Split one repository's changes into Change segments, for either scope.
//!
//! Branch scope picks each git branch's base the way its pull request would:
//! a stack's PRs each against the PR below (the bottom one against the stack's
//! base), any other branch against its PR's base, else the remote's default
//! branch. Thread scope compares every branch tip with the one recorded when
//! the thread started. Both end with the working tree.

use std::path::{Path, PathBuf};

use super::listing::{committed_listing, working_listing, Listing};
use super::refs::{
    ancestor, current_branch, empty_tree, has_commit, head_sha, in_progress, merge_base, read_refs,
    Refs,
};
use super::{ChangeSegment, SegmentKind};
use crate::git::remotes::read_remotes;
use crate::review::worktree_purpose::WorktreePurpose;
use crate::storage::TipSnapshot;
use crate::util::host::Host;

/// What git says about a repository right now.
#[derive(Clone, Debug)]
pub(super) struct RepoState {
    pub(super) root: PathBuf,
    pub(super) branch: Option<String>,
    pub(super) head_sha: Option<String>,
    pub(super) in_progress: Option<String>,
    pub(super) refs: Refs,
    /// `(remote, owner/name)` for each remote on GitHub.
    pub(super) slugs: Vec<(String, String)>,
}

/// The PR a git branch belongs to, as the inbox last read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrHint {
    pub(super) number: i64,
    pub(super) base_ref: String,
    /// The remote the PR's repository is.
    pub(super) remote: Option<String>,
}

/// What Pingex knows about the repository, from its own database.
#[derive(Clone, Debug, Default)]
pub(super) struct RepoFacts {
    pub(super) purpose: Option<WorktreePurpose>,
    pub(super) pr: Option<PrHint>,
    pub(super) snapshot: Option<TipSnapshot>,
    /// The snapshot names only some branches (made from a PR link's start
    /// commit), so a branch it lacks is not new.
    pub(super) partial_snapshot: bool,
}

pub(super) fn inspect(host: &Host, root: &str) -> Result<RepoState, String> {
    let path = PathBuf::from(root);
    let refs = read_refs(host, &path)?;
    let slugs = read_remotes(host, &path)
        .map(|remotes| {
            remotes
                .into_iter()
                .filter_map(|remote| remote.slug.map(|slug| (remote.name, slug)))
                .collect()
        })
        .unwrap_or_default();
    Ok(RepoState {
        branch: current_branch(host, &path),
        head_sha: head_sha(host, &path),
        in_progress: in_progress(host, &path),
        refs,
        slugs,
        root: path,
    })
}

fn segment(
    id: String,
    kind: SegmentKind,
    label: String,
    base_label: String,
    base_sha: String,
    head_sha: Option<String>,
) -> ChangeSegment {
    ChangeSegment {
        id,
        kind,
        label,
        branch: None,
        pr_number: None,
        base_label,
        base_sha,
        head_sha,
        current: false,
        rewritten: false,
        needs_restack: false,
        note: None,
        files: Vec::new(),
        total_files: 0,
        truncated: false,
        additions: 0,
        deletions: 0,
    }
}

fn fill(mut segment: ChangeSegment, listing: Result<Listing, String>) -> ChangeSegment {
    match listing {
        Ok(listing) => {
            segment.files = listing.files;
            segment.total_files = listing.total_files;
            segment.truncated = listing.truncated;
            segment.additions = listing.additions;
            segment.deletions = listing.deletions;
        }
        Err(error) => segment.note = Some(error),
    }
    segment
}

fn committed(host: &Host, root: &Path, segment: ChangeSegment) -> ChangeSegment {
    let Some(head) = segment.head_sha.clone() else {
        return segment;
    };
    let listing = committed_listing(host, root, &segment.base_sha, &head);
    fill(segment, listing)
}

/// The working tree against `HEAD` (the empty tree before the first commit),
/// or `None` when it is clean.
fn uncommitted(host: &Host, state: &RepoState) -> Option<ChangeSegment> {
    let (base, base_label) = match &state.head_sha {
        Some(sha) => (sha.clone(), "HEAD".to_string()),
        None => (
            empty_tree(host, &state.root).ok()?,
            "nothing yet".to_string(),
        ),
    };
    let mut segment = segment(
        "uncommitted".into(),
        SegmentKind::Uncommitted,
        "Uncommitted".into(),
        base_label,
        base.clone(),
        None,
    );
    segment.branch = state.branch.clone();
    segment.current = true;
    let segment = fill(segment, working_listing(host, &state.root, &base));
    (segment.total_files > 0 || segment.note.is_some()).then_some(segment)
}

/// Put the uncommitted segment after the checked-out branch's, else last.
fn with_uncommitted(
    mut segments: Vec<ChangeSegment>,
    uncommitted: Option<ChangeSegment>,
) -> Vec<ChangeSegment> {
    let Some(uncommitted) = uncommitted else {
        return segments;
    };
    let at = segments
        .iter()
        .position(|segment| segment.current)
        .map(|index| index + 1)
        .unwrap_or(segments.len());
    segments.insert(at, uncommitted);
    segments
}

fn stack_prs(
    facts: &RepoFacts,
) -> Option<(&str, &[crate::review::worktree_purpose::StackMarkerPr])> {
    match &facts.purpose {
        Some(WorktreePurpose::Stack { base_ref, prs, .. }) => Some((base_ref.as_str(), prs)),
        _ => None,
    }
}

pub(super) fn branch_segments(
    host: &Host,
    state: &RepoState,
    facts: &RepoFacts,
) -> Vec<ChangeSegment> {
    let committed_segments = match stack_prs(facts) {
        Some((base_ref, prs)) => stack_segments(host, state, base_ref, prs),
        None => branch_segment(host, state, facts).into_iter().collect(),
    };
    with_uncommitted(committed_segments, uncommitted(host, state))
}

/// One segment per PR of the stack, bottom first, read from refs so every
/// PR shows whichever git branch is checked out.
fn stack_segments(
    host: &Host,
    state: &RepoState,
    base_ref: &str,
    prs: &[crate::review::worktree_purpose::StackMarkerPr],
) -> Vec<ChangeSegment> {
    let refs = &state.refs;
    let remote = refs.preferred_remote(state.branch.as_deref());
    let mut below: Option<(String, String, bool)> = refs
        .base_tip(remote.as_deref(), base_ref)
        .map(|(label, sha)| (label, sha, true));
    let mut segments = Vec::new();
    for pr in prs {
        let Some(tip) = refs.branch_tip(remote.as_deref(), &pr.head_ref) else {
            continue;
        };
        let (base_label, base_sha, needs_restack) = match &below {
            Some((label, below_tip, bottom)) => (
                label.clone(),
                merge_base(host, &state.root, below_tip, &tip),
                !bottom && !ancestor(host, &state.root, below_tip, &tip),
            ),
            None => (base_ref.to_string(), None, false),
        };
        let mut built = segment(
            format!("pr:{}", pr.number),
            SegmentKind::StackPr,
            format!("#{} {}", pr.number, pr.title),
            base_label,
            base_sha.clone().unwrap_or_default(),
            Some(tip.clone()),
        );
        built.branch = Some(pr.head_ref.clone());
        built.pr_number = Some(pr.number);
        built.current = state.branch.as_deref() == Some(pr.head_ref.as_str());
        built.needs_restack = needs_restack;
        built = match base_sha {
            Some(_) => committed(host, &state.root, built),
            None => {
                built.note = Some(format!("Could not find where {} starts", pr.head_ref));
                built
            }
        };
        segments.push(built);
        below = Some((format!("#{}", pr.number), tip, false));
    }
    segments
}

/// The checked-out git branch against its PR's base, else the default
/// branch; on the default branch itself, what it has that its upstream lacks.
fn branch_segment(host: &Host, state: &RepoState, facts: &RepoFacts) -> Option<ChangeSegment> {
    let refs = &state.refs;
    let head = state.head_sha.clone()?;
    let branch = state.branch.clone();
    let remote = facts
        .pr
        .as_ref()
        .and_then(|pr| pr.remote.clone())
        .or_else(|| refs.preferred_remote(branch.as_deref()));
    let (base_label, base_tip, unpushed) = match &facts.pr {
        Some(pr) => {
            let (label, sha) = refs.base_tip(remote.as_deref(), &pr.base_ref)?;
            (label, sha, false)
        }
        None => {
            let (short, label, sha) = refs.default_branch(remote.as_deref())?;
            if branch.as_deref() == Some(short.as_str()) {
                let upstream = refs.upstream.get(&short)?;
                let sha = refs.remotes.get(upstream)?.clone();
                (upstream.clone(), sha, true)
            } else {
                (label, sha, false)
            }
        }
    };
    let base_sha = merge_base(host, &state.root, &base_tip, &head)?;
    let name = branch
        .clone()
        .unwrap_or_else(|| "Detached HEAD".to_string());
    let label = match (&facts.pr, unpushed) {
        (Some(pr), _) => format!("{name} · #{}", pr.number),
        (None, true) => format!("Unpushed on {name}"),
        (None, false) => name.clone(),
    };
    let mut built = segment(
        format!("branch:{name}"),
        SegmentKind::Branch,
        label,
        base_label,
        base_sha,
        Some(head),
    );
    built.branch = branch;
    built.pr_number = facts.pr.as_ref().map(|pr| pr.number);
    built.current = true;
    let built = committed(host, &state.root, built);
    (built.total_files > 0 || facts.pr.is_some() || built.note.is_some()).then_some(built)
}

/// Branches that moved, appeared or went away since the start tips, then
/// the working tree. `None` when there are no start tips to compare with.
pub(super) fn thread_segments(
    host: &Host,
    state: &RepoState,
    facts: &RepoFacts,
) -> Option<Vec<ChangeSegment>> {
    let snapshot = facts.snapshot.as_ref()?;
    let root = &state.root;
    let pr_of = |name: &str| -> Option<i64> {
        if let Some((_, prs)) = stack_prs(facts) {
            if let Some(pr) = prs.iter().find(|pr| pr.head_ref == name) {
                return Some(pr.number);
            }
        }
        (state.branch.as_deref() == Some(name))
            .then(|| facts.pr.as_ref().map(|pr| pr.number))
            .flatten()
    };
    let label_of = |name: &str| match pr_of(name) {
        Some(number) => format!("{name} · #{number}"),
        None => name.to_string(),
    };
    let mut segments = Vec::new();
    let mut names: Vec<String> = snapshot
        .branches
        .iter()
        .filter_map(|(refname, _)| refname.strip_prefix("refs/heads/").map(str::to_string))
        .collect();
    if !facts.partial_snapshot {
        names.extend(state.refs.heads.keys().cloned());
    }
    names.sort();
    names.dedup();
    for name in names {
        let old = snapshot
            .branches
            .iter()
            .find(|(refname, _)| refname.strip_prefix("refs/heads/") == Some(name.as_str()))
            .map(|(_, sha)| sha.clone());
        let new = state.refs.heads.get(&name).cloned();
        let built = match (old, new) {
            (Some(old), Some(new)) if old == new => continue,
            (Some(old), Some(new)) => {
                let mut built = segment(
                    format!("moved:{name}"),
                    SegmentKind::Moved,
                    label_of(&name),
                    "tip at thread start".into(),
                    old.clone(),
                    Some(new.clone()),
                );
                if has_commit(host, root, &old) {
                    built.rewritten = !ancestor(host, root, &old, &new);
                    committed(host, root, built)
                } else {
                    built.note = Some("The commit this thread started from is gone".into());
                    built
                }
            }
            (None, Some(new)) => {
                let Some(start) = snapshot.head_sha.as_deref() else {
                    continue;
                };
                let Some(base) = merge_base(host, root, &new, start) else {
                    continue;
                };
                if base == new {
                    continue;
                }
                let built = segment(
                    format!("new:{name}"),
                    SegmentKind::NewBranch,
                    label_of(&name),
                    "where it branched".into(),
                    base,
                    Some(new),
                );
                committed(host, root, built)
            }
            (Some(old), None) => {
                let mut built = segment(
                    format!("deleted:{name}"),
                    SegmentKind::DeletedBranch,
                    label_of(&name),
                    "tip at thread start".into(),
                    old.clone(),
                    Some(old),
                );
                built.note = Some("Deleted since the thread started".into());
                built
            }
            (None, None) => continue,
        };
        let mut built = built;
        built.branch = Some(name.clone());
        built.pr_number = pr_of(&name);
        built.current = state.branch.as_deref() == Some(name.as_str());
        segments.push(built);
    }
    // The checked-out branch first, then the rest by name.
    segments.sort_by_key(|segment| !segment.current);
    let uncommitted = uncommitted(host, state).map(|mut segment| {
        for file in &mut segment.files {
            file.before_thread = snapshot.dirty_paths.contains(&file.path);
        }
        segment
    });
    Some(with_uncommitted(segments, uncommitted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repo::TestRepo;
    use crate::review::worktree_purpose::StackMarkerPr;

    fn read(fixture: &TestRepo) -> RepoState {
        inspect(&Host::Native, fixture.repo.to_str().unwrap()).unwrap()
    }

    fn ids(segments: &[ChangeSegment]) -> Vec<&str> {
        segments.iter().map(|segment| segment.id.as_str()).collect()
    }

    fn paths(segment: &ChangeSegment) -> Vec<&str> {
        segment
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect()
    }

    /// The regression the old panel had: once the agent commits on the main
    /// worktree, the branch still shows what it changed.
    #[test]
    fn committed_work_stays_visible_against_the_default_branch() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.branch("feature");
        fixture.commit("src/a.rs", "fn a() {}\n", "work");
        let state = read(&fixture);
        let segments = branch_segments(&Host::Native, &state, &RepoFacts::default());
        assert_eq!(ids(&segments), vec!["branch:feature"]);
        assert_eq!(segments[0].base_label, "origin/main");
        assert_eq!(paths(&segments[0]), vec!["src/a.rs"]);
        assert!(segments[0].current);
    }

    /// A linked worktree compares with the default branch, not with whatever
    /// the main worktree has checked out.
    #[test]
    fn a_linked_worktree_ignores_the_main_worktrees_branch() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.branch("other");
        fixture.commit("other.txt", "x\n", "other work");
        let linked = fixture.repo.parent().unwrap().join("linked");
        fixture.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            linked.to_str().unwrap(),
            "main",
        ]);
        std::fs::write(linked.join("f.txt"), "f\n").unwrap();
        fixture.git_in(&linked, &["add", "f.txt"]);
        fixture.git_in(&linked, &["commit", "-q", "-m", "feature"]);
        let state = inspect(&Host::Native, linked.to_str().unwrap()).unwrap();
        let segments = branch_segments(&Host::Native, &state, &RepoFacts::default());
        assert_eq!(paths(&segments[0]), vec!["f.txt"]);
    }

    #[test]
    fn the_default_branch_shows_what_it_has_not_pushed() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.git(&["branch", "-q", "--set-upstream-to=origin/main"]);
        let state = read(&fixture);
        assert!(branch_segments(&Host::Native, &state, &RepoFacts::default()).is_empty());
        fixture.commit("local.txt", "1\n", "local");
        let segments = branch_segments(&Host::Native, &read(&fixture), &RepoFacts::default());
        assert_eq!(segments[0].label, "Unpushed on main");
        assert_eq!(paths(&segments[0]), vec!["local.txt"]);
    }

    /// Untracked files and files in subdirectories all come back relative to
    /// the repository's top level, with literal names.
    #[test]
    fn the_working_tree_lists_root_relative_paths() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        std::fs::create_dir_all(fixture.repo.join("sub")).unwrap();
        std::fs::write(fixture.repo.join("README.md"), "changed\n").unwrap();
        std::fs::write(fixture.repo.join("sub/new.txt"), "n\n").unwrap();
        std::fs::write(fixture.repo.join("sub/[id].ts"), "x\n").unwrap();
        std::fs::write(fixture.repo.join("café.txt"), "c\n").unwrap();
        // Read from the top level, as the command resolves it.
        let root = super::super::discover::top_level(
            &Host::Native,
            fixture.repo.join("sub").to_str().unwrap(),
        )
        .unwrap();
        let state = inspect(&Host::Native, &root).unwrap();
        let segments = branch_segments(&Host::Native, &state, &RepoFacts::default());
        assert_eq!(ids(&segments), vec!["uncommitted"]);
        let mut listed = paths(&segments[0]);
        listed.sort();
        assert_eq!(
            listed,
            vec!["README.md", "café.txt", "sub/[id].ts", "sub/new.txt"]
        );
        assert!(segments[0]
            .files
            .iter()
            .all(|file| file.content_key.is_some()));

        let diff = super::super::listing::working_diff(
            &Host::Native,
            &state.root,
            state.head_sha.as_deref().unwrap(),
        )
        .unwrap();
        let mut diffed: Vec<&str> = diff.files.iter().map(|file| file.path.as_str()).collect();
        diffed.sort();
        assert_eq!(diffed, listed);
        assert!(diff.files.iter().all(|file| !file.hunks.is_empty()));
    }

    fn stack(prs: &[(i64, &str)]) -> RepoFacts {
        RepoFacts {
            purpose: Some(WorktreePurpose::Stack {
                repo_dir: "/repo".into(),
                number: 1,
                name: "billing".into(),
                base_ref: "main".into(),
                prs: prs
                    .iter()
                    .map(|(number, head_ref)| StackMarkerPr {
                        number: *number,
                        title: format!("PR {number}"),
                        head_ref: head_ref.to_string(),
                    })
                    .collect(),
                local_stack: false,
            }),
            ..RepoFacts::default()
        }
    }

    /// Every PR of a stack shows against the one below, whichever is checked
    /// out, and the working tree sits after the checked-out one.
    #[test]
    fn a_stack_splits_by_pr_and_flags_a_needed_restack() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.branch("a");
        fixture.commit("a.txt", "a\n", "a");
        fixture.branch("b");
        fixture.commit("b.txt", "b\n", "b");
        fixture.branch("c");
        fixture.commit("c.txt", "c\n", "c");
        fixture.switch("b");
        std::fs::write(fixture.repo.join("dirty.txt"), "d\n").unwrap();
        let facts = stack(&[(1, "a"), (2, "b"), (3, "c")]);
        let segments = branch_segments(&Host::Native, &read(&fixture), &facts);
        assert_eq!(ids(&segments), vec!["pr:1", "pr:2", "uncommitted", "pr:3"]);
        assert_eq!(paths(&segments[0]), vec!["a.txt"]);
        assert_eq!(paths(&segments[1]), vec!["b.txt"]);
        assert_eq!(paths(&segments[3]), vec!["c.txt"]);
        assert_eq!(segments[1].base_label, "#1");
        assert!(segments[1].current);
        assert!(segments.iter().all(|segment| !segment.needs_restack));

        // Amend the bottom PR without restacking the rest.
        fixture.switch("a");
        std::fs::remove_file(fixture.repo.join("dirty.txt")).unwrap();
        fixture.git(&["commit", "-q", "--amend", "-m", "a, amended"]);
        let segments = branch_segments(&Host::Native, &read(&fixture), &facts);
        let b = segments
            .iter()
            .find(|segment| segment.id == "pr:2")
            .unwrap();
        assert!(b.needs_restack);
        // Until it is restacked, #2 still carries the old #1 commit, as its
        // pull request would show it.
        assert_eq!(paths(b), vec!["a.txt", "b.txt"]);
    }

    fn snapshot_of(fixture: &TestRepo) -> TipSnapshot {
        super::super::snapshot::take(&Host::Native, fixture.repo.to_str().unwrap()).unwrap()
    }

    #[test]
    fn this_thread_shows_what_moved_since_the_start_tips() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.branch("feat");
        fixture.commit("before.txt", "1\n", "before the thread");
        fixture.git(&["branch", "-q", "untouched"]);
        fixture.git(&["branch", "-q", "doomed"]);
        std::fs::write(fixture.repo.join("README.md"), "dirty before\n").unwrap();
        let facts = RepoFacts {
            snapshot: Some(snapshot_of(&fixture)),
            ..RepoFacts::default()
        };

        fixture.commit("during.txt", "2\n", "during the thread");
        fixture.git(&["branch", "-q", "-D", "doomed"]);
        fixture.git(&["branch", "-q", "fresh"]);
        fixture.switch("fresh");
        fixture.commit("fresh.txt", "3\n", "fresh");
        fixture.switch("feat");
        std::fs::write(fixture.repo.join("now.txt"), "n\n").unwrap();

        let segments = thread_segments(&Host::Native, &read(&fixture), &facts).unwrap();
        assert_eq!(
            ids(&segments),
            vec!["moved:feat", "uncommitted", "deleted:doomed", "new:fresh"]
        );
        assert_eq!(paths(&segments[0]), vec!["during.txt"]);
        assert!(!segments[0].rewritten);
        // Made from `feat` during the thread, so it holds both of its commits.
        assert_eq!(paths(&segments[3]), vec!["during.txt", "fresh.txt"]);
        let readme = segments[1]
            .files
            .iter()
            .find(|file| file.path == "README.md")
            .unwrap();
        assert!(readme.before_thread);
        let now = segments[1]
            .files
            .iter()
            .find(|file| file.path == "now.txt")
            .unwrap();
        assert!(!now.before_thread);

        // A rebase onto a moved main rewrites the branch.
        fixture.switch("main");
        fixture.commit("main.txt", "m\n", "main moves");
        fixture.switch("feat");
        std::fs::remove_file(fixture.repo.join("now.txt")).unwrap();
        fixture.git(&["checkout", "-q", "--", "README.md"]);
        fixture.git(&["rebase", "-q", "main"]);
        let segments = thread_segments(&Host::Native, &read(&fixture), &facts).unwrap();
        assert!(segments[0].rewritten);
    }

    #[test]
    fn a_collected_start_tip_degrades_to_a_note() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.branch("feat");
        fixture.commit("gone.txt", "1\n", "soon unreachable");
        let facts = RepoFacts {
            snapshot: Some(snapshot_of(&fixture)),
            ..RepoFacts::default()
        };
        fixture.git(&["reset", "-q", "--hard", "main"]);
        fixture.commit("other.txt", "2\n", "replacement");
        fixture.git(&["reflog", "expire", "--expire=now", "--all"]);
        fixture.git(&["gc", "-q", "--prune=now"]);
        let segments = thread_segments(&Host::Native, &read(&fixture), &facts).unwrap();
        assert_eq!(ids(&segments), vec!["moved:feat"]);
        assert!(segments[0].note.is_some());
        assert!(segments[0].files.is_empty());
    }
}
