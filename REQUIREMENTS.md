# Pingex requirements

`[x]` implemented · `[ ]` not implemented

## Runtime and projects

- [x] Run as a Tauri desktop app with a Svelte frontend and native draggable titlebar.
- [x] Resolve `CODEX_HOME` from `--codex-home`, the environment, or `~/.codex`.
- [x] Resolve the Codex CLI from `PINGEX_CODEX_CLI_PATH`, legacy `PINGU_CODEX_CLI_PATH`, `CODEX_CLI_PATH`, or `codex`.
- [x] Connect to `codex app-server` over stdio and show connection/request errors.
- [x] Add an existing project with a native folder picker, validate it, and persist it in the Rust-owned Turso database at `CODEX_HOME/pingex.db`.
- [x] Copy legacy Pingu Codex settings and local state once without deleting the legacy files.
- [x] Cache thread details in Turso and reuse them while the matching Codex thread update timestamp is unchanged.
- [x] Serve project/thread pin and rename mutations from the cached Turso projection without another Codex refresh.
- [x] Group up to 200 non-archived Codex threads under projects by working directory.
- [x] Collapse projects, show thread counts/titles/relative update times, select a thread, and refresh manually.
- [x] Remove or reorder saved projects.
- [x] Show archived threads or paginate beyond 200 threads.
- [x] Custom context menu on right-click for projects and threads, plus a `...` button shown on hover that opens the same menu.
- [x] Context menu: open project/thread in Finder.
- [x] Context menu: rename project/thread.
- [x] Context menu: pin project.
- [x] Context menu: pin thread to top of its project, shown with a small star.
- [x] Discover Codex's permanent worktrees and display them as projects with an icon indicating they are worktrees.

## Thread viewer

- [x] Load a selected thread with all available turns and show loading, empty, failed-turn, and request-error states.
- [x] Render user text and named input references.
- [x] Render agent messages, plans, and reasoning summaries as sanitized GitHub-flavored Markdown.
- [x] Syntax-highlight fenced code blocks.
- [x] Show command status, command text, duration, exit code state, and collapsible output.
- [x] Show file-change kind and collapsible highlighted diffs.
- [x] Make sure file change diffs are truncated before expansion / collapse to prevent rendering massive diffs.
- [x] Show MCP, dynamic-tool, and web-search activity summaries.
- [x] Render image, URL, and local-file input attachments.
- [x] Stream live turn/item updates without a manual refresh.
- [x] Create a thread, compose/send messages, interrupt work, or answer approvals.
- [x] Rename, archive, delete, or resume threads.
- [x] Show reasoning / working while a response is being generated with a cut-off unless user expands
- [x] Show a loading indicator while a response is being generated.
- [x] Allow user to expand reasoning to see the full reasoning summary even once response is finished and nested under "Worked for 11ms...".
- [x] Be able to select model and effort levels below chat input (expand popover)
- [x] Control included subagent models and effort levels from a dedicated popover separate from the parent model selector.
- [x] Show all descendant subagent threads, resolved model/effort, and active/finished state in the floating menu.
- [x] Show descendant subagent counts on root sidebar threads, hide child rows, and navigate children with a parent back button.
- [x] Be able to set permissions level below chat input
- [x] Show live context usage as a ring below the chat input that fills as the window is consumed, with exact token stats in a popover on hover.
- [x] Compact the thread with `/compact` (or by clicking the context ring) and show a marker in the transcript where the context was compacted.
- [x] Be able to attach files with `@` syntax and fuzzy search from the project (default to using .gitignore)
    - [x] This should use rust for iterating / searching the project and only pass relevant file paths to the frontend
    - [x] They should be rendered with icons for common file types and languages
- [x] Be able to remove projects from the list

### Thread Forking / Editing

- [x] Be able to edit a users message and resend it
- [x] Be able to fork a thread to ask separate questions
- [x] Be able to ask side questions on a thread, these should be rendered on a right hand panel that can be closed xd out
- [x] Store side questions and their responses with a way for people to easily view them for threads (and see how many side questions a thread has without loading it)
- [x] Be able to fork a thread without sending a new message

### Thread Floating Menu

- [x] Have a floating menu like `./floating menu.png` on each thread that displays useful info and the ability to open stuff like plans, changes etc on the right hand panel
- [x] Allow user to open project in Finder or IDE (just zed ide for now)

## Account and shell

- [x] Show the current account label, kind/plan, resolved `CODEX_HOME`, and Codex binary.
- [x] Provide browser-only preview data for frontend development.
- [x] Enforce an 820×560 minimum window and support drag/double-click maximize behavior.
- [x] Edit runtime/account settings from the settings dialog.
- [x] Resolve the Codex CLI even when launched from Finder (bare PATH), validate it before a home is
      opened or created, and offer a binary-path input in the launch picker when it cannot be found.
- [x] Make the visible **New thread** button functional.

## Remote Sync

- [x] Be able to connect to this from my phone's chatgpt app with a QR code to scan and connect
