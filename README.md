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
- Videos and reels play in the reader, with Play/Pause, Stop, seek and a volume slider, and every control reachable with Tab. A video shared from another site — a YouTube, Vimeo, Twitch or TikTok clip in a group post — plays there too; the player fetches that page itself and your Facebook session is never sent to it.
  Windows uses automatically installed mpv and its bundled codecs. Native NVDA controls keep the video surface out of Tab order.
- File, Open Facebook link (Ctrl+O) opens a post, video or reel URL directly.
- The final Load more list item (Enter, or Ctrl+L) appends the next posts, reels, results or chats and selects the first new item. Sections and searches open with 10 items (five for My posts and reels), then Load more requests another 10 (five reels); Facebook may return fewer. The news feed opens with 10 posts and reads the next 10 in the background once you are within three posts of the end. It reads only that far ahead because Facebook keeps just the last 10 to 25 posts of its feed page, and Not interested needs a post still there.
- External links come first, shared source links follow, and the current Facebook post stays last. Links have a context submenu and receive focus first in the reader.
- Facebook post menu (Post menu, or the context menu) shows Facebook's own menu for a post and every step it opens: Interested and Not interested, Save, notifications, Snooze, Hide, Report and its follow-up screens, using Facebook's own wording in your language. Interested and Not interested exist only on a post's card in the News feed, so they show for the posts Facebook still keeps there; other posts get the post's own menu.
- A shared post's row reads your words, then the original post.
- Login requests: Enter on a "Someone's trying to log in" notification shows the device, place and time, and Facebook's own two answers (Yes, it's me or No, it's not me; No is selected first).
- Notifications open their posts in the reader, including group post/comment links. Other destinations open on Facebook in your browser. Photo links follow their parent post when available.
- Friends list with search: open a profile, read its posts, or jump into a Messenger chat.
- Groups you belong to, with their posts read and answered the same way as the feed.
- Your own profile posts, and deleting your own posts.
- Saved posts: the posts you saved on Facebook, read, opened, reacted to and commented on like the feed's.
- Post to your profile with an audience (public, friends, only me) and optional images.
- Search people, pages, groups, posts and events from one dialog with a filter combo box.
- Marketplace: browse the listings feed (Enter opens a listing in your browser, Message seller from the context menu) and sell with photos or video.
- Messenger: browse chats (including unread only), read conversations, send text and file attachments, unsend your last message, and unlock end-to-end encrypted chats with your Messenger PIN, which is never stored.
- OCR an image file with the OCR engine built into your operating system.
- F1 lists the keyboard commands; Ctrl+1 to Ctrl+9 switch views, F5 refreshes, Ctrl+N writes a post, Ctrl+/ searches everything (posts, people, friends, photos, videos, pages, places, groups, events, Marketplace).
- Alt+Ctrl+F hides BlindFB to the system tray and brings it back. File > Show/hide hotkey changes the shortcut (or turns it off); the new one applies right away.
- In the post reader, Ctrl+C copies the selected comment, Ctrl+Shift+C copies the post and all comments, and Ctrl+Enter opens the first link in the comment. The feed also offers Group's posts on group posts.
- File > Keyboard shortcuts changes, removes or adds a shortcut for any menu command and for the post reader's comment keys. Pick Ctrl, Alt and Shift and a key from a list; BlindFB warns before taking a shortcut another command uses. Changes apply right away and are saved in `shortcuts.txt` in BlindFB's data folder.
- The video volume is remembered between videos and launches, and Help > Language offers French, Russian and Estonian, all fully translated.
- File, Check for updates downloads the new release, verifies its minisign signature, installs it and restarts. Releases come from Forgejo; when Forgejo cannot be reached, the update and the backend download come from the GitHub mirror instead.

Some things the `fb` backend cannot do are linked out instead of faked: voice and video calls open the conversation in Messenger on the web, adding or removing friends opens the person's real profile, and posting into a group opens the group's web page. Photo albums open in your browser. A few feed rows (often ads and suggestions) never expose a link to Facebook's own scripts; BlindFB marks those "(no link)" and cannot open them.

## Download and install

