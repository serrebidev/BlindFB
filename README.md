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

- Browse your **news feed** (with refresh-ahead scrolling), or **Feeds
  (most recent)** with Ctrl+7, with each post's full text in its row. React
  (like, love, care, haha, wow, sad, angry), comment, and open the author's
  profile or posts.
- **Post reader**: press Enter on a post to open it in a native dialog with
  the full post, its comments and replies, and a comment box ("Write a
  comment", Alt+W; Ctrl+Enter sends). Applications or Shift+F10 on a comment
  replies to that exact comment, likes it, copies it or opens it on the web;
  on the post it comments, reacts, shares, copies the text or link, or opens
  it on the web. **Share** (Alt+H) uses Facebook's own share dialog, tells
  you the audience first, and takes optional text.
- **Videos and reels** play in the reader, with Play/Pause, Stop, back/forward
  10 seconds and a volume slider. Tab reaches every control; comments,
  reactions and sharing remain available. **File → Open Facebook link**
  (Ctrl+O) opens a post, video or reel URL. Post links have their own list;
  Enter or **Open link** opens the selected page in your browser. F1 lists
  keyboard commands. The video helper installs itself when first needed.
- **Reels** (Ctrl+8) has its own section, with Load more and the same reader,
  playback and post actions. Its list stays cached until you refresh it;
  arrow keys change the selected reel, and Enter opens the player and starts
  playback automatically. The selected and next reel prepare in the background.
  Notifications about several new reels list the
  exact reels when Facebook supplies their IDs; otherwise they show the
  account's reels and explain that fallback.
- Every item in the Items list has a context menu (Applications or
  Shift+F10) with the actions that fit it. **Share** is the second item
  for posts, including the news feed, Feeds, profile and group posts,
  search results and notifications. It opens the reader and share prompt.
- **Load more** (Ctrl+L, the button, or View menu) appends more items
  without moving your selection or repeating existing rows. It works in
  feeds, post lists, search results, notifications, friends and chat lists.
  Other sections do not support loading more. Facebook may return fewer
  items or none when it has no further results available.
- Browse **notifications** and open the posts they point to.
- Browse and search your **friends list**; open any friend's profile, read
  their posts, or jump straight into a Messenger chat with them. (Adding and
  removing friends is not something the `fb` backend can do — the Post menu
  links out to the person's real profile on the web, where that lives.)
- **Groups**: the Groups view lists the groups you belong to (read out of
  facebook.com/groups through the backend's named-page browser support).
  Open a group to read its posts and take part with the usual view, comment
  and react actions; group posts also appear in your news feed as usual.
  Posting *into* a group and group settings live on the group's web page,
  one menu item away.
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
- **Check for updates** from the Help menu: BlindFB downloads the new
  release, verifies its minisign signature, installs it and restarts.
  Releases are packaged with [shipfitter](https://github.com/trypsynth/shipfitter)
  and installed by [ship-shape](https://github.com/trypsynth/ship-shape).

### Not supported in-app (linked out instead)

Voice and video **calls**: the `fb` backend has no call commands, and
Messenger calls use a separate calling stack. The Chat menu's "Open chat in
Messenger (web)" takes you to that very conversation in your browser, where
the call buttons are. The same pattern covers adding/removing friends
(profile on the web) and posting into a group (the group's web page).
Everything else the backend exposes is in the app.

Facebook hides the link of many feed posts until they get focus. BlindFB's
backend build reveals most of them, including group posts, but a few rows
(often ads and suggestions) still have none; BlindFB cannot open, react to
or comment on those and marks them “(no link)” in the list. Photo albums open
in your browser instead of the reader.

## Requirements

On Ubuntu 24.04 or compatible Debian systems (glibc 2.39 or newer), extract
the Linux download and start `blindfb`. On
first launch it installs missing GTK and GStreamer media libraries through
your package manager, using the standard system password prompt. Keep
`blindfb` and `blindfb-bin` together. macOS uses its built-in media frameworks.

- A browser you are signed in to Facebook in: Google Chrome, Chromium,
  Brave, Microsoft Edge or Firefox. Chrome, Chromium, Brave and Edge can
  run the backend. When stable Chrome/Chromium is unavailable, BlindFB
  starts another installed Chromium browser in a separate profile; your
  everyday browser stays separate. Firefox can supply a session but needs
  one of those Chromium browsers as the engine. **Backend → Install a
  browser** installs a browser from its vendor.
- Nothing else to install: **BlindFB installs the `fb` backend itself** on
  first run — it downloads the matching facebook-cli release from GitHub,
  verifies its SHA-256 checksum, and keeps it in BlindFB's own app-data
  folder, replacing an older copy at startup. Until two fixes are merged
  upstream (hidden feed post links, hieu-lee/facebook-cli#1; the Messenger
  restore prompt, #2), that release is
  [serrebidev/facebook-cli `v0.1.5-blindfb.4`](https://github.com/serrebidev/facebook-cli/releases/tag/v0.1.5-blindfb.4):
  upstream 0.1.5 plus those fixes, faster feed loading, complete shared-post
  text and the empty comment/share editor fix ([PR #3](https://github.com/hieu-lee/facebook-cli/pull/3),
  [PR #4](https://github.com/hieu-lee/facebook-cli/pull/4)). Full post text is
  expanded before reading, including list rows ([PR #5](https://github.com/hieu-lee/facebook-cli/pull/5)).
  Expansion controls are excluded from post text ([PR #6](https://github.com/hieu-lee/facebook-cli/pull/6)).
  **Backend → Install or update backend**
  reinstalls it any time.
  (BlindFB also finds an `fb` you installed yourself: the managed copy
  first, then next to the BlindFB executable, then your `PATH`, then
  `~/.local/bin`; the `BLINDFB_FB` environment variable overrides all.)
- First run, sign-in happens **in your browser**: BlindFB asks which
  browser to copy your Facebook session from (your default browser is
  marked). On Windows, BlindFB uses [rookie-cookies](https://github.com/teng-lin/rookie-cookies)
  to read Facebook cookies from the selected Chromium profile. Close that
  browser first if its cookie database is locked. Encrypted-cookie import
  depends on the installed browser's Windows encryption service; Stable
  Brave is tested, while Beta/Nightly may require direct sign-in instead.
  On macOS/Linux, tick **Allow remote debugging** if the browser asks.
  Firefox sessions are read from its cookie database. You can also choose
  **Sign in in a new browser window** and sign in directly in BlindFB's
  separate browser profile. A completed import is checked against Facebook.
  Encrypted chats additionally need **Chat → Unlock encrypted chats**
  once per daemon lifetime.

  On Windows, rookie-cookies may use an unprivileged browser-process
  injection method for app-bound encrypted cookies. BlindFB does not request
  its elevated SYSTEM fallback. Direct sign-in avoids cookie extraction.
  Cookie values are never passed on the command line or written to logs.

## Layout of the window

A Views list (News feed, Notifications, Friends, My posts, Chats, Groups,
Feeds), an Items list for the current view, a Details pane for the selected
item, an actions row, and a message compose row for chats. Menus mirror every
action with keyboard shortcuts: Ctrl+1…7 switch views, Enter opens the
selected item, F5 refreshes, Ctrl+L loads more, Ctrl+N writes a post, Ctrl+F searches.

Facebook is read live, so a section's first load takes several seconds.
Once the News feed is up, BlindFB loads the other main sections in the
background, so switching sections is instant. A section you return to is
shown at once and quietly refreshed if it is over a minute old; the News
feed keeps your place until you press F5; Ctrl+L appends its next posts.

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
cargo xtask package   # writes the updater's asset: dist/BlindFB.zip (Windows),
                      # dist/BlindFB.tar.gz (Linux), dist/BlindFB-<arch>.dmg (macOS)
                      # and its .minisig when MINISIGN_KEY is set
```

Linux build dependencies for wxWidgets are the usual GTK set (see
`.github/workflows/ci.yml`). Windows releases are packaged locally, Linux on
the release build host, and macOS by GitHub Actions on `v*` tags.

## License

MIT. BlindFB is an unofficial client and is not affiliated with Meta.
