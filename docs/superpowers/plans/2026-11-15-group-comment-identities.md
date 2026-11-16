# Group Comment Identities Implementation Plan

> Use superpowers:subagent-driven-development for the independent page adapter and native integration, then review the complete change.

**Goal:** Choose and verify Facebook group comment identities entirely within BlindFB.

**Architecture:** reader.js supplies its current root/editor/read callbacks to identities.js. The adapter returns current identity metadata and token-scoped menu/form panels. Rust validates these panels and the native reader renders them with existing native controls. Sending carries the last displayed identity for verification before Enter.

**Tech stack:** existing Rust, wxDragon and dependency-free browser JavaScript.

**Spec:** ../specs/2026-11-15-group-comment-identities-design.md

## Global constraints

- Preserve existing edits, comments and error handling; no new dependencies.
- Native labeled controls, predictable focus, keyboard operation and spoken status.
- Read/prepare may recover lost pages; identity actions and submission must not retry mutations.
- Do not publish or react on live Facebook during testing.

## Shared interfaces

- PostDetail.comment_identity: optional {label: string, can_change: boolean, reason: string}.
- Identity panel JSON: {identity: {token: string, text: string, options: [{key: string, label: string, selected: boolean, enabled: boolean}], fields: [{kind: text|choice|check, prompt: string, choices: string[]}], answers: JSON[], error: string}}.
- window.__blindfbPost.identityOpen(parent) returns panel JSON; identityAct(token, choice, answers) returns the next panel or fresh post detail; identityCancel(token) closes only its own UI and returns fresh detail.
- prepare(parent, text, expectedIdentity) verifies an optional expected identity label and rechecks before Enter.
- Initialization injects identities.js before reader.js. reader.js creates the adapter with its current/composer/read callbacks.

## Tasks

1. Page adapter: capture current controls; add failing DOM fixtures; implement scoped discovery, menu/setup navigation, stale-token checks, identity metadata and submit guards. Own identities.js, identities.test.js, reader.js and reader.test.js.
2. Native integration: add typed panel/metadata parsing and bridge methods; native identity action and form dialog; wire jobs/results and submission identity. Add parser tests and update fixtures. Own Rust/app/i18n/documentation files.
3. Verify: focused JS and Rust tests, workspace build/Clippy, independent review and regression fixes. Inspect live controls without submitting if an authenticated session is available.

## Review focus

- Unknown languages and hidden duplicate controls must not select unrelated buttons.
- Per-post locks, disappearing choices and replaced dialogs must preserve the existing identity and draft.
- Reply editor changes and identity drift must stop sending.
- Nickname setup validation stays visible; cancel never submits a comment.
- Non-group readers and existing participation questions retain existing behavior.
