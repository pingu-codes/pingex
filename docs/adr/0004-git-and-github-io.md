---
status: accepted
---

# Git and GitHub I/O: read by SHA, one fixed environment, gh as the only credential

The review desk reads other people's pull requests, stacks and review state
while agent threads run git in the same repositories. Before it, every PR
diff came from a working tree or a branch name, git ran with whatever locale
and config the user's shell had, and each GitHub read was its own `gh`
call. We decided four things.

1. **PR content is read by SHA from the object database.** A PR's commits are
   fetched into hidden refs, `refs/pingex/pr/<n>/head`, in one `git fetch`
   per repository, from the remote that points at the PR's base repository
   (GitHub publishes `refs/pull/<n>/head` there for fork PRs too). Every
   diff, file and "changes since" read after that names commits by SHA
   (`src-tauri/src/git/objects.rs`). Nothing is checked out and no branch is
   created. A SHA never changes what it points at, so a result can be cached
   for as long as memory allows, and a force-push shows up as a new SHA
   rather than a silently different answer. Patches, blobs and inbox
   commit counts are cached for 30 minutes (`util/ttl_cache.rs`); every
   backend cache expires, even one whose answer cannot change. The Seen head is pinned at
   `refs/pingex/pr/<n>/seen/<sha>`, so its commits survive the force-push
   and `git gc`, and "changes since" can still compare against it. That
   comparison is a plain diff when the Seen head is an ancestor of the new
   head, and `git range-diff` when it is not. Ref writes are
   `update-ref --stdin` transactions under the common-dir lock.

2. **Every git call runs with one fixed environment** (`GIT_ENV` and
   `GIT_CONFIG` in `src-tauri/src/git/run.rs`):
   - `LC_ALL=C`, so the stderr classifiers match on any locale. The
     developer's own git is German-capable, and the suite passes under
     `LC_ALL=de_DE.UTF-8`.
   - `GIT_OPTIONAL_LOCKS=0`, so a background `status` never takes
     `index.lock` from an agent.
   - `GIT_TERMINAL_PROMPT=0`, `GIT_PAGER=cat`, `GIT_ADVICE=0`.
   - `-c core.quotepath=off -c color.ui=never`.

   On top of that, `-z` goes on every listing that carries paths,
   `--no-ext-diff --no-textconv` on every diff, and exit codes are used
   where git offers one (`merge-base --is-ancestor`,
   `rev-parse --verify --quiet`). Big blobs and patches are read through a
   byte cap that kills the child (`run_git_capped`).

3. **GitHub goes through `gh`, and reads are GraphQL pages Pingex walks
   itself** (`gh_graphql` in `src-tauri/src/review/github/run.rs`). gh stays the
   only store of tokens: a WSL Home uses the distribution's own gh and login,
   and Pingex never stores a credential.
   - A repository can run as a signed-in account other than gh's active one
     (`src-tauri/src/review/github/account.rs`). Pingex asks `gh auth token` for
     that account and passes the token to `gh` in `GH_TOKEN`, for one command.
     It holds the token in memory for that long, keeps it off command lines
     (`Host::set_secret_env`) and never writes it anywhere. The alternative,
     `gh auth switch`, would change the account `gh` uses in the user's
     terminal, and two repositories syncing at once would race on it.
   - The inbox is one query per repository (`src-tauri/src/review/github/inbox.rs`),
     25 PRs a page and at most 8 pages, each page with its own timeout. We
     page ourselves rather than use `gh --paginate` so a huge repository
     cannot turn one sync into an unbounded read.
   - Page size is measured: on cli/cli, 50 PRs took about 7s against
     GitHub's ~10s limit, and asking for `mergeStateStatus` pushed it into a
     502. That field is left out, and "behind the base" is counted locally
     from the fetched refs.
   - The "since you last looked" feed is derived by comparing each sync with
     the snapshot stored from the previous one. That costs no extra requests
     and needs no notification scope.
   - Reviews are always posted with `commit_id`, the head the reviewer read.

4. **Stacks are GitHub's native stacks** (`GET repos/{o}/{r}/stacks`,
   `src-tauri/src/review/github/stacks.rs`), not inferred from base branches.
   - The listing runs bottom first. A merged PR stays in it, and the PR
     above that one is retargeted onto the stack's base; this was observed
     on a real repository.
   - A repository that answers 404 has no stacks, which is cached for an
     hour.
   - Native stacks only cover stacks made with GitHub's tooling, and that
     was accepted. Inferring from base chains would also catch hand-made
     chains, but it guesses, and it cannot tell a stack from two unrelated
     PRs that happen to share a branch.

## Writes (next slice)

Every history-changing operation follows the same transaction, under the
common-dir lock: snapshot → act → verify → restore.

1. **Snapshot.** Record the SHA of every ref the operation can move.
2. **Act.** Run the operation:
   - A restack is `git rebase --update-refs` in the stack's worktree.
   - Pushing a stack is `git push --atomic` with
     `--force-with-lease=<ref>:<snapshotted sha>` and `--force-if-includes`.
     It goes to the PR's head remote, never a hard-coded `origin`. Pushing
     to someone else's branch asks every time.
   - A Review checkout is `git worktree add --detach` followed by
     `worktree lock`. Promoting it to a worktree is a `git switch` in place.
3. **Verify** that the refs moved where they were supposed to.
4. **Restore on failure or abort.** Put every ref back with one
   `update-ref --stdin` transaction.

Several of these flags need a minimum git version, and WSL distributions
often ship an older git, so the git version is probed once per Host and
operations it lacks are refused with a message:
- `diff --merge-base` needs git 2.30.
- `--path-format` needs 2.31.
- `rebase --update-refs` needs 2.38.

## Considered options

- **A native HTTP client with the token from `gh auth token`.** This would
  have given ETags, rate-limit headers and connection reuse. It was rejected
  because Pingex would then hold a token for as long as it runs, and a WSL Home
  would need its own credential path.
- **libgit2 or gix in place of the git CLI.** Rejected because agents and
  users run the CLI on the same repositories. Matching its config, hooks,
  credential helpers and on-disk formats is the CLI's job.
