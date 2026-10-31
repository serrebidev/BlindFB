# BlindFB

A vibe-coded, screen-reader-friendly Facebook and Messenger client for Windows, macOS, and Linux, built for reading, posting and chatting without the web site.

[![Join SerrebiProjects on Telegram](https://img.shields.io/badge/Telegram-SerrebiProjects-2CA5E0?style=for-the-badge&logo=telegram&logoColor=white)](https://t.me/SerrebiProjects)
[![License: MIT](https://img.shields.io/badge/License-MIT-green?style=for-the-badge)](LICENSE)

**Questions, bugs, or release news?** Join the [SerrebiProjects Telegram group](https://t.me/SerrebiProjects), the fastest place to get help.

BlindFB is a front end for the [`fb` command-line tool](https://github.com/hieu-lee/facebook-cli), which drives a dedicated browser profile that stays logged in to your Facebook account. BlindFB runs `fb` for everything and presents its output in a keyboard-first window built on [wxDragon](https://github.com/trypsynth/wxDragon): native lists, menus and dialogs, the real platform widgets your screen reader already knows.

## Features

- News feed with refresh-ahead scrolling, and Feeds (most recent) on Ctrl+7, with each post's full text expanded in its row.
- A post reader on Enter: the full post, its comments and replies, and a comment box; the context menu replies to, likes and copies the exact comment.
- React with like, love, care, haha, wow, sad and angry, comment, and share through Facebook's own share dialog, which tells you the audience first and takes optional text.
- Reels on Ctrl+8, in a cached list you can browse by caption; Enter plays, Escape returns to the same reel.
- Videos and reels play in the reader, with Play/Pause, Stop, seek and a volume slider, and every control reachable with Tab.
  Windows uses automatically installed mpv and its bundled codecs. Native NVDA controls keep the video surface out of Tab order.
- File, Open Facebook link (Ctrl+O) opens a post, video or reel URL directly.
- The final Load more list item (Enter, or Ctrl+L) appends the next posts, reels, results or chats and selects the first new item. Batches request 100 items, or 20 reels; Facebook may return fewer.
- External links come first, shared source links follow, and the current Facebook post stays last. Links have a context submenu and receive focus first in the reader.
- Notifications open their posts in the reader, including group post/comment links. Other destinations open on Facebook in your browser. Photo links follow their parent post when available.
- Friends list with search: open a profile, read its posts, or jump into a Messenger chat.
- Groups you belong to, with their posts read and answered the same way as the feed.
- Your own profile posts, and deleting your own posts.
- Post to your profile with an audience (public, friends, only me) and optional images.
- Search people, pages, groups, posts and events.
- Messenger: browse chats (including unread only), read conversations, send text and file attachments, unsend your last message, and unlock end-to-end encrypted chats with your Messenger PIN, which is never stored.
- OCR an image file with the OCR engine built into your operating system.
- F1 lists the keyboard commands; Ctrl+1 to Ctrl+8 switch views, F5 refreshes, Ctrl+N writes a post, Ctrl+F searches.
- Help, Check for updates downloads the new release, verifies its minisign signature, installs it and restarts.

Some things the `fb` backend cannot do are linked out instead of faked: voice and video calls open the conversation in Messenger on the web, adding or removing friends opens the person's real profile, and posting into a group opens the group's web page. Photo albums open in your browser. A few feed rows (often ads and suggestions) never expose a link to Facebook's own scripts; BlindFB marks those "(no link)" and cannot open them.

## Download and install

Grab the latest build from the [Releases page](https://github.com/serrebidev/BlindFB/releases).
For a version-by-version history, see the [changelog](CHANGELOG.md).

1. Download the archive for your system: `BlindFB.zip` (Windows), `BlindFB.tar.gz` (Linux) or `BlindFB-arm64.dmg` (macOS). Each has a `.minisig` beside it.
2. Extract or open it and run BlindFB. There is nothing else to install: on first run BlindFB downloads the matching `fb` backend release, verifies its SHA-256 checksum, and keeps it in its own app-data folder.
3. Sign in: BlindFB asks which browser to copy your Facebook session from, with your default browser marked. Pick the browser you are signed in to Facebook in, or sign in directly in the separate browser window BlindFB offers.

The managed backend is [serrebidev/facebook-cli `v0.1.5-blindfb.10`](https://github.com/serrebidev/facebook-cli/releases/tag/v0.1.5-blindfb.10): upstream 0.1.5 plus the BlindFB fixes that are in review upstream, including the staged Messenger history restore ([#15](https://github.com/hieu-lee/facebook-cli/pull/15)) and message reply/reactions with idle-tab freezing ([#16](https://github.com/hieu-lee/facebook-cli/pull/16)). Backend, Install or update backend reinstalls it at any time. BlindFB also finds an `fb` you installed yourself; the `BLINDFB_FB` environment variable overrides every other location.

### Antivirus false positives

BlindFB is not code-signed yet, and one of its documented features is copying your Facebook session from your own browser with your permission. Some antivirus products flag that combination heuristically — ESET NOD32 has reported the Windows build as `Win64/HackTool.Agent.XJ`, which is a false positive. Every release asset is signed with minisign: verify the `.minisig` beside the archive before running it, and only download from this repository's Releases page. If your antivirus flags BlindFB, add an exclusion for the folder you extracted it to and consider reporting the file as a false positive to your vendor, which is how heuristic detections get removed.

## Requirements

- A browser you are signed in to Facebook in: Google Chrome, Chromium, Brave or Microsoft Edge, including their Beta, Dev, Canary and Nightly channels, each with its own data folder. Firefox can supply a session, but one of the Chromium browsers is still needed as the engine. Backend, Install a browser installs a browser from its vendor.
- On Windows, cookie import reads the chosen browser's cookie store locally. Close that browser first if its database is locked. Beta and Nightly encrypted cookies may not import on Windows; signing in directly in BlindFB's browser window always works.
- Linux builds need Ubuntu 24.04 or a compatible Debian-based system (glibc 2.39 or newer). On first launch the Linux launcher offers to install the GTK and GStreamer packages the app needs through your package manager. Keep `blindfb` and `blindfb-bin` together.
- Facebook is read live, so a section's first load takes several seconds. After that, sections are cached and prefetched, and switching is instant.

## Privacy policy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

BlindFB has no telemetry, analytics or accounts of its own. It talks to Facebook through the `fb` backend and the browser profile you sign in with, under Meta's own policies, and checks this repository's GitHub releases for app updates. Your Facebook cookies are read locally from the browser you pick; they are never passed on a command line, written to logs, or sent anywhere except to Facebook itself by the backend's browser. Your Messenger PIN is typed into the backend and never stored.

## Building

```sh
cargo build --release -p blindfb
cargo xtask package   # dist/BlindFB.zip (Windows), dist/BlindFB.tar.gz (Linux),
                      # dist/BlindFB-<arch>.dmg (macOS), each with its .minisig
```

Linux build dependencies for wxWidgets are the usual GTK set (see `.github/workflows/ci.yml`). GitHub Actions builds signed packages for all three systems on `v*` tags and publishes when all expected assets arrive.

## Debug logging

BlindFB writes a `startup.log` in its app-data folder as the window opens, recording each startup step. If the app ever runs without showing a window, the last line of that log names the step it reached.

## Contributing

Pull requests are welcome. If BlindFB has been useful to you, open a PR with a fix or feature and I'll review it.

## License

BlindFB is under the [MIT license](LICENSE) — use it, change it, redistribute it, or package it for a distro repository, no permission needed.

BlindFB is an unofficial client and is not affiliated with Meta.

## Community and support

Report bugs and request features in [Issues](https://github.com/serrebidev/BlindFB/issues). For questions, feedback, and release news, join the [SerrebiProjects Telegram group](https://t.me/SerrebiProjects).
