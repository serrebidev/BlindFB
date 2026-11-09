# BlindFB local build

How to build, test, package and release BlindFB from this PC.

## You need

- Rust stable through rustup. Cargo lives at
  `C:\Users\admin\.cargo\bin`. If `cargo` is not recognized, prepend that
  folder to PATH in the same shell first.
- MSVC C++ build tools. wxWidgets compiles from source on the first build,
  which takes a while once; after that `target/` caches it.
- Node 22, only for the full test gate (two JavaScript fixture tests).
- GNU gettext (`xgettext`, `msgmerge`), only for `cargo xtask i18n`.

## Everyday loop

```powershell
$env:PATH = "C:\Users\admin\.cargo\bin;" + $env:PATH
cargo build -p blindfb            # debug binary
cargo run -p blindfb              # run it
cargo build --release -p blindfb  # target/release/blindfb.exe
```

Tests and lint (the same gate the release workflow runs):

```powershell
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
node crates/core/src/post_scripts/reader.test.js
node crates/core/src/reels.test.js
```

Never run `cargo fmt --all`: the repo is hand-formatted and that rewrites
untouched files. Format only files you changed:

```powershell
rustfmt --edition 2021 --style-edition 2024 --config use_small_heuristics=Max,max_width=100 <files>
```

Do not rustfmt `fb.rs` or `main.rs` at all; format those by hand.

## Package and sign (Windows)

The signing key is
`C:\Users\admin\cloud\Seafile\Docs\SigningThings\blindfb\minisign.key`
(see its README.txt). The password sits beside it in `password.txt`; both
are required.

```powershell
$env:MINISIGN_KEY = Get-Content 'C:\Users\admin\cloud\Seafile\Docs\SigningThings\blindfb\minisign.key' -Raw
$env:MINISIGN_PASSWORD = Get-Content 'C:\Users\admin\cloud\Seafile\Docs\SigningThings\blindfb\password.txt' -Raw
cargo xtask package   # builds release, writes dist/BlindFB.zip + .minisig
cargo xtask verify dist/BlindFB.zip
```

`dist/` is per-machine build output and is gitignored. Other helpers:
`cargo xtask sign <file>`, `cargo xtask verify <file>...`,
`cargo xtask update-check [version]`.

## Backend

On first run BlindFB downloads the managed `fb` backend release, verifies
its SHA-256 checksum and keeps it in its own app-data folder. The
`BLINDFB_FB` environment variable overrides the backend location. Before
publishing an app release, check the pinned backend release exists;
v0.7.18 shipped once with a missing backend and every install failed to
fetch it.

## Release (Forgejo, while GitHub is suspended)

Releases live at `https://git.serrebiradio.com/serrebi/BlindFB/releases`.
There is no CI runner: every step below is manual.

1. Bump the workspace version in `Cargo.toml` and write the
   `## vX` section in `CHANGELOG.md`. The section is required; the publish
   step refuses a release without one.
2. `git fetch`, confirm `main` is where you think (other sessions push
   here too), commit, push `main` to the `forgejo` remote, tag `vX`,
   push the tag. Push main and tag as separate commands.
3. Windows asset on this PC: `cargo xtask package` with `MINISIGN_KEY`
   set, then `cargo xtask verify` it.
4. Linux asset on the VPS (`serrebiradio-root` over Tailscale): build
   inside the `blindfb-media-build` Docker image with the source at
   `/src` and the `blindfb-cargo-registry`, `blindfb-cargo-git` and
   `blindfb-linux-target` volumes reused, run `cargo xtask package`
   unsigned, copy `dist/BlindFB.tar.gz` back here and sign it here with
   `cargo xtask sign`. Ubuntu 22.04 keeps glibc old enough for Debian 13
   and Ubuntu 24.04.
5. Write `dist/SHA256SUMS` over every asset, two spaces between hash
   and file name.
6. Create the Forgejo release on the tag with the CHANGELOG section as
   notes, and upload each asset plus its `.minisig` through the release
   attachments API. The multipart field name must be `attachment`. The
   API token stays on the VPS at `/root/forgejo-muse-token.txt`; run the
   upload from there so the token never leaves it.
7. There is no macOS runner while GitHub is suspended, so releases ship
   Windows and Linux only. Say so in the notes.

Every installed copy older than the rekey (v0.7.34 and below) must install
a Forgejo release by hand: the in-app updater still reads GitHub releases,
so no older build offers a Forgejo-only release itself.

## Troubleshooting

- `cargo` not recognized: prepend `C:\Users\admin\.cargo\bin` to PATH.
- First build very slow: that is wxWidgets compiling; later builds reuse it.
- Disk filling up: `target/` grows into tens of gigabytes; `cargo clean`
  when C: runs low.
- `cargo xtask verify` fails on a fresh signature: the matching public key
  is `app/assets/minisign.pub`. A signature made for another key reports
  "created with a different key".
