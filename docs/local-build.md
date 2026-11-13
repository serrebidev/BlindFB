# BlindFB local build

How to build, test, package and release BlindFB from this PC.

## You need

- Rust stable through rustup. Cargo lives at
  `C:\Users\admin\.cargo\bin`. If `cargo` is not recognized, prepend that
  folder to PATH in the same shell first.
- MSVC C++ build tools. wxWidgets compiles from source on the first build,
  which takes a while once; after that `target/` caches it.
- Node 22, only for the JavaScript fixture tests in the full test gate.
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
node crates/core/src/post_scripts/questions.test.js
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
A tag push runs `release.yml` on the Forgejo runners (Windows, WSL Ubuntu 22.04,
Mac). It signs with the `MINISIGN_KEY` repository secret, uploads to a draft
release and publishes once every asset is there. The current signing key
(F806D88DE00499EF) is not under `~/.minisign` on this PC: the keys there are
older ones, and assets signed with them fail `cargo xtask verify`. The manual steps
below are for a leg that fails, and need the current key.

1. Bump the workspace version in `Cargo.toml` and write the
   `## vX` section in `CHANGELOG.md`. The section is required; the publish
   step refuses a release without one.
2. `git fetch`, confirm `main` is where you think (other sessions push
   here too), commit, push `main` to the `forgejo` remote, tag `vX`,
   push the tag. Push main and tag as separate commands.
3. Windows assets on this PC: `cargo xtask package` with `MINISIGN_KEY`
   set writes `dist/BlindFB.zip` (the updater's asset) and the per-user
   installer `dist/BlindFB-Setup.exe` (Inno Setup, `packaging/blindfb.iss`),
   each with its `.minisig`. `cargo xtask verify` both and upload both.
4. Linux asset in local WSL (`ssh wsl-build`): build inside an
   `ubuntu:22.04` container (image `blindfb-linux-build:ubuntu-22.04`,
   the apt list from `release.yml` plus build-essential, cmake, git,
   libssl-dev and rustup) with the tag cloned to `/src` and the
   `blindfb-cargo-registry`, `blindfb-cargo-git` and
   `blindfb-linux-target` volumes reused, run `cargo xtask package`
   unsigned, copy `dist/BlindFB.tar.gz` back here and sign it here with
   `cargo xtask sign`. Ubuntu 22.04 keeps glibc old enough for Debian 13
   and Ubuntu 24.04. Never build natively in WSL: its glibc is too new.
5. Write `dist/SHA256SUMS` over every asset, two spaces between hash
   and file name.
6. Create the Forgejo release on the tag with the CHANGELOG section as
   notes, and upload each asset plus its `.minisig` through the release
   attachments API. The multipart field name must be `attachment`. The
   API token stays on the VPS at `/root/forgejo-muse-token.txt`; run the
   upload from there so the token never leaves it.
7. macOS: dispatch `macos-release.yml` on `main` with
   `release_tag=vX`. The Forgejo Mac runner builds, signs and attaches
   `BlindFB-arm64.dmg` and its `.minisig`; it does not touch
   `SHA256SUMS`, so write that file after the dmg lands.

Do not put `[skip ci]` in the commit the tag points at: Forgejo then skips
`release.yml` and nothing is built. Create a manual release as a
prerelease until every asset is attached (`/releases/latest` skips
prereleases, so installed copies are not offered a half-built release),
then clear the prerelease flag.

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
