# Group posting implementation plan

**Goal:** Publish a new post to one joined group selected from Audience.
**Architecture:** Extend the shared backend composer with a validated group URL.
Use a native app picker with asynchronous joined-group loading and pagination.
**Tech stack:** Rust, wxDragon, existing JavaScript browser helpers.
**Spec:** `docs/superpowers/specs/2026-11-15-group-posting-design.md`.

## Tasks

- [x] Backend: investigate real group composer without publishing; add failing
  destination/receipt/DOM tests, implement `post --group`, retain profile behavior,
  test and review, open upstream PR, integrate into the fork and release.
- [x] Core/app: test group command arguments and group-specific outcome messages;
  implement Groups audience and a native asynchronous group picker with type-ahead,
  retry/cancel, and Load more. Never send when selection or loading fails.
- [x] Translate new strings; update README and changelog; pin released backend.
- [x] Run workspace tests, JavaScript fixtures, and clippy. Review full changes,
  commit, fetch/check main, integrate, push main then tag separately.
- [x] Verify published backend and signed app assets plus updater discovery.

## Added task: group creation

- [x] Inspect Facebook's live group-creation form without submitting it; determine
  the exact name/privacy/visibility fields and successful-creation receipt.
- [x] Add tested backend `group-create` command with explicit settings, account and
  cancellation guards, dry-run safety, verified success, and uncertain-error advice.
  Open upstream PR and integrate/release after backend .69.
- [x] Add File > New group, labeled native form, validation, modal registration,
  explicit Create/Cancel controls, background submission, and groups-list refresh.
- [x] Translate creation messages, update docs/changelog, and pin the final backend.
- [x] Repeat only affected checks, run final combined gate/review, then commit,
  release, and verify both features together.

## Added task: group invitations

- [x] Inspect eligible-person picker and invitation confirmation without sending.
- [x] Add backend invitation protocol with stable identities, permission handling,
  selection validation, cancel/dry safety, and verified outcomes; test/review and
  open upstream PR. Combine with group creation in the next backend release.
- [x] Add Invite people to the group window and native recipient selection with
  explicit Send invitations, cancellation, search/loading as needed, and status.
- [x] Translate invitations, update docs/changelog, run combined tests/review, then
  commit and release the original posting plus creation and invitations together.

## Review focus

- More than 100 groups remain reachable through Load more.
- Duplicate names select distinct canonical group URLs.
- Cancel/error/empty groups never fall back to profile posting.
- Group and profile audiences cannot be combined.
- Pending moderation and uncertain publication direct users to the group.
- Dialog focus, Enter, Escape, retry, and late asynchronous results remain safe.

## Decisions

The user requested implementation, commit, and release together. Proceed within
that authorization; the one-group preference was explicitly confirmed. Existing
unrelated backend workflow edits belong to another session and must be preserved.
