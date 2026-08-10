# Pull-request review

Priority: P1

## What it should do

Open a pull request, inspect changed files and conversation, ask Codex for a review, and publish or save review comments. Track whether the PR changed while it is open.

## How

Add a provider-neutral review service with a GitHub adapter first. Fetch metadata, commits, changed files, inline diff hunks, and comments through native commands or an approved connector. Reuse `DiffBlock` for rendering, but add stable file/line anchors and a review state model.

## What it should look like

Use a three-pane review view: PR summary and files on the left, diff in the center, and review/comments on the right. Add `Start review`, `Reply`, `Resolve`, and `Submit review` actions. Show a stale-data banner when the remote PR changes.
