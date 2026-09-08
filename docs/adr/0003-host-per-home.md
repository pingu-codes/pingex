---
status: accepted
---

# Every Home has a Host: native or a WSL distribution

Pingex was a macOS app whose harnesses, repositories and config directories
all lived on the machine it ran on. On Windows the CLIs and the repositories
usually live inside WSL, while the app itself is a native window, and a user
may keep one Codex home in a distribution next to a native Claude. We chose to
make the location a property of each Home, called its Host (`native` or
`wsl(<distro>)`, `src-tauri/src/util/host.rs`), and to give the Host the three
things that differ: how a process is spawned (`wsl.exe -d <distro> --cd <cwd>
--exec /usr/bin/env K=V <program> ...`), how a *host path* maps to a *local
path* the app can open (`/home/u/x` to `\\wsl.localhost\<distro>\home\u\x`,
`/mnt/c/..` to `C:\..`), and how a binary is located (a login shell inside
the distribution, then an interactive bash, then the usual install
directories, remembered per distribution). Everything else asks the Host:
the drivers, the git runner, the worktree and hub code, file search, the
indexer, attachments, handoff commands and deep links. The database of a WSL
home stays on the Windows side (`<data_dir>/pingex/hosts/wsl/<distro>/...`),
because SQLite over the 9P share locks badly. A WSL home's key is
`wsl:<distro>:<path>`, so the same folder on two hosts is two homes; native
keys are unchanged. The cost is a `host` parameter threaded through every
place that touched the filesystem or spawned a process, and a per-distribution
process reaper, since killing `wsl.exe` does not reliably kill the Linux
child. The gain is that native and WSL harnesses run in one window at once,
with no harness or frontend code checking where anything lives.

The vocabulary is in `CONTEXT.md`; the mechanics are in
`features/13-harnesses.md`, section "Hosts".

## Considered options

- **A global "run everything in WSL" toggle.** One setting, no home keys to
  change. Rejected because the user wants a native Claude next to a WSL Codex,
  and two Codex homes on different hosts, at the same time.
- **Run Pingex itself inside WSL under WSLg.** No path translation at all, the
  Linux build just works. Rejected because the user wants a native Windows
  window, and WSLg windows lack the OS integration (file dialogs, the
  clipboard, deep links) the app relies on.
- **Detect the host from the binary.** Probe `wsl.exe -l` when `codex` is not
  found natively. Rejected because it cannot express two homes on different
  hosts, and a probe that picks a distribution silently is a surprise.
- **Keep the database inside the WSL home over the share.** Simplest layout.
  Rejected after checking how turso opens the file: byte-range locks over 9P
  are unreliable and every page read crosses the VM boundary.