Grab the latest build from the [Releases page](https://git.serrebiradio.com/serrebi/BlindFB/releases) (mirror: [GitHub](https://github.com/serrebidev/BlindFB/releases)).
For a version-by-version history, see the [changelog](CHANGELOG.md).

1. Download the file for your system. Windows: `BlindFB-Setup.exe` installs BlindFB for your account only, without an administrator prompt, to `%LOCALAPPDATA%\Programs\BlindFB`, with a Start menu shortcut and an entry in Installed apps; `BlindFB.zip` is the portable copy. Linux: `BlindFB.tar.gz`. macOS: `BlindFB-arm64.dmg`. Each has a `.minisig` beside it.
2. Run the installer, or extract or open the archive, and start BlindFB. Uninstalling removes the program only; your sign-in and settings stay in `%LOCALAPPDATA%\BlindFB`. There is nothing else to install: on first run BlindFB downloads the matching `fb` backend release, verifies its SHA-256 checksum, and keeps it in its own app-data folder.
3. Sign in: BlindFB asks which browser to copy your Facebook session from, with your default browser marked. Pick the browser you are signed in to Facebook in, or sign in directly in the separate browser window BlindFB offers.

The managed backend is [serrebi/facebook-cli `v0.1.5-blindfb.55`](https://git.serrebiradio.com/serrebi/facebook-cli/releases/tag/v0.1.5-blindfb.55): upstream 0.1.5 plus the BlindFB work that is in review upstream, including reading dated messages again — Messenger labels anything older than today with its full date, and the sender/text split dropped every one of them, which made chat history read as blank lines ([#22](https://github.com/hieu-lee/facebook-cli/pull/22)) — the Messenger history restore that finds the real restore prompt — a single PIN field that submits itself when the PIN is complete ([#20](https://github.com/hieu-lee/facebook-cli/pull/20), on top of the staged restore in [#15](https://github.com/hieu-lee/facebook-cli/pull/15) and the link and long-banner entries in [#18](https://github.com/hieu-lee/facebook-cli/pull/18)), and never declines the restore when a read meets the prompt, instead waiting out Facebook's verification while keeping the page alive ([#21](https://github.com/hieu-lee/facebook-cli/pull/21)), message reply/reactions with idle-tab freezing ([#16](https://github.com/hieu-lee/facebook-cli/pull/16)), full search (every filter type), group join/leave with membership-question answers, and Marketplace browsing and selling; reactions are verified on the post before they are reported (the picker is the toolbar that appears after the hover, not any toolbar on the page), message reactions and replies can name an older duplicate of a message instead of always the newest, and group member lists read every profile link shape Facebook uses. Verified badges read as just "Verified". Backend, Install or update backend reinstalls it at any time. BlindFB also finds an `fb` you installed yourself; the `BLINDFB_FB` environment variable overrides every other location.

### Antivirus false positives

BlindFB is not code-signed yet, and one of its documented features is copying your Facebook session from your own browser with your permission. Some antivirus products flag that combination heuristically — ESET NOD32 has reported the Windows build as `Win64/HackTool.Agent.XJ`, which is a false positive. Every release asset is signed with minisign: verify the `.minisig` beside the archive before running it, and only download from this repository's Releases page. If your antivirus flags BlindFB, add an exclusion for the folder you extracted it to and consider reporting the file as a false positive to your vendor, which is how heuristic detections get removed.

## Requirements

- A browser you are signed in to Facebook in: Google Chrome, Chromium, Brave or Microsoft Edge, including their Beta, Dev, Canary and Nightly channels, each with its own data folder. Firefox can supply a session, but one of the Chromium browsers is still needed as the engine. Backend, Install a browser installs a browser from its vendor.
- On Windows, cookie import reads the chosen browser's cookie store locally. Close that browser first if its database is locked. Current Edge and Brave encrypt their cookies so only the browser itself can read them; when the cookie import cannot read a browser's store, BlindFB imports through that browser instead — tick "Allow remote debugging" if it asks. Signing in directly in BlindFB's browser window always works.
- Linux builds need Ubuntu 24.04 or a compatible Debian-based system (glibc 2.39 or newer). On first launch the Linux launcher offers to install the GTK and GStreamer packages the app needs through your package manager. Keep `blindfb` and `blindfb-bin` together.
- Facebook is read live, so a section's first load takes several seconds. After that, sections are cached and prefetched, and switching is instant.

## Privacy policy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

BlindFB has no telemetry, analytics or accounts of its own. It talks to Facebook through the `fb` backend and the browser profile you sign in with, under Meta's own policies, and checks this repository's Forgejo releases (or the GitHub mirror when Forgejo is unreachable) for app updates. Your Facebook cookies are read locally from the browser you pick; they are never passed on a command line, written to logs, or sent anywhere except to Facebook itself by the backend's browser. Your Messenger PIN is typed into the backend and never stored.

## Building

```sh
cargo build --release -p blindfb
cargo xtask package   # dist/BlindFB.zip + dist/BlindFB-Setup.exe (Windows, the installer
                      # needs Inno Setup 6 or 7), dist/BlindFB.tar.gz (Linux),
                      # dist/BlindFB-<arch>.dmg (macOS), each with its .minisig
```

Linux build dependencies for wxWidgets are the usual GTK set (see `.github/workflows/ci.yml`). GitHub Actions builds signed packages for all three systems on `v*` tags and publishes when all expected assets arrive. Full process for this PC, including Forgejo releases while GitHub is suspended: [docs/local-build.md](docs/local-build.md).

## Alerts

View > Voice alerts and View > Sound alerts tell you about new notifications and unread chats while BlindFB is open. Both start off. When one is on, BlindFB checks Facebook every two minutes. Voice alerts read the notification or message aloud with your system's voice, separately from your screen reader. Sound alerts play a short chime. Nothing that was already there when you turned alerts on is read out. On Linux they use `spd-say` and `canberra-gtk-play` when installed.

View > Voice alert when a list loads and View > Sound alert when a list loads do the same for lists: when a list you opened finishes loading, the voice one says its name and how many items it has, and the sound one chimes, so you do not have to check the status bar. Turn on either one, or both; both start off and each choice is remembered. Background reads, like sections warming up at launch, never announce.

## Debug logging

BlindFB writes a `startup.log` in its app-data folder as the window opens, recording each startup step. If the app ever runs without showing a window, the last line of that log names the step it reached.

To report something that did not work, check Help > Debug logging, repeat what failed, then use Help > Save debug report and send the developer that file. The report holds BlindFB's version, your system, what BlindFB ran, how long it took and the errors it got, plus Facebook links you opened or reacted to. It does not hold your post text, messages, file names or PIN. Debug logging stays on until you uncheck it.

## Contributing

Pull requests are welcome. If BlindFB has been useful to you, open a PR with a fix or feature and I'll review it.

## License

BlindFB is under the [MIT license](LICENSE) — use it, change it, redistribute it, or package it for a distro repository, no permission needed.

BlindFB is an unofficial client and is not affiliated with Meta.

## Community and support

Report bugs and request features in [Issues](https://github.com/serrebidev/BlindFB/issues). For questions, feedback, and release news, join the [SerrebiProjects Telegram group](https://t.me/SerrebiProjects).
