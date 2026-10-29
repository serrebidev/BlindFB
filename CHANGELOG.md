# Changelog

Readable release history for BlindFB. When adjacent releases were part of the
same fix stream, they are combined with a version range such as
`v0.3.1-v0.3.2`.

## v0.6.7 - 2026-10-29

- Windows playback uses automatically installed mpv and bundled codecs, with native NVDA controls and no mediaCtrl Tab stop.
- Enter on the final Load more list item appends the next batch and selects the first new item. Feeds request 100 items; reels request 20.
- Group notifications resolve their post and comment links. Other notification destinations open directly on Facebook; photo links follow their original post when available.
- Reader links are available while loading and keep their selection on refresh. External links come first, source posts follow, and the current Facebook post stays last.
- Shared feed posts identify the original author. Captionless reels use the available video title.
- Stop main-window timers before destruction to prevent the native timer crash. Retry temporary Windows locks when replacing the backend.
- Verify Microsoft Edge session import with live profile/feed reads and persistence after browser restart.
- Build and sign all three platform packages through GitHub Actions before publishing.

## v0.6.3 - 2026-10-29

- Reels (Ctrl+8) gets a cached list: browse full captions with the arrow keys, Enter opens the player and playback starts at once, and Escape returns to the same reel.
- Reel notifications open their exact videos when Facebook supplies the IDs; otherwise the account's reels open and the fallback is explained.
- Posts show expanded text, including shared-post text, and the post's links and playback controls are reachable with Tab.
- Video and reel playback gains Play/Pause, Stop, seek and a volume slider; the playback tools install themselves when first needed.
- Fix the multiple-visible-post error on shared posts, and make the first feed load faster.
- The Linux launcher offers to install the GTK and GStreamer packages the app needs.

## v0.6.2 - 2026-10-29

- Help, Check for updates now downloads the new release, verifies its minisign signature, installs it and restarts; a signature that does not match is never installed.
- First release in the signed updater layout: copies of 0.6.1 or older must be replaced once by hand, and from 0.6.2 on updates happen in the app.

## v0.6.1 - 2026-10-28

- Reuse in-flight background section loads instead of starting duplicates, and reuse HTTP connections between backend calls.
- Validate long comment threads in linear time.
- Prevent duplicate message sends, and keep a completed send or an Unsend from clearing another chat's draft or changing the active view.
- Reset chat context and discard stale results when switching Facebook accounts.
- Mark rows that have no post link directly in the list.
- Read live Firefox cookies through SQLite, without temporary cookie database copies.
- Hide backend console windows on Windows, and open web links without shell interpretation.
- Upgrade ZIP handling to zip 9.1.3, resolving six reported dependency vulnerabilities, and support installer downloads larger than 10 MiB.

## v0.6.0 - 2026-10-28

- Share is the second item in post context menus, in the news feed, Feeds, profile and group posts, search results and notifications; it shows the audience and takes optional text.
- Load more (Ctrl+L) appends items without moving your selection or repeating rows, in feeds, post lists, search results, notifications, friends and chat lists.
- The managed backend moves to the pinned build with the hidden feed link and Messenger restore prompt fixes.

## v0.5.1 - 2026-10-28

- Instant section switching: sections are cached and prefetched in the background.

## v0.5.0 - 2026-10-28

- Post reader: Enter on a post opens it with its comments and replies and a comment box; the context menu replies to, likes and copies the exact comment.
- Feeds (most recent) view, context menus on every list item, and sharing through Facebook's own share dialog.

## v0.4.0 - 2026-10-28

- Sign in without Google Chrome: when Chrome or Chromium is unavailable, BlindFB starts a separate Brave or Edge profile and signs in there.
- On Windows, the Facebook session is imported from the browser you pick with rookie-cookies; cookie values never reach the command line or the logs.
- Sign-in errors are preserved, and a failed or cancelled setup no longer loads the feed.

## v0.3.2 - 2026-10-28

- Sign-in supports browser channels as their own entries: Brave Beta and Nightly, Chrome Beta, Dev and Canary, and Edge Beta and Dev, each with its own data folder.
- Only stable Chrome and Chromium use the backend's own login; every other browser signs in by importing from the exact install you picked.

## v0.3.1 - 2026-10-28

- Write a startup log as the window opens, and raise the window after showing it, to catch the Windows launch failure where the process ran with no window.

## v0.3.0 - 2026-10-28

- Sign in from Chrome, Brave, Edge or Firefox, with your default browser marked in the list.
- Backend, Install a browser installs a missing browser from its vendor.

## v0.2.0 - 2026-10-27

- The `fb` backend installs itself on first run: BlindFB downloads the matching facebook-cli release, verifies its SHA-256 checksum, and keeps it in its own app-data folder.
- Guided browser sign-in replaces manual backend setup.
- TLS is verified against the platform certificate store, not only the Mozilla roots.
- Features the backend cannot do (voice and video calls, adding friends, posting into a group) link out to the right page on the web.

## v0.1.0 - 2026-10-27

- First release: an accessible Facebook and Messenger client for Windows, macOS and Linux, written in Rust with native wxWidgets controls.
- News feed, notifications, friends, your own profile posts, search and groups, read live through the `fb` command-line backend in a keyboard-first window.
- Messenger chats: read conversations, send text and file attachments, unsend your last message, and unlock end-to-end encrypted chats with your Messenger PIN.
- Post to your profile with an audience and optional images; react to and comment on posts; delete your own posts.
- OCR an image file with the OCR engine built into your operating system.
