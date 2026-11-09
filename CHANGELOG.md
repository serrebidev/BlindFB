# Changelog

Readable release history for BlindFB. When adjacent releases were part of the
same fix stream, they are combined with a version range such as
`v0.3.1-v0.3.2`.

## v0.7.36 - 2026-11-08

BlindFB gets its own permanent signing key (public key F806D88DE00499EF),
replacing the temporary v0.7.35 key from the Forgejo move. Every product now
signs with its own key, and every secret and password lives in the owner's
key store, so a lost machine never breaks releases again. Install this one
manually: v0.7.35 verifies with the old key and cannot offer this update.
No app changes since v0.7.35.

## v0.7.35 - 2026-11-08

Update signing rekeyed. BlindFB's original minisign secret existed only as
a GitHub Actions secret and was lost with the GitHub account suspension, so
this release starts a new signing key (public key F69FBB62D15E51DB). Builds
of v0.7.34 and older cannot verify this or later releases - install this
one manually; updates work normally from here. Also carries the v0.7.34
fixes, which never shipped publicly: Reels and Places in Views (issues #28
and #29, reported by MichalSuchy). Backend v0.1.5-blindfb.41.

## v0.7.34 - 2026-11-08

Fixes Reels and Places in Views (issues #28 and #29, reported by
MichalSuchy). Backend v0.1.5-blindfb.41.

- Places no longer always comes back empty. The places page is a
  legacy city directory with no main landmark on it, and the backend
  waited for its list inside that landmark, so nothing could ever
  appear. Places now lists the directory's "Things to do in …" city
  pages, gathered from the whole page.
- Reels no longer comes back empty when Facebook itself is not in
  English, French or Vietnamese. The reel card was recognised by its
  English-style "Comment" button wording, so in any other language no
  card was ever found. The card is now also recognised by its shape:
  the video's container that holds the author's link and the action
  buttons, whatever language they are in.

## v0.7.33 - 2026-11-08

The Author profile's last row (issue #27, reported by MichalSuchy).
Backend unchanged, v0.1.5-blindfb.40.

- The last row of a profile read as a bare "actions" and pressing
  Enter on it did nothing. That row lists the actions Facebook shows
  on the profile; it now reads in full, as "Actions: …", and Enter on
  it opens the person's actions menu (posts, message, add friend,
  open on Facebook).
- Enter works on every profile row now: the name opens the person's
  posts, the address row opens the profile in your browser, and the
  information rows say they are information only instead of staying
  silent. A profile's context menu (Applications or Shift+F10) offers
  the same actions as a friend's: posts, message, add friend, and
  open on Facebook.

## v0.7.32 - 2026-11-08

