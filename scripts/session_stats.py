#!/usr/bin/env python3
"""Where do agent sessions spend tokens and time?

Reads the Claude Code transcripts for this project and prints:
  - token totals and the biggest sessions (context size x turns is the cost)
  - tool-call counts, result sizes and wait time
  - Bash commands grouped by shape (test / check / lint / read ...) with timings
  - the files agents Read most often

Usage: python3 scripts/session_stats.py [transcripts-dir]
Default dir: ~/.claude-personal/projects/<this-project-slug> (falls back to ~/.claude).
"""

import collections
import glob
import json
import os
import re
import statistics
import sys
from datetime import datetime


def transcripts_dir() -> str:
    if len(sys.argv) > 1:
        return sys.argv[1]
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    slug = repo.replace("/", "-")
    for base in ("~/.claude-personal", "~/.claude"):
        d = os.path.expanduser(f"{base}/projects/{slug}")
        if os.path.isdir(d):
            return d
    sys.exit("no transcript directory found; pass it as the first argument")


def strip_paths(cmd: str) -> str:
    return re.sub(r"/[\w./-]+", "", cmd)


def shape(cmd: str) -> str:
    s = strip_paths(cmd.strip())
    if "cargo test" in s:
        if "live_" in s:
            return "cargo test live_*"
        if "--lib" in s:
            return "cargo test --lib"
        if re.search(r"cargo test \S", s):
            return "cargo test <filter>"
        return "cargo test (all)"
    for word in ("check", "clippy", "build", "fmt"):
        if f"cargo {word}" in s:
            return f"cargo {word}"
    if "test:e2e" in s or "playwright" in s:
        return "playwright / live e2e"
    if re.search(r"deno task test\b", s):
        return "deno task test <filter>" if re.search(r"deno task test(?: --)? \S*\.test", cmd) else "deno task test (all)"
    if "deno task check" in s or "svelte-check" in s:
        return "deno task check"
    if "deno task lint" in s or "biome" in s:
        return "deno task lint"
    if "deno task typegen" in s:
        return "deno task typegen"
    if re.match(r"(cd \S+ *(&&|;) *)?git\b", s):
        return "git"
    if re.match(r"(cd \S+ *(&&|;) *)?(cat|head|tail|sed -n|grep|rg|find|ls|wc|awk|tree|diff)\b", s):
        return "shell read (cat/grep/sed)"
    if re.match(r"(cd \S+ *(&&|;) *)?python", s):
        return "python"
    return "other"


def ts(s: str) -> datetime:
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def main() -> None:
    root = transcripts_dir()
    files = sorted(glob.glob(os.path.join(root, "**", "*.jsonl"), recursive=True))
    totals = collections.Counter()
    sessions = []
    tool_calls = collections.Counter()
    tool_chars = collections.Counter()
    tool_wait = collections.defaultdict(float)
    bash = collections.defaultdict(list)
    bash_chars = collections.Counter()
    reads = collections.Counter()
    read_chars = collections.Counter()
    turns_per_prompt = []

    for f in files:
        pending = {}
        usage = collections.Counter()
        ctx_sizes = []
        turns = 0
        since_prompt = 0
        for line in open(f):
            try:
                d = json.loads(line)
            except json.JSONDecodeError:
                continue
            kind = d.get("type")
            if kind == "assistant":
                msg = d["message"]
                u = msg.get("usage") or {}
                for k in ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens"):
                    usage[k] += u.get(k) or 0
                ctx = (u.get("cache_read_input_tokens") or 0) + (u.get("cache_creation_input_tokens") or 0)
                if ctx:
                    ctx_sizes.append(ctx)
                turns += 1
                since_prompt += 1
                for b in msg.get("content", []):
                    if isinstance(b, dict) and b.get("type") == "tool_use":
                        tool_calls[b["name"]] += 1
                        pending[b["id"]] = (b["name"], b.get("input", {}), d.get("timestamp"))
                        if b["name"] == "Read":
                            reads[os.path.relpath(b["input"].get("file_path", ""), os.getcwd())] += 1
            elif kind == "user":
                content = d["message"].get("content")
                if isinstance(content, str) or (isinstance(content, list) and content and content[0].get("type") == "text"):
                    if since_prompt:
                        turns_per_prompt.append(since_prompt)
                        since_prompt = 0
                if not isinstance(content, list):
                    continue
                for b in content:
                    if not (isinstance(b, dict) and b.get("type") == "tool_result"):
                        continue
                    cc = b.get("content")
                    text = "".join(x.get("text", "") for x in cc if isinstance(x, dict)) if isinstance(cc, list) else str(cc or "")
                    p = pending.pop(b.get("tool_use_id"), None)
                    if not p:
                        continue
                    name, inp, t0 = p
                    try:
                        wait = (ts(d["timestamp"]) - ts(t0)).total_seconds()
                    except (KeyError, ValueError, TypeError):
                        wait = 0
                    tool_chars[name] += len(text)
                    tool_wait[name] += wait
                    if name == "Read":
                        read_chars[os.path.relpath(inp.get("file_path", ""), os.getcwd())] += len(text)
                    if name == "Bash":
                        k = shape(inp.get("command", ""))
                        bash[k].append(wait)
                        bash_chars[k] += len(text)
        if turns:
            sessions.append((sum(usage.values()), f, turns, ctx_sizes))
            totals.update(usage)

    total = sum(totals.values())
    print(f"sessions: {len(sessions)}   total tokens: {total / 1e6:.0f}M")
    for k, v in totals.items():
        print(f"  {k:30s} {v / 1e6:8.1f}M  ({v / total:.0%})")
    side = sum(t for t, f, *_ in sessions if os.sep in os.path.relpath(f, root))
    print(f"  subagent sidechains            {side / 1e6:8.1f}M  ({side / total:.0%})")

    print("\nTop sessions (tokens = avg context x turns):")
    for tot, f, turns, ctx in sorted(sessions, reverse=True)[:10]:
        avg = statistics.mean(ctx) if ctx else 0
        print(f"  {tot / 1e6:6.0f}M  turns={turns:4d}  avg_ctx={avg / 1e3:4.0f}k  max_ctx={max(ctx, default=0) / 1e3:4.0f}k  {os.path.basename(f)[:8]}")

    if turns_per_prompt:
        tp = sorted(turns_per_prompt)
        print(f"\nassistant turns per user prompt: median={statistics.median(tp):.0f} mean={statistics.mean(tp):.1f} p90={tp[int(len(tp) * .9)]} max={tp[-1]}")

    print("\nTools: calls / result kchars / wait seconds")
    for k, v in tool_calls.most_common(12):
        print(f"  {k:28s} {v:5d} {tool_chars[k] / 1e3:8.0f}k {tool_wait[k]:8.0f}s")

    print("\nBash by shape: runs / median / p90 / total wait / result kchars")
    for k, v in sorted(bash.items(), key=lambda x: -sum(x[1])):
        v = sorted(v)
        print(f"  {k:28s} {len(v):5d} {statistics.median(v):6.1f}s {v[int(len(v) * .9)]:6.1f}s {sum(v) / 60:6.0f}min {bash_chars[k] / 1e3:7.0f}k")

    print("\nMost Read files: count / kchars")
    for k, v in reads.most_common(12):
        print(f"  {v:3d} {read_chars[k] / 1e3:6.0f}k {k}")


if __name__ == "__main__":
    main()
