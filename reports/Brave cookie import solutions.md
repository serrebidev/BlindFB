# Test Stable Before Replacing Cookie Extraction

**Local follow-up:** Brave Stable 156.1.98.60 was installed and the other channels removed at the user's request. BlindFB successfully imported its Facebook cookies, read the authenticated Facebook profile, closed and restarted its dedicated browser, and read the profile again. This verifies the Stable route on this machine; Nightly import remains unsupported by the tested library configuration.

**Test the user's chosen Brave Stable setup with BlindFB's existing importer before replacing its extraction library.** The reviewed source supports that experiment: rookie-cookies discovers Nightly profiles but chooses a single Stable Brave COM identity when decrypting app-bound cookies. The reported Nightly failure—nine Facebook `v20` cookies failing with `REGDB_E_CLASSNOTREG`—fits that mismatch, although source review alone cannot prove the installed service registration caused it. No repository in the requested searches established a working Windows Brave Nightly → BlindFB Facebook-session transfer. A browser-mediated, explicitly triggered Facebook-cookie export is the strongest alternative if automatic import remains blocked. A fresh Facebook login in BlindFB's dedicated browser profile is the simplest operational fallback. **Stable import success remains untested in this report.**

## Forty search positions represent 38 repositories

The exact GitHub repository search was **`brave cookies`**, with default repository search fields and no extra qualifiers. The first pass requested `sort=updated&order=desc&per_page=20`; the second requested `sort=stars&order=desc&per_page=20`. Both snapshots returned **101 total results**, `incomplete_results=false`; neither required a broader query. These links reproduce the requests, but their live results can change. ([Recent API search](https://api.github.com/search/repositories?q=brave%20cookies&sort=updated&order=desc&per_page=20), [stars API search](https://api.github.com/search/repositories?q=brave%20cookies&sort=stars&order=desc&per_page=20))

The preserved `recent.json` and `stars.json` in `research_notes/Brave cookie import solutions` are the ranking evidence. The stars snapshot records retrieval at **2026-10-28 04:53:45 UTC**, equivalent to October 27 at 21:53:45 in Vancouver. The recent snapshot lacks a retrieval-time field; an exact time should not be invented. Its recorded update dates extend into October 28 UTC. API order is retained even where update timestamps are non-monotonic: “recent” means GitHub's returned search order, not independently sorted latest source commits.

There are **38 unique repositories across 40 positions**. `teng-lin/rookie-cookies` appears eighth in both lists; `ciacou001/Browser-Data-Grabber` appears tenth by recent and thirteenth by stars. Every position was assessed, including irrelevant, empty, binary-only and credential-stealing results. Researchers retrieved README files and selected implementation/docs through GitHub's API, preserving source snapshots and commit-pinned links. Missing implementation is explicitly recorded below. None of the researched code or binaries was executed, no researched dependency was installed, and research did not access personal cookies. Cookie values and keys were not logged.

## Nightly discovery does not establish Nightly decryption

The task's local diagnostic evidence reports rookie-cookies 0.6.0 finding nine Facebook `v20` cookies, all ending in `ProviderFailed`, with `CoCreateInstance` returning `0x80040154` (`REGDB_E_CLASSNOTREG`). The reviewed upstream source already supports app-bound encryption and tries **IElevator2 before the older interface**. Calling this simply “no v20 support” would miss the stronger evidence. Its COM lookup compares executable basenames and assigns every `brave.exe` one CLSID, `{576B31AF-6369-4B6B-8560-E4B203A97A8B}`; there are no separate Brave Nightly/Beta entries. Supplying a Nightly executable alone therefore does not select a Nightly class. ([COM table](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/windows/appbound/native/com_iid.c), [extractor](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/windows/appbound/native/abe_extractor.c))

The library's registry includes Nightly data directories, but host inference reduces channel-specific paths to vendor `brave`, and executable fallback paths target Stable installations. **Profile discovery and cryptographic host selection are separate capabilities.** That architecture supports a Stable/channel-registration mismatch hypothesis; missing service registration or installation differences remain alternative explanations until installed registrations are checked. This review compared current upstream source, not byte-for-byte equivalence with the installed 0.6.0 build. ([Profile registry](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/browser_registry.json), [host inference](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/browser/appbound_host.rs), [executable resolution](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/windows/appbound/browser_path.rs))

The default policy is **`InjectionOnly`**. An explicit `AllowElevatedFallback` policy attempts injection and then a separate SYSTEM/user DPAPI route with scheme-dependent key derivation. That fallback does not use the same browser COM selector, but its success with this Nightly installation was not tested. Selecting it does not itself grant privileges or trigger UAC; the implementation performs privileged token impersonation. This is an untested alternative, not a reason to run the GUI elevated or disable protections. ([Policy](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/execution.rs), [fallback](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/windows/appbound/mod.rs), [privilege handling](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/windows/appbound/impersonate.rs))

Switching to Stable aligns the browser channel with these paths and COM assumptions, making it a sensible minimal experiment. It does **not** establish that Nightly's existing session becomes usable in Stable. A meaningful success check requires Facebook authentication in the selected Stable source profile, import into BlindFB's selected dedicated context, and an authenticated Facebook page afterward. Browser installation alone proves none of those steps. If Stable still fails, preserve redacted diagnostics and check installed COM registration before changing libraries.

## Browser-owned exports offer the strongest alternative

For retaining an existing Nightly session, the best design reference is **LopRixx's Session Transfer**: an explicitly triggered extension reads and sets cookies through the browser's own APIs, avoiding external ABE decryption. It offers encrypted transfer files and domain selection. Its actual limits matter: author testing covers Opera GX → Brave, partitioned cookies are skipped, session cookies are excluded by default, and the UI is French. Nightly → BlindFB and NVDA usability remain unverified. ([Implementation](https://github.com/LopRixx/opera-gx-to-brave-session-transfer/blob/7c527e192ceb6668b8102c84f935bef2d5b25229/extension/lib/cookies-core.js), [documented limits](https://github.com/LopRixx/opera-gx-to-brave-session-transfer/blob/7c527e192ceb6668b8102c84f935bef2d5b25229/docs/04-LIMITATIONS.md))

Cookie Manager is a smaller JSON-export reference; CookieLens adds per-site permission options but also persistent local backups. Both inspected JSON formats omit partition metadata. Any implementation should export only `facebook.com` and its subdomains, preserve host-only/domain, path, expiry/session, Secure, HttpOnly and SameSite semantics, and explicitly handle unsupported partitioned records. Source cookie-store IDs must not become destination profile selectors. Export files contain credentials and must stay out of logs. Browser extension access requires cookies/host permissions and runs in the selected source profile. ([Cookie Manager](https://github.com/nullstacks/cookie-manager/blob/f8d19f44f33e365fa15e9d0582e0cb70da8920d5/popup.js), [CookieLens format](https://github.com/annayanami19/cookielens/blob/9bf541ddb332fe56f4e7d52c79035abcde471251/lib/formats/json-format.js), [Chrome cookies API](https://developer.chrome.com/docs/extensions/reference/api/cookies))

Source-browser CDP and copied-profile experiments remain unproven. Chrome documents restrictions on debugging its default data directory and different keys for nonstandard directories; this does not establish identical Brave behavior or make a copied Nightly profile decryptable. None of these results verified that workaround. Facebook can also reject a technically faithful transferred session. Direct login in the persistent dedicated profile avoids transfer dependencies and provides a practical fallback. ([Chrome debugging changes](https://developer.chrome.com/blog/remote-debugging-port))

## The 20 most-recent results contain four useful candidates

Each paragraph preserves its API rank. Dates are snapshot `updated_at` dates, not necessarily code-push dates; links pin reviewed evidence.

**1. helldragon-44/Brave — 0 stars; October 28, 2026.** Download landing page directing users elsewhere; no cookie importer in its four-file tree. Unrelated to official Brave. ([README](https://github.com/helldragon-44/Brave/blob/456f8dfeac80448d785e847ad33bee701efdb6a7/README.md))

**2. NivaucchuRabuessa/Brave-Search-Rerank — 0 stars; October 28.** Search reranking userscript, not session migration. ([Source](https://github.com/NivaucchuRabuessa/Brave-Search-Rerank/blob/2b8a855cf9c1f609198440282487223c8e7f8c62/brave-rerank-quicktune.user.js))

**3. smastrom/cookie-banner-killer-firefox — 0 stars; October 27.** Firefox consent/banner filtering; wrong browser and feature. ([Source](https://github.com/smastrom/cookie-banner-killer-firefox/blob/c72752b9d893537beb27ea3ce86cdf34bcd0d5c0/src/background.js))

**4. rviach/chrome-offline-to-brave — 0 stars; October 27.** Offline Windows-to-Linux research and CDP importer; no demonstrated Nightly extraction. Its `browserContextId` placement needs protocol verification. ([Source](https://github.com/rviach/chrome-offline-to-brave/blob/e4e39dffea8c87b37c77a9e0353afbb9dee04583/scripts/import_cookies.py))

**5. krypten/browser-dig — 0 stars; October 25.** Forensic parser exposes encrypted fields without the needed decryption/import route. ([Source](https://github.com/krypten/browser-dig/blob/ac8ecb9bebff9076b019db1fb2452e9ca9e2f55a/browserdig/browsers/common/chromium.py))

**6. LopRixx/opera-gx-to-brave-session-transfer — 0 stars; October 25.** Best consent-based transfer reference, MIT; limitations described above prevent calling it verified Nightly support. ([Source](https://github.com/LopRixx/opera-gx-to-brave-session-transfer/blob/7c527e192ceb6668b8102c84f935bef2d5b25229/extension/lib/cookies-core.js))

**7. anaskhan0009/cookie-tracker-guide — 0 stars; October 21.** Educational guide; its export handles task records, not authenticated cookies. ([Source](https://github.com/anaskhan0009/cookie-tracker-guide/blob/bcdb842aa8fa47b467b680ff5ec2a070eb2843ea/index.html))

**8. teng-lin/rookie-cookies — 10 stars; October 22.** Best existing library candidate, MIT; v20/IElevator2 support with the channel-selection gap above. ([Source](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/windows/appbound/native/com_iid.c))

**9. toejaiden1/idm-cookie-importer-2026 — 0 stars; October 12.** ZIP promotion with no inspectable extractor implementation; compatibility claims unverifiable. Binary not downloaded. ([README](https://github.com/toejaiden1/idm-cookie-importer-2026/blob/8fd7b9dbe8f52b6d8e8d3c58c2cb3285d8a01046/README.md))

**10. ciacou001/Browser-Data-Grabber — 4 stars; October 12.** Credential-stealer builder with exfiltration/stealth architecture; unsuitable despite benign marketing. ([Architecture](https://github.com/ciacou001/Browser-Data-Grabber/blob/9f7a33e50d45bca1e36302926df3042d6fb6927e/ARCHITECTURE.md))

**11. jempresaz87-crypto/veil-browser — 0 stars; October 11.** Incomplete binary bundle, README absent; launcher references missing executable and provides no migration logic. ([Launcher](https://github.com/jempresaz87-crypto/veil-browser/blob/1ba491615e94d27370cefbf8ce11eb387f99673b/Iniciar-Veil.bat))

**12. nullstacks/cookie-manager — 2 stars; October 27.** Small MIT JSON bridge reference; broad host permissions and omitted partition metadata require attention. ([Source](https://github.com/nullstacks/cookie-manager/blob/f8d19f44f33e365fa15e9d0582e0cb70da8920d5/popup.js))

**13. MJVasya/consent-buttons — 0 stars; October 5.** Automates consent buttons; no session export. ([Source](https://github.com/MJVasya/consent-buttons/blob/ef4e7b06766b075e9c4afbe53ef4800f866665cd/content.js))

**14. annayanami19/cookielens — 0 stars; September 19.** MIT cookie editor/exporter; useful alternative reference, with backup persistence and partition limitations. ([Source](https://github.com/annayanami19/cookielens/blob/9bf541ddb332fe56f4e7d52c79035abcde471251/background/service-worker.js))

**15. techiepingu/brave-browser-mcp — 1 star; September 14.** Extension exposes cookie get/set through MCP; excessive integration and PolyForm Noncommercial licensing. ([Source](https://github.com/techiepingu/brave-browser-mcp/blob/27545e76b3eb1a0d16e08b0d4c1078fa5bf56fae/extension/background.js))

**16. yesacarlosm/history-guard — 0 stars; September 9.** Deletes history while preserving cookies; no transfer implementation. ([Source](https://github.com/yesacarlosm/history-guard/blob/212c83c51d47d4aa6e57d38b295e69b566f2a3cb/background.js))

**17. Aybraheem/C00ki3-M0nst3r — 1 star; October 24.** browser_cookie3 wrapper prints selected cookie names; incomplete Facebook jar and no independent ABE fix. ([Source](https://github.com/Aybraheem/C00ki3-M0nst3r/blob/bfa8ecc04de879772062fd57e3d13c751be49c14/CookieGrabber/script.py))

**18. hollyshitss/2026-stealer-takinti.xyz — 0 stars; September 1.** Explicit credential-stealer promotion; README only, no implementation to verify. ([README](https://github.com/hollyshitss/2026-stealer-takinti.xyz/blob/38bae0858ba65565846ec439caad2eeab3509b57/README.md))

**19. zaidrao/chrome_backup — 0 stars; August 31.** Whole-profile restore and separate ABE extraction requirements; excessive scope, with unacceptable antivirus-disabling instructions. ([README](https://github.com/zaidrao/chrome_backup/blob/666a757f7477a0bc214cc80c4a0194d54e4c9b77/README.md))

**20. eventh0riz0n/site-nuke — 0 stars; August 29.** Deletes site cookies/storage; opposite of session preservation. ([Source](https://github.com/eventh0riz0n/site-nuke/blob/061af7fb619ed123bc7011e0c261bc91e42615b1/extension/service_worker.js))

## The 20 most-starred results do not supply a verified fix

Stars reflect the preserved snapshot, not endorsement. The leader's 1,828 stars do not establish safe integration or Nightly compatibility.

**1. xaitax/Chrome-App-Bound-Encryption-Decryption — 1,828 stars.** Current IElevator2 research, but explicitly post-exploitation with evasion and broad secrets extraction; Stable-only Brave identity. Reference only. ([Configuration](https://github.com/xaitax/Chrome-App-Bound-Encryption-Decryption/blob/63b719f2ab3dbf8573fd9228a9c028526e9eb1db/src/payload/browser_config.hpp))

**2. ksdodk/ChromiumStealer — 65 stars.** Archived credential stealer sending secrets to Telegram/Discord; no verified Nightly support. ([Source](https://github.com/ksdodk/ChromiumStealer/blob/d636025408360306bb1274ccfb26e965808bba46/main.py))

**3. Filza2/DBreaker — 32 stars.** Legacy broad-data extractor; inspected DPAPI code lacks APPB/IElevator2 handling. ([Source](https://github.com/Filza2/DBreaker/blob/ff7d40aa6abbb79197addf21ec29885f3f4d7b30/DBreaker.py))

**4. sh4den/Hawkish-Eyes-NoDualHook — 30 stars.** Credential-stealing fork removes a duplicate destination while retaining secret collection/webhook behavior. ([Source](https://github.com/sh4den/Hawkish-Eyes-NoDualHook/blob/e462df515c6e9526cf0c3b0b81e0bdc53cf99ab7/main.py))

**5. phoqe/havelock — 27 stars.** Windows decryption explicitly unsupported; DPAPI helper is unfinished. ([Source](https://github.com/phoqe/havelock/blob/fe2ebbcaccb822e4d858510d13ba84c42f7395e8/crypto/win32.js))

**6. 00nx/Chrome-App-Bound-Encryption-Bypass — 25 stars.** Evasion-oriented fork; Stable Brave IDs, no newer IElevator2 implementation in inspected source. ([Source](https://github.com/00nx/Chrome-App-Bound-Encryption-Bypass/blob/d78039ebddfe9086078b10f985cfc34de53c95e5/src/chrome_decrypt.cpp))

**7. Majanito/ABE-Decryption — 21 stars.** Smaller IElevator2 research example; Stable paths, printed master key and hard-coded Netscape attributes make it unsuitable for faithful automatic import. ([Source](https://github.com/Majanito/ABE-Decryption/blob/e409a6e612d6f96dbf667b77eead19585ce8ffb5/src/injector_template.py))

**8. teng-lin/rookie-cookies — 10 stars.** Overlap with recent rank 8; maintained library shape fits BlindFB better than offensive tools, but Nightly remains unverified. ([Source](https://github.com/teng-lin/rookie-cookies/blob/21ce5462955860a019b03507f9813407e2fd615c/rookie-rs/src/windows/appbound/native/com_iid.c))

**9. marichu-kt/PrivScore — 8 stars.** Legitimate privacy analysis; summarizes cookie names/security flags rather than exporting session values. ([Source](https://github.com/marichu-kt/PrivScore/blob/ab732915a590a7646c611952294b6c9a830e55dd/extension/src/background/cookies_summary.js))

**10. JustWatch31/KRY-Stealer — 7 stars.** ABE credential stealer with webhook support and Stable paths; unsuitable dependency. ([Source](https://github.com/JustWatch31/KRY-Stealer/blob/bbf074e8068a07128800575205ba7566e54d4667/main.py))

**11. zeusvuln/echoleak — 5 stars.** Broad secrets/injection tool; basename COM selection, Stable paths and no inspected Nightly selector. ([Source](https://github.com/zeusvuln/echoleak/blob/6a089f001a315148c05feba8b9f44d45341848dc/dll/src/lib.rs))

**12. rr-uchchash360/No-Cookie-YouTube — 5 stars.** Privacy-enhanced YouTube playback; export/import concerns settings, not Facebook sessions. ([README](https://github.com/rr-uchchash360/No-Cookie-YouTube/blob/a33ad5e4e46b67834d259cdd33a13c3866fcebaf/README.md))

**13. ciacou001/Browser-Data-Grabber — 4 stars.** Overlap with recent rank 10; generator implements exfiltration destinations and secret capture. ([Source](https://github.com/ciacou001/Browser-Data-Grabber/blob/9f7a33e50d45bca1e36302926df3042d6fb6927e/src/generator/stubgenerator.cpp))

**14. Malicord-Analysis/Malicord-Analysis — 3 stars.** Credential stealing, Discord modification and upload machinery; unsuitable. ([Source](https://github.com/Malicord-Analysis/Malicord-Analysis/blob/3a7371c811b9c32a740eb9726a83cf03c240ef2e/main.py))

**15. DomenicoCentrone/cookie-monster — 3 stars.** Legitimate deletion/privacy extension; its portable export excludes saved cookie values. Do not mistake settings backup for session export. ([Transfer source](https://github.com/DomenicoCentrone/cookie-monster/blob/0603832fe0779b7d684a366098341017bafc5000/src/core/transfer.ts))

**16. CRB-CyberSec-Dev/SessionHijack — 3 stars.** Client/server session-hijacking demo, legacy decryption and June 2024 compatibility claims; wrong integration/security model. ([Source](https://github.com/CRB-CyberSec-Dev/SessionHijack/blob/63c525437f824193997afac9847d61276a5927c6/SessionHijackW/SessionHijackW.cpp))

**17. 0xGval/openclaw-headless-setup — 3 stars.** Ubuntu documentation with X/Twitter CDP injection; no Windows Nightly extraction implementation. Its sandbox-disabling configuration is unsuitable to copy. ([Documentation](https://github.com/0xGval/openclaw-headless-setup/blob/7ab81c8cc6267871d3560629651ea8dfb4554108/openclaw-minisforum-security-setup.md))

**18. N0namii/Husein-Cookie-Stealer — 3 stars.** Explicit browser-secret collection and Telegram ZIP delivery; unsuitable. ([Source](https://github.com/N0namii/Husein-Cookie-Stealer/blob/073b3ab29c60e5f80c268688f4ac6b43d1879d88/Husein.py))

**19. RaniGaytri/Stop-Google-location — 3 stars.** Empty repository describing a VPN/location question; README returned 404, tree returned 409. No source exists to inspect. ([Tree API](https://api.github.com/repos/RaniGaytri/Stop-Google-location/git/trees/main))

**20. logly-uk/logly-wordpress — 2 stars.** Cookieless WordPress analytics and Brave compatibility; unrelated to browser-session migration. ([Source](https://github.com/logly-uk/logly-wordpress/blob/6f31b68e126e85896ce4cd97d3f4d3e876a17452/loglyuk-privacy-analytics/loglyuk-privacy-analytics.php))

The next decision should follow a redacted end-to-end Stable test, not repository popularity. If automatic extraction works there, channel compatibility can remain a documented Nightly limitation. If it fails, distinguish key decryption, cookie transport and Facebook acceptance before expanding the implementation. This research did not test Stable import, elevated fallback, copied-profile CDP, extension accessibility or Facebook session acceptance; it establishes which routes deserve testing and which repositories to exclude.
