# Post reading, navigation, and Messenger recovery

## Intent and user choices

Sign-in is verified in the real GUI and released as v0.4.0. The user now wants reliable section navigation, a native post reader, comment replies from the Applications menu, a chronological Feeds section, full post text in NVDA list rows, and recovery from Messenger's history-restoration prompt.

The user explicitly chose author, full text, then time in each feed row. Chronological Feeds means all followed friends, pages and groups, alongside the existing News feed. Preserve existing Ctrl+1 through Ctrl+6 shortcuts; add Feeds on Ctrl+7.

## Approaches

Extending BlindFB through the backend's existing named-page browser interface is the selected engineering approach. Maintaining a fork of facebook-cli would add an updater and distribution problem; redirecting every reply to the website would not fulfill the requested native workflow. Keep existing backend commands for ordinary reads and post actions wherever they suffice.

## Findings

Notification TSV places time first and the post reference fourth. BlindFB currently uses the first field as the post reference, explaining the reported unknown post '1h'. The Enter accelerator is global, so it can activate a stale item while the user switches sections. Async list results also need request identities so obsolete loads cannot replace the current section.

facebook-cli v0.1.5 supports full feed text with --full. Its view output clips comments and contains no comment IDs. Its comment command supports top-level comments only. True replies require a named browser page, comment identity, a scoped Reply action and verified submission. The built-in posts command can load a Facebook Feeds URL. A read-only local probe verified https://www.facebook.com/?filter=all&sk=h_chr displays the Feeds heading.

## Native reader

Open a resizable modal dialog directly from item activation, before its asynchronous load. The existing frame results timer must remain able to run; never open the long-lived reader from inside that timer callback. Include a labelled readonly full-post field, native comment list, labelled multiline comment/reply composer, Send, Refresh, Cancel reply, and Close controls. Enter in the composer inserts a newline; Ctrl+Enter sends. Escape closes the dialog and restores the selected main-list item.

Applications and Shift+F10 open the selected comment's native context menu. Reply selects that exact comment and focuses the composer with an accessible Reply to author label. Provide Copy comment and Open comment on Facebook when identity is available. Keep full comment text. Update rows in place and preserve selection; do not reset lists to the first row on refresh. Give native controls clear accessible names and ordinary tab order. Do not announce reaction-count shorthand in feed rows.

## Backend bridge and safety

Use one named page per reader, closed on dialog exit. Read the full post and loaded comments into typed data. Prefer actual comment permalinks/IDs; local DOM identities must be checked for attachment and unchanged identity before acting. Fail closed if a target is missing or ambiguous. A reply must never fall back to a top-level comment. Use fresh backend page references for editing, check the selected composer before Enter, and verify a new comment in the correct thread. Do not retry an uncertain submission automatically. Preserve the draft on failure. No live comment/message submissions during agent testing.

Resolve notification references from the proper field; structured notification reads may preserve canonical URLs needed by the reader. Reject timestamps and other non-post tokens locally. Scope Enter to the items list, clear or disable stale selection during navigation, and ignore obsolete list completions.

## Messenger

Recognize the explicit history-prompt-blocked error. Reuse the existing password-style Messenger PIN dialog, pass the PIN only through stdin, and retry the exact failed request once after successful unlock. Preserve unread filters. Cancellation and failed unlock must not create an automatic prompt loop. Never store or log the PIN.

## Acceptance

Regression tests cover notification references, invalid post identifiers, full text, typed reader parsing, exact-thread targeting, stale/ambiguous targets, chronological feed arguments, and Messenger error classification/retry. Run workspace tests, Clippy and local packaging. Perform read-only Facebook smoke checks and native keyboard/NVDA validation with the user. Commit and release the finished follow-up as v0.5.0; do not alter the completed v0.4.0 tag.
