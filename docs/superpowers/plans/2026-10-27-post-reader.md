# Post reader implementation plan

**Goal:** Fulfill the post-reader and Messenger follow-up without forking facebook-cli.

**Architecture:** Keep the single backend worker and result timer. Add a core named-page reader bridge and a separate native dialog module. Track list and reader request identities.

**Tech Stack:** Rust, wxDragon 0.9.24, wx-utils 0.4.4, existing facebook-cli named pages and serde_json.

**Spec:** docs/superpowers/specs/2026-10-27-post-reader-design.md

## Constraints and review focus

No real submissions in agent tests. No top-level fallback for replies. Fail stale/ambiguous targets. Preserve selection and drafts. Scope Enter. Keep PIN on stdin. Do not block the result timer with a reader modal. No new dependency without necessity.

## Core interface

New `blindfb_core::posts` provides `PostDetail { url, author, time, text, comments: Vec<Comment> }` and `Comment { id, author, time, text, parent_id: Option<String>, url: Option<String> }`. All non-optional fields are String. `parse_detail(&str) -> Result<PostDetail, String>` validates the JSON. `load(&Fb, page: &str, target: &str)`, `refresh(&Fb, page: &str)` and `send_comment(&Fb, page: &str, parent: Option<&str>, text: &str)` return `Result<String, String>` containing a fresh detail JSON on success. `close(&Fb, page: &str) -> Result<String, String>` closes the named page. Use an isolated name per dialog and no more than one active reader.

## App interface

Root owns `Ui.post_reader: Rc<RefCell<Option<post_dialog::PostReader>>>`. New `post_dialog::open(&Ui, target: &str)` opens from the item event handler and owns modal lifecycle. `post_dialog::apply(&Ui, session: u64, result: Result<String,String>, sent: bool)` updates the active dialog only if its identity matches.

Root worker tasks: `PostLoad { page: String, target: String }`, `PostRefresh { page: String }`, `PostSend { page: String, parent: Option<String>, text: String }`, `PostClose { page: String }`. Result tags: `PostLoaded(u64)`, `PostSent(u64)`, `Silent`. Child module can use root-private Ui/Job/Task/Tag. Root adds list generation IDs and a retry-aware Messenger unlock tag.

## Tasks

- [x] Core bridge: add failing model/targeting tests; implement typed parsing, page extraction, thread-specific sends and read-only smoke proof.
- [x] Dialog: build native controls and scoped key/context actions; send jobs through the existing worker; preserve draft and selection.
- [x] Main integration: fix notification target routing/global Enter/stale list results; full post rows and Feeds; password PIN recovery with one retry.
- [x] Verify: workspace tests, Clippy, local Facebook reads, native keyboard/NVDA user checks. Independent review not run for v0.5.0.
- [x] Package, commit and release v0.5.0, verifying platform assets and CI.

## Execution

The user specified the workflow and confirmed full-text rows and followed-post Feeds. Implement within that authorized scope. Core bridge, dialog module and main integration have separate owners; coordinate exact interfaces above and do not overwrite another owner's edits.
