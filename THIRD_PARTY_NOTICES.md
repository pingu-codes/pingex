# Third-party dependency notices

Pingex itself is MIT-licensed. Its Rust and npm dependencies are separately licensed and must retain the notices required by their respective licenses.

Before distributing a Pingex artifact, generate and review an inventory from a clean checkout:

```sh
cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1 --locked > /tmp/pingex-cargo-metadata.json
bun pm ls --all > /tmp/pingex-npm-modules.txt
```

For each package in those inventories, collect the license expression and notice text from its package metadata, then include the resulting consolidated notice file beside the distributed artifact. Review packages without a declared license manually; do not ship until their terms are understood.

This source-first release does not distribute binaries, so it has no generated binary notice bundle yet.
