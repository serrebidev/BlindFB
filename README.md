# BlindFB

An accessible Facebook and Messenger client for screen reader users, written
in Rust with [wxDragon](https://github.com/trypsynth/wxDragon) (native
wxWidgets controls on Windows, macOS and Linux, so lists, menus and dialogs
are the real platform widgets your screen reader already knows).

BlindFB is a front end for the
[`fb` command-line tool](https://github.com/hieu-lee/facebook-cli)
(hieu-lee/facebook-cli), which drives a dedicated headless Chrome profile
that stays logged in to your Facebook account. BlindFB runs `fb` for
everything and presents its output in a keyboard-first window.

## What you can do

- Browse your **news feed** (with refresh-ahead scrolling), read a post's
  full text and comments, react (like, love, care, haha, wow, sad, angry),
  comment, and open the author's profile or posts.
- Browse **notifications**.
- Browse and search your **friends list**; open any friend's profile, read
  their posts, or jump straight into a Messenger chat with them. (Adding and
  removing friends is not something the `fb` backend can do.)
- Read your own profile posts and **delete** your own posts.
- **Post** to your profile, with an audience (public, friends, only me) and
  optional images.
- **Search** people, pages, groups, posts and events.
- **Messenger**: browse chats (including unread-only), read conversations,
  send text and file attachments, unsend your last message, and unlock
  end-to-end encrypted chats with your Messenger PIN (the PIN is fed to `fb`
  on stdin, never stored).
- **OCR an image** file using the OCR engine built into your operating
  system (via [native-ocr](https://github.com/trypsynth/native-ocr); on
  Linux that is Tesseract from your package manager).
- **Check for updates** from the Help menu; releases are packaged and
  versioned with [shipfitter](https://github.com/trypsynth/shipfitter).

### Not supported

Voice and video **calls** are not supported: the `fb` backend has no call
commands, and Messenger calls use a separate calling stack that no part of
this project drives. Everything else the backend exposes is in the app.

## Requirements

- Google Chrome, logged in to Facebook (the backend imports that session).
- The `fb` binary: install it first —
  Linux/macOS/WSL: `curl -fsSL https://raw.githubusercontent.com/hieu-lee/facebook-cli/main/install.sh | sh`
  Windows (PowerShell): `irm https://raw.githubusercontent.com/hieu-lee/facebook-cli/main/install.ps1 | iex`
  BlindFB looks for `fb` on your `PATH`, in `~/.local/bin`, next to the
  BlindFB executable, or at the path in the `BLINDFB_FB` environment
  variable.
- First run: use **Backend → Log in** and click **Allow** if Chrome asks
  about remote debugging. Encrypted chats additionally need **Chat → Unlock
  encrypted chats** once per daemon lifetime.

## Layout of the window

A Views list (News feed, Notifications, Friends, My posts, Chats), an Items
list for the current view, a Details pane for the selected item, an actions
row, and a message compose row for chats. Menus mirror every action with
keyboard shortcuts: Ctrl+1…5 switch views, F5 refreshes, Ctrl+N writes a
post, Ctrl+F searches.

The lists follow the quiet-list design rule (from
[quiet-list](https://github.com/trypsynth/quiet-list)): rows are plain
native list rows, refreshed in place, so a screen reader is not spammed with
re-announcements while you browse. Helper dialogs (confirm, prompt, about)
come from [wx-utils](https://github.com/trypsynth/wx-utils). The design of
the backend ecosystem follows the Rust conventions of
[rust-shed](https://github.com/facebookexperimental/rust-shed); `fb` itself
is the consumer-facing piece of that world.

## Building

```sh
cargo build --release -p blindfb
cargo xtask package   # writes dist/BlindFB-<version>-<os>-<arch>.zip / .tar.gz
```

Linux build dependencies for wxWidgets are the usual GTK set (see
`.github/workflows/ci.yml`). Releases are built by GitHub Actions on
`v*` tags for Windows, Linux and macOS.

## License

MIT. BlindFB is an unofficial client and is not affiliated with Meta.