Fixes the system tray from v0.7.31 (issue #26, reported by MichalSuchy).
Backend unchanged, v0.1.5-blindfb.40.

- Ctrl+T now restores as well as hides. It was only a menu shortcut, and a hidden window receives no menu shortcuts; on Windows it is now a system-wide hotkey, so the same Ctrl+T brings BlindFB back.
- Opening the tray icon's right-click menu no longer crashes BlindFB — the menu was being freed while the icon still pointed at it.
- Quitting BlindFB now removes its tray icon instead of leaving a ghost icon behind.

## v0.7.31 - 2026-11-07

Two requests from MichalSuchy (issues #25 and #26).
Backend v0.1.5-blindfb.40.

- Events, Pages and Places join the Views list (and the View menu).
  Events and Places open on Facebook in your browser; a Page opens
  its posts in BlindFB, like a profile. The backend learned three
  new commands — events, pages and places — that read each section's
  own page the way Groups already did.
- Hide BlindFB to the system tray and bring it back: Ctrl+T or
  File > Hide to system tray hides the window; double-click the
  tray icon, or use its Show / hide menu item, to toggle it back.
  The tray menu also has Exit.

## v0.7.30 - 2026-11-07

Search works the way it should (MichalSuchy's report, issue #24).
Backend v0.1.5-blindfb.39.

- Search results no longer read "(no link)". People, pages, groups,
  events and places were marked unopenable even though Enter opened
  them; the mark now appears only on a result that truly has nothing
  to open.
- Every search result's context menu can open it on Facebook in your
  browser, and people and pages gained a Posts item, so you can read
  someone's posts straight from the results. The backend now sends
  each result's own web address along, which is what makes this
  possible.
- Back from a search result restores the results list. Before, it
  landed on an empty list with no way back short of searching again.

## v0.7.29 - 2026-11-07

The translation engine moves to patois 0.3, the release its author
announced for other apps to adopt. Backend v0.1.5-blindfb.38,
unchanged.

- Your translations are untouched: the French catalog is the same
  file with the same words, now read by patois 0.3. The libraries
  that share the catalog moved with it (wx-utils 0.5 for the dialog
  helpers, ship-shape 0.6 for the update checker), so their text
  follows the same language setting as the rest of the app.
- Under the hood, translatable strings are now plain function calls
  instead of macros. That is the form standard gettext tools such as
  Poedit collect, which is how the next language gets added.

## v0.7.28 - 2026-11-07

Links inside comments, and release notes that say what changed.
Backend v0.1.5-blindfb.38.

- New: browse the links in a comment. In the post reader, selecting a
  comment that has links points the links list at that comment — its
  title says whose links they are — and the comment's context menu
  gains the same Links submenu as the post's. In the comments list
  view, a comment's links join the link list under its details.
  Facebook only shows a trimmed label for a link inside a comment, so
  the real address used to be lost; BlindFB now carries it, with
  Facebook's redirect unwrapped and tracking parameters stripped.
- Release notes are now the per-version sections of this changelog,
  the way BlindRadio ships them: Check for updates shows what actually
  changed instead of an empty page. The notes for v0.7.21 to v0.7.27
  have been filled in on GitHub too, and from now on a release whose
  section is not written does not publish.

## v0.7.27 - 2026-11-07

News feed cursor (issue #23): opening an author's posts or profile
from the feed and coming back with Escape landed on the first feed
item. Each list's cursor is now remembered when the list is left —
before its rows are cleared for the next list — so Back returns to
the item you were on. Backend v0.1.5-blindfb.37 (unchanged).

## v0.7.26 - 2026-11-06

Comments (issue #22): replies hidden behind Facebook's "View replies"
buttons now load automatically when a post opens, and each reply row
names the comment it answers. After you send a comment, the reader
lands on your new comment in the list so you hear it read back, and
commenting from the list views now says "Comment sent." and refreshes
an open comments view instead of a bare "Done". Backend
v0.1.5-blindfb.37 (unchanged).

## v0.7.25 - 2026-11-06

Opening a post in the reader, and sending a comment from it, failed on
Windows with "could not run fb.exe: The filename or extension is too
long" and your draft was kept. The reader's script had grown past
Windows' 32,767-character command-line limit, so the backend never
started. Long reader scripts are now sent to the page in pieces and
run whole. Backend v0.1.5-blindfb.37 (unchanged).

## v0.7.24 - 2026-11-06

Posting and the feed, fixed for non-English interfaces (issues #20 and
#21, thanks Michal). Backend v0.1.5-blindfb.37.

- Posting failed with "audience option not found" whenever Facebook was
  not in English, French or Vietnamese: the backend's audience picker
  only knew those languages. It now knows every language the Post
  button knows — Czech and Slovak first, then the rest.
- Sponsored and suggested feed rows that carry no link to open no
  longer appear in the feed list. They could not be opened, reacted to
  or commented on, so listing them only dead-ended on Enter.

## v0.7.23 - 2026-11-06

BlindFB in French (thanks math65). Backend v0.1.5-blindfb.35.

- New: the whole interface — menus, dialogs, lists and messages — in
  French, chosen from the same language setting as the other
  translations.
- The backend reads French Facebook pages: times, counts, the verified
  badge, presence and Marketplace, plus the long French reaction labels
  ("J'aime" and its selected form).

## v0.7.22 - 2026-11-06

No console window on Windows. The app is now linked as a Windows GUI
application, so launching it no longer opens a command prompt window
beside it — the same fix as the other apps. Backend v0.1.5-blindfb.34
(unchanged).

## v0.7.21 - 2026-11-05

List-loaded voice and sound alerts. Backend v0.1.5-blindfb.34 (unchanged).

- New: View > Voice alert when a list loads and View > Sound alert when a list loads (issue #17, thanks Michal). When a list you opened finishes loading, the voice alert says its name and how many items it has — for Load more, how many new items arrived and the new total — and the sound alert plays a short chime. Turn on either one, or both, separately from the notification alerts. Both start off, and each choice is remembered. Turning one on plays it once, so you know it works.
- Background reads never announce: sections warming up at launch, the News feed reading ahead, and a cached section refreshing itself all stay silent. Only a list you are waiting for announces.

## v0.7.20 - 2026-11-05

Voice and sound alerts, and Author's profile can no longer open the wrong person. Backend v0.1.5-blindfb.34.

- New: View > Voice alerts and View > Sound alerts (issue #11). While BlindFB is open, they tell you about new notifications and unread chats. Voice alerts read the notification or the message aloud with your system's voice, separately from your screen reader, and also show it in the status bar. Sound alerts play a short chime. Turn on either one, or both. Both start off, and each choice is remembered. Turning one on plays it once, so you know it works.
- When an alert is on, BlindFB checks Facebook every two minutes in the background, and never ahead of something you opened. Nothing that was already there when you turned alerts on is read out. When several things arrive at once, the first three are read, then how many more. A check that fails, for example while Messenger is locked, stays silent and tries again later.
- Author's profile and Author's posts could open the wrong person for a post whose author has no profile link. BlindFB took the first profile on the card, which could be someone named in the post or in a comment. Now only the post's header counts, and a post without an author link falls back to searching by name as before.
- Alerts on Windows, including speech, sound and the checks, were tested on this release. On macOS and Linux they use the system's `say` and `afplay`, or `spd-say` and `canberra-gtk-play`, which were not tested there.

## v0.7.19 - 2026-11-05

Comments and replies with emoji are confirmed and read properly. Backend v0.1.5-blindfb.31. This release also brings the fixes listed under v0.7.16 and v0.7.17, which were never published on their own because their backend builds failed.

- If you installed v0.7.18, please update. It was online for about two hours on November 5 and then withdrawn, because it asked for a backend version that had not been published, so the backend could not be downloaded. This release uses one that exists.

- A comment or reply could go through while BlindFB said "Submission was not confirmed in the selected thread" and kept your draft. Facebook draws most emoji as pictures, so the text BlindFB read back was missing them and did not match what you wrote. If you saw this error, check the post before sending again: your comment is probably already there.
- Confirmation now compares the words of your comment, so other changes Facebook makes don't cause the same error, such as turning :) into an emoji, shortening a link or cutting long text behind See more. It still waits until Facebook has accepted the comment, not just while it shows "Posting...".
- Comments in the post reader keep their emoji. Before, they were dropped, an emoji in a sentence split it across two lines, and a comment that was only an emoji looked empty.
- Tested by sending a real comment and a real reply with an emoji on live Facebook, then deleting them. Replying to someone else, where Facebook starts your reply with their name, was tested on live Facebook only up to the point of sending.

## v0.7.17 - 2026-11-04

Cursor position, author actions and post details in the News feed. Backend v0.1.5-blindfb.29.

- Coming back to a list (Back, or returning from a post's comments) put the cursor on the first item instead of the item you were on (issue #15). Each list now remembers its selected item and restores it when you return.
- Author's posts and Author's profile on a shared post in the News feed failed with "no person matches" (issue #14). The author line of a shared post names two people, and searching for the whole line matches nobody. The menu now acts on the poster, the first name. Posts whose author link the backend reads keep using that link directly, as since v0.7.16.
- The details pane's Time line could be empty (issue #16). The backend now falls back to the timestamp link's own text when neither of its other time sources reads. The counts line now names each number: "Reactions: 0, comments: 0, shares: 0" instead of a bare "0/0/0".

## v0.7.16 - 2026-11-04

Author's profile and Author's posts work on group posts in the News feed. Backend v0.1.5-blindfb.28.

- On a group post in the News feed, Author's profile failed with "profile not found / not accessible" and Author's posts failed with "page did not load" (issues #12 and #13). A group post's author line merges the group name into the author's name, so looking the person up by that name could not work.
- Feed posts now carry the author's own profile link, the same way comments already did. Author's profile, Author's posts, and Add friend use that link directly instead of searching by name, which also means a common name can no longer open the wrong person's profile or posts.

## v0.7.15 - 2026-11-04

Replying to a comment works again. Backend unchanged, v0.1.5-blindfb.27.

- Replying could fail with "No unique editor inside the selected comment thread. Nothing was sent." Facebook now draws the reply box beside the comment instead of inside it, and BlindFB only looked inside. It now uses the box Facebook focuses when you press Reply, and refuses it unless it belongs to the thread you selected.
- A top-level comment could fail with the same error when any reply box was open on the post, including one left open by a failed reply. BlindFB now tells the post's own comment box apart from reply boxes.
- When you reply to someone who has replies under their comment, or to a reply, Facebook starts your reply with that person's name. BlindFB used to treat the name as an unsent draft. It now keeps the name, as Facebook does, and types your words after it. Text you left in the box yourself is still never overwritten.
- A sent reply could be reported as unconfirmed when it landed under the thread's top comment, or when Facebook removed its reply box after sending. Both now count as sent.
- All of this was checked on live Facebook up to the point of sending, on posts with and without open threads. No comment was actually published in testing, so please report it if a reply goes through but BlindFB says it could not confirm it.

## v0.7.14 - 2026-11-04

Posts that were published no longer come back as errors. Backend v0.1.5-blindfb.27.

- A post could report "post may have been published, but no verified receipt: unique creation identity missing" and still publish. Facebook sometimes accepts a post and publishes it moments later, and the backend read that as a failure. It now waits up to a minute for the post to appear. If the post is still not there, BlindFB says Facebook accepted it and you do not need to post it again.
- A post containing an emoji could report the same kind of error, because Facebook draws most emoji as pictures and the check compared only the text. The check now counts them. Post text in the news feed and the post reader keeps its emoji for the same reason.
- If Facebook's first attempt to publish failed and Facebook retried by itself, the backend reported failure while the retry went through. It now waits for the retry.
- When BlindFB really cannot confirm a post, the message now says in plain words to check My posts before posting again, instead of naming a command-line tool.
- A successful post now says "Your post is published." instead of showing the backend's raw result line.

## v0.7.13 - 2026-11-03

Posting opens the real composer on more accounts, and a new debug report makes other problems easier to fix. Backend v0.1.5-blindfb.22.

- Posting could fail with "Post button not found; nothing was posted", depending on how your profile is laid out. On some layouts the backend can open the "Share a thought" note dialog instead of the post composer. It now finds the composer by its layout in any language and refuses any other dialog before typing. The fix was reproduced and tested with a stand-in page, not on an affected account, so please report it if the error continues.
- The Post button is found in any language, and a Next step before Post is followed. When a post can't continue, the error names the dialog, the page language and the visible buttons, never your text.
- Help has a new Debug logging checkbox and Save debug report. Check the box, repeat what failed, save the report and send the file to the developer. It holds BlindFB's version, your system, what BlindFB ran, how long it took and the errors it got, plus Facebook links you opened or reacted to. It does not hold your post text, messages, file names or PIN.

## v0.7.12 - 2026-11-03

Reaction verification understands Facebook's author-qualified button labels. Backend v0.1.5-blindfb.21.

- A successful Like could report "the reaction did not land" because Facebook now names both the reaction and the post's author in the button. Verification reads the reaction before the author's name, recognizes the neutral Like button, and keeps repeated Like and Remove reaction requests from toggling the state again. Removing a reaction succeeds only when the button returns to its neutral state. The exact reported RNC label has a regression test ([upstream PR #35](https://github.com/hieu-lee/facebook-cli/pull/35)).
- Posting refocuses the editor before typing and checks that the draft matches the requested text before publication. Publish controls work with both native buttons and role buttons. A timeout distinguishes an empty composer, a missing or ambiguous Post button, a disabled button, and a visible progress indicator instead of guessing that an upload is running. The original intermittent posting timeout was not reproduced; these changes improve text entry, prevent changed drafts from being posted, and make any remaining failure diagnosable.

## v0.7.9 - 2026-11-01

Reactions land on the item you picked, group members show up, and the group window loads at news-feed speed. Backend v0.1.5-blindfb.18.

- Reactions sometimes landed on the wrong thing, or nowhere, while BlindFB reported success. The reaction picker was chosen as the last toolbar on the page with enough buttons, so on busy post pages the click could go to the comment box's toolbar instead; and nothing checked the result. The picker is now the toolbar that appears after hovering the Like button, the wait polls instead of guessing a fixed delay, and the reaction is verified on the post before BlindFB says it landed — if it did not land, you get an honest error, not a false success. Removing a reaction is verified the same way.
- Reacting to a message or replying to one always acted on the newest message with that text, so an older "ok", "thanks" or a link sent twice could not be picked — the newest copy got the reaction instead. Both the chat window and the Chats view now name the exact message you selected, counting duplicates from the newest.
- Group members never appeared. Facebook links members in two shapes the backend did not recognise (a /people/ address, and an address inside the group's own path), and those members were silently dropped from the list. Both shapes now resolve, for every group.
- The group window loads like the news feed: the first 10 posts and 50 members appear as soon as they are ready, then the rest (50 posts, up to 500 members) fills in behind you without moving your place in either list. Before, the window waited for everything — the full member scroll included — before showing anything.
- The Linux download runs on current systems again. It was built on a runner so new that it refused to start on Debian 13 and Ubuntu 24.04 LTS (it demanded a newer system library than those ship). The Linux build is pinned to Ubuntu 22.04, whose binaries run on every current distribution.

## v0.7.8 - 2026-10-31

Smaller live reads reduce loading delays, and recommended reels work with Facebook's counted Comment buttons. Backend unchanged, v0.1.5-blindfb.16.

- Sections and searches open with 10 items; My posts opens with five. Reading several screens before showing the first one cost unnecessary scrolling. On this account, notifications fell from 6.26 seconds for 50 to 1.92 seconds for 10; My posts fell from 12.01 seconds for 20 to 3.75 seconds for five.
- Load more adds 10 items instead of 50. On this account, the next My posts batch took 8.55 seconds instead of about 33 seconds. Existing items retain their order and reading position.
- Reels loads five at a time instead of 20. A measured batch fell from about 22 seconds to eight seconds, including opening its page. Exact groups of reels from notifications retain all their known links.
- Background prefetch leaves a read slot free for the section you open. Opening a queued prefetch promotes that same request, without starting it twice. Superseded requests are checked again before they reach Facebook.
- Facebook now names some reel buttons “Comment, 6 comments” instead of “Comment.” Both the reels list and reader recognize those labels, which previously caused an empty list or a reader timeout.
- Marketplace listings gain Copy link and Share on Facebook in the context menu. An item's link now comes from the field that holds its target, not its label — Copy link on a listing previously read the title field and never found a link. Share on Facebook opens Facebook's share page in your browser with the listing address filled in; items whose target is only an internal reference have no link to copy or share.

## v0.7.7 - 2026-10-31

Two fixes: videos shared from other sites now play, and arrowing through the Views list no longer starts a read for every section it passes. Backend unchanged, v0.1.5-blindfb.16.

- A video shared from another site plays in the reader. A group post carrying a YouTube, Vimeo, Twitch or TikTok clip offered nothing: the reader only recognised Facebook video links, and the player refused any video that was not on facebook.com. The reader now offers the shared video as the post's video and lists its link with the others, and the player opens it. The player fetches that page itself with the yt-dlp it already keeps, so nothing about the video is resolved in the app — and your Facebook session is never handed to the other site. Sites the player accepts: YouTube, Vimeo, Dailymotion, Twitch, TikTok, Streamable and Rumble.
- Arrowing down the Views list reads the section you stop on. Each selection change used to start its read immediately, so arrowing from News feed to Chats started reads for Notifications, Friends and My posts as well, and Chats then waited behind them — one keypress could cost half a minute. A section is now read once the selection rests about half a second, so a run of arrow keys costs one read.

## v0.7.6 - 2026-10-31

A speed release. Nothing here changes what BlindFB reads; it asks Facebook smaller questions at better times. Backend unchanged, v0.1.5-blindfb.16.

- Launch no longer drags. The background warm-up used to read five sections at 100 items each right after the first posts appeared — Feeds (most recent) and My posts each scroll a long Facebook tail, so they held both read slots for a minute or more while Notifications, Friends and Chats waited behind them. It now warms the three sections a session actually starts with, at 50 items. Measured on this account: everything warm in 41 s instead of 171 s.
- First pages are half the size. The backend scrolls a live Facebook page per item it has to add, so a page of 100 costs several times a page of 50 — Friends took 7.5 s for 100 and 4.4 s for 50 on the same account. Every section now opens with 50 (My posts with 20; the news feed keeps its 10 and grows on demand).
- Load more on the news feed is instant. The backend keeps a 60-post read-ahead buffer of its own, so asking for 50 fits inside it; asking for 100 spilled past the buffer and had to scroll. After a minute of reading, Load more went from 15.4 s to under a tenth of a second.
- The feed's second page no longer blocks anything. It used to run as a normal read of 100 posts immediately after the first page — the whole load happened before anything else could use a slot. It is now a 30-post background read-ahead, which fills the list while you read the first posts.
- A section you open outranks the warm-up. The two read slots now go to whatever you last asked for: a background prefetch stands aside rather than taking the last free slot, so opening Chats never waits behind a warm-up that was already running.

## v0.7.5 - 2026-10-31

A bug-audit release: chat history reads again, and four smaller faults found in the same pass are gone. Backend v0.1.5-blindfb.16.

- Chat history reads again. Messenger labels any message older than today with its full date — "At September 5, 2026, 10:24 PM, You: ..." — and the backend split that label at the first comma, which is the date's own. Every dated message was thrown away, so a thread listed its dates and then blank `me:` lines with no sender and no text. That looked exactly like chats that could not be decrypted; they were decrypted, and their text is back ([#22](https://github.com/hieu-lee/facebook-cli/pull/22)).
- Unlock says why it found nothing. When no restore prompt exists, the backend no longer returns a wall of your chat list — names and message previews read back inside an error — and it now explains that a profile which once answered "Don't restore messages" keeps that answer, so Facebook will not offer the PIN prompt there again. Chats that profile already restored still read.
- A locked thread offers the unlock dialog. The backend reports a thread it could not open as "e2ee chat locked", which BlindFB did not recognise as a lock, so you got a dead-end error instead of the PIN prompt.
- Feed authors read once. Facebook emits an author's parts plus the assembled name, so a page called "Grok by SpaceXAI" read as "Yury. Ai and Grok by SpaceXAI / Yury. Ai and Grok by SpaceXAI".
- Notifications: one odd row no longer empties the list. A notification without a Facebook target keeps its text and reads "(no link)", and a row this build does not understand is skipped instead of failing the whole section.
- A reply quote stays in its chat. Replying in one conversation and opening another carried the quote over, so the next send quoted a message from the wrong thread.
- A chat window's status no longer counts its date headers as messages.

## v0.7.4 - 2026-10-31

The unlock fix, finished. Two deeper faults kept Messenger history locked even with the right prompt finder, both found by auditing the whole read/unlock path end to end:

- The backend no longer declines your restore for you. When a read or a send met the restore prompt, the old code cleared it by pressing "Don't restore messages" on Facebook's confirmation — telling Facebook to skip the restore on this device, so the PIN prompt Unlock waits for never came back. It now answers Cancel and simply reports the chat as locked until you unlock it. Declining a restore is your decision, never a side effect of reading your chats.
- Unlock waits for the restore to actually finish. "Verifying your PIN" can run for minutes; the old code called the sealed PIN field a success after a few seconds and parked the browser tab frozen — and a frozen page cannot finish restoring, so the prompt kept coming back. Unlock now waits until the prompt really clears (up to four minutes). If Facebook is still verifying at that point, BlindFB says so — "PIN accepted. Facebook is still restoring your chat history" — and leaves the tab running so the restore completes in the background. Backend v0.1.5-blindfb.15.

## v0.7.3 - 2026-10-31

Three fixes from hands-on testing, verified against a real account this time.

- Unlock works on the restore prompt Facebook actually shows: a single PIN field under the heading "Enter your PIN to restore your chats", with no dialog frame the old code could recognise and no Continue button — typing the full PIN is what submits it, and Facebook then shows "Verifying your PIN" while it restores in the background. Unlock now finds that prompt by its heading, types the PIN, and reads that verifying state as success instead of reporting failure. The secure-storage page the previous release walked is not a restore page at all (it redirects to an unrelated business page) and is no longer used. Backend v0.1.5-blindfb.14.
- Marketplace is a view now: the Views list gains Marketplace, showing the listings feed with price and place on each row. Enter opens a listing in your browser, and the context menu offers Message seller. Selling was already there; browsing was the missing half.
- Search is one dialog: the query box and the filter combo box (Everything, Posts, People, and the rest) sit in a single dialog, replacing the type-then-pick two-step.

## v0.7.2 - 2026-10-31

Three fixes from hands-on testing.

- Unlock reaches the restore on accounts where the chat list never offers it: once the first-run restore prompt is spent, the messages page has no restore entry at all, and Unlock walked that page in vain. Unlock now also goes to Facebook's secure-storage page, where the PIN restore lives permanently, and walks the PIN surface there. Backend v0.1.5-blindfb.13.
- Launch no longer pins the CPU: the first feed page, its Load more, and the background prefetch of every other section each ran as their own tab on a live Facebook page, all at the same moment. Backend reads now run at most two at a time.
- Signing in with Google Chrome works again: Chrome now encrypts its cookies so a cookie-file copy cannot always read them. Picking Google Chrome (or Chromium) in the sign-in dialog now uses the backend's guided sign-in, which copies the session from Chrome itself; the cookie import remains as a fallback, and an empty list now says "no items to show" in the status bar instead of leaving the list silently blank.

## v0.7.1 - 2026-10-31

Two fixes from hands-on testing, both in backend v0.1.5-blindfb.12.

- Verified accounts read properly: a post from a verified page or person read out the badge's whole tooltip ("Accounts with a verified badge have been authenticated and can be Meta Verified subscribers or notable persons or brands."). It now just says "Verified".
- Unlock finds more restore entries: the Messenger history restore walk only recognised a short-labelled button, so on pages where Facebook offers the restore as a link, or as a banner whose label is a whole sentence, Unlock reported the restore dialog missing while the entry was on screen. The walk now presses links and long banners too, and if the entry still cannot be found, the error shows the restore-related text Facebook is actually displaying.

## v0.7.0 - 2026-10-30

Search and Marketplace, end to end.

- Search covers everything Facebook can search: Ctrl+/ (File, Search Facebook) now offers Everything (the default), Posts, People, Friends, Photos, Videos, Pages, Places, Groups, Events and Marketplace. Friends narrows the people results with Facebook's own Friends filter. Every result row knows what it is: Enter opens a post in the comments view, a person or page in their profile, a group in the group window, a photo or video in the reader, and a Marketplace listing in your browser. Results read as "Name — info" instead of reading out an internal reference.
- Join groups from search results: the context menu on a group result has Join group. Public groups join at once; private groups send a request. If the group asks membership questions, BlindFB asks you each question and submits your answers — for a multiple-choice question, type the option's text.
- Leave groups from the Groups list: the context menu has Leave group, with a confirmation. The list refreshes without the group afterwards.
- Sell on Marketplace: File, Sell on Marketplace walks the listing form — title, price (0 means free), condition, category, description — and attaches the photos and videos you pick (up to 10). The details pane confirms with the new listing.
- Message a seller: on a Marketplace search result, the context menu's Message seller sends your message from the listing page.
- Posts take videos now: New post with media accepts photos and videos together, in any mix.
- Fixed: pressing Enter (or React, Comment, Share) on a search result used to report "no link" for every row — search results were looked up in the wrong column. Search results now open and act properly.
- Backend updated to facebook-cli 0.1.5-blindfb.11.

## v0.6.16 - 2026-10-30

A bug-audit release: every module was reviewed and the test suite extended; these are the fixes that came out of it.

- Chat windows: Load more no longer throws away your place. Loading older messages used to rebuild the list and land you on the newest message; now the message you were reading stays selected, and a refresh that changes nothing leaves the list untouched. Refresh and sending still land on the newest message, as they should.
- Chat windows: the Attach files dialog now opens over the chat window. Before, it belonged to the main window and could appear behind the chat, leaving the app looking hung.
- Load more on the chat, friends and group lists no longer drops people: entries whose row has no link were treated as duplicates of each other and silently removed as the list grew.
- Opening a second chat or group window while one is already open now says so in the status bar instead of doing nothing, and Enter on a group member with no profile link says why nothing happened.
- A commenter's profile link can no longer attach itself to the wrong comment in the comments view.
- README: an antivirus note explains the ESET NOD32 false positive (`Win64/HackTool.Agent.XJ`) — BlindFB is unsigned and copies your own browser session with your permission, which trips a heuristic — and how to verify a download with the minisign signature that ships beside every release.

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
