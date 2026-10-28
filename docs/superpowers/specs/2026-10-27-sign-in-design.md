# Sign-in repair

The installed backend fails before sign-in because it only launches stable Chrome/Chromium. This machine has Brave Nightly and Edge. The user chose using Brave as the engine instead of installing Chrome.

The user subsequently requested a review of 20 recently updated and 20 most-starred Brave cookie repositories, then chose to replace Beta/Nightly with Brave Stable. The research covers 38 unique repositories and is recorded in `reports/Brave cookie import solutions.md`. The installed Stable version is 156.1.98.60. A real local test imported its Facebook session, read the authenticated profile, restarted the dedicated browser, and read the profile again successfully.

## Approaches

- Install Chrome and repair the prerequisite check: smaller, but requires another browser. Not selected.
- Launch an installed Chromium browser in a dedicated BlindFB profile and connect the backend using its supported FB_CDP interface: selected by the user.

## Design

Keep the backend's native Chrome path when available. Otherwise prefer an installed default Chromium browser, then another installed Chromium browser, including channel builds. Launch it with a separate BlindFB profile and a localhost-only ephemeral debugging port. Never use the everyday browser profile as the backend engine.

Import from the exact browser selected by the user. For direct sign-in, reopen the dedicated engine visibly and wait for verified signed-in status. Respect an explicit FB_CDP setting. Stop the daemon when changing its endpoint; quit closes only BlindFB's dedicated engine. Preserve backend errors and do not load the feed following failed or cancelled setup.

Use existing browser detection, CDP support, native dialogs and dependencies. No account credentials or cookie values in logs. No unrelated Facebook features.

Windows cookie import uses the user-suggested rookie-cookies 0.6.0 library against the selected browser's exact profile root, restricted to Facebook domains. Its SQLite dependency requires updating rusqlite to 0.40.2. Use the library's default unprivileged policy, never its elevated SYSTEM fallback. Closed-browser extraction can be necessary for locked cookie databases. Nightly app-bound import failed locally because its COM service was unavailable through the library's stable-Brave lookup. Keep direct sign-in as the fallback. Validate both authentication cookies, expiry and exact Facebook domains before importing. Clear the dedicated browser's previous cookies and daemon caches, then verify a real Facebook profile read.

## Acceptance

Regression tests cover engine selection, profile separation, command configuration and authentication errors. Test this machine with Brave Nightly import and a read-only Facebook command. Run workspace tests and Clippy, package Windows locally, commit and publish the release, and verify its assets.
