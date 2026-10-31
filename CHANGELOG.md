# Changelog

Readable release history for BlindFB. When adjacent releases were part of the
same fix stream, they are combined with a version range such as
`v0.3.1-v0.3.2`.

## v0.6.15 - 2026-10-30

- Controls now follow what you are doing: buttons that do not apply to the current view or selection are disabled instead of sitting there live. Post actions (Open, React, Comment) only work in post lists, Delete only in My Posts, a person's Profile, Posts and Message only where a person is selected, and the message box with its Attach, Attachments and Send buttons only inside a chat.
- Messages have a proper context menu now: right-click a message in a chat — in the Chats view or in a chat window — for Reply and React (Like, Love, Care, Haha, Wow, Sad, Angry), alongside Copy. Reply quotes the message above the composer; choosing Cancel reply before sending clears the quote. Reacting and replying act on the newest message containing the text you picked, the same rule Unsend already used.
- Attachments you add to a message can be removed again: the new Attachments button beside the message box shows how many files are attached and lets you remove one or all of them, without leaving the chat. Before this, a file over Messenger's 25 MB limit failed not just its own send but every later message too, until the chat was closed and reopened — a failed send now also says the files are still attached and where to remove them.
- Pressing Enter on a post now lands on the post text itself, ready to read, instead of on the post's link list.
- Going back works: with a list focused, Backspace or Escape returns to the previous list — from a chat back to the chat list, from someone's posts back to where you were. Typing in the message box or any text field is unaffected.
- The Recent feed now says when each post was published: the time comes right after the author's name in the list, before the post text.
- Links in a post open in your default browser: pressing Enter on a link in the post's link list works now, and the Open link button no longer opens your Documents folder in File Explorer instead of the link.
- Less CPU, steadier screen reading: arrowing through a list no longer rebuilds the details pane and link list for every row you pass — only the row you stop on is built. In the backend, every parked background tab is now frozen while it waits (the feed tab already was), so idle Facebook tabs cost no CPU; a tab wakes the moment a command needs it.
- Backend updated to facebook-cli 0.1.5-blindfb.10.

## v0.6.14 - 2026-10-30

- Messenger unlock fixed for real accounts: on a real account Facebook's restore opens in stages — first an explanation, then a choice of how to restore — and the PIN box only appears after those. BlindFB's backend only recognised a dialog that already contained the PIN box, so unlocking failed with "PIN restore dialog not found" before the PIN was ever typed. The unlock now walks the stages (choosing the PIN method, never the reset path), opens the restore offer when it sits in the chat pane instead of a dialog, and finds the PIN field on full-page restore screens too. If Facebook changes the flow again, the error now names the dialogs and buttons actually on screen instead of just reporting the dialog missing. A correct PIN is also no longer failed by the words "try again" in the dialog's help text.
- Chats: pressing Enter on a chat or a group chat now opens that conversation in its own window, like groups. The window shows the messages (newest last), with Load more for older history, Refresh, and a composer — Enter or Send sends, and files can be attached. If Messenger is locked, the window says so and points at Chat, Unlock encrypted chats.
- Backend updated to facebook-cli 0.1.5-blindfb.9.

## v0.6.13 - 2026-10-30

Seven fixes contributed by Michał Dziwisz (PRs #1-#7), all in chats, groups, the reader and the player:

- Chats: a chat row with fewer fields than expected no longer crashes the window when the reader moves onto it.
- Chats: messaging a friend or commenter now opens their thread by its backend reference, not by display name, so two people with the same name can no longer open each other's thread.
- Chats: unlocking Messenger from the menu now reloads the chat list, instead of leaving the empty list on screen after a successful unlock.
- Groups: a member action on a row missing its profile link or name now says why and keeps the group window open, instead of closing it onto nothing.
- Reader: the reader's buttons no longer announce "and Play video" — the mnemonic marker is stripped from their accessible names.
- Reader: "Copy post text" is disabled until the post has loaded, so the "Loading post and comments..." placeholder can no longer land on the clipboard announced as a successful copy.
- Player: a rejected player command (for example stopping a video that just ended) no longer disables the seek controls or reports a failed playback while the video is still playing.

## v0.6.12 - 2026-10-30

- Messenger unlock fixed: entering your PIN in BlindFB now types it the way you would, digit by digit. On Messenger surfaces that give each digit its own box, the old single paste only ever landed the first digit, and surfaces waiting for a Continue or Restore tap never got one — the right PIN still left chats locked. If Facebook rejects the PIN, BlindFB now shows Facebook's own message instead of a generic failure.
- Less CPU: the feed read-ahead tab the backend keeps in the background is frozen while it waits at its limit instead of running at full speed the whole session. Refills start just as fast as before.
- Backend updated to facebook-cli 0.1.5-blindfb.8.

## v0.6.11 - 2026-10-29

- Chats fixed: pressing Enter on a chat now opens the thread itself instead of matching the words on screen, so the "ambiguous 'Active now'" error is gone. Group chats and rooms open the same way and are marked as group chats in the list.
- Groups: pressing Enter on a group opens a group window with the group's posts. Tab reaches the members list, which shows the whole list (capped at 500, and says so when capped). A member's context menu offers View profile, Message, Add friend and Open on Facebook; Enter on a post opens it in the main window.
- Comments: the context menu on a comment now offers the commenter's profile and Add friend right there, in the post window and in the main window, alongside Message commenter and Copy text.
- Backend updated to facebook-cli 0.1.5-blindfb.7.

## v0.6.10 - 2026-10-29

- Groups fixed: the list now comes from the backend's new `groups` command, which reads your joined-groups list and scrolls it in full. Before, Groups scraped the groups feed page, so it mixed in suggested groups, missed most groups you belong to, and often showed post text instead of group names. Group details now include the group's privacy and size line, and Load more works for groups. Backend updated to facebook-cli 0.1.5-blindfb.6.

## v0.6.9 - 2026-10-29

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
