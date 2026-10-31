# Chat tail, allowance and desktop disconnect

The latest-message affordance is separate from pausing automatic follow. Touching history stops follow without showing the button; local scroll distance opens it at 120dp, with a 48dp near-tail threshold for hiding. A measured tail gap can correct local distance after layout or message changes. The complete conversation and native virtualization remain intact; scroll gestures do not fetch history.

The message field uses a compact native editor, four visible lines at most, a bounded height and bottom/side gutters. The model label no longer sits inside the empty editor. The existing left-hand model button opens model/reasoning controls and an allowance card. Allowance is absent from the upper-right menu. Details show independent provider-supplied windows, remaining percentages, local reset times, optional extra credits and explicit unavailable/stale states.

The upper-right menu adds **Disconnect desktop**, calling the existing media stop path. It releases input and removes the desktop layout while leaving the chat connection, conversation and remote agent intact.

## Data and local checks

Read-only queries against an existing Codex 0.164.0 owner confirmed ChatGPT account metadata and a rate-limit response. No model prompt was sent. The generated schema and [official protocol](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt) define integer used percentages, optional periods/reset times and optional multi-bucket/credit facts. Account email and identifiers are discarded. Actual allowance values are not included in public evidence.

Quota reads use a separately owned task on the existing JSON-RPC transport; chat does not wait for them. Client drop cancels both quota task and reader. Account epochs and notification revisions reject stale results. Five domain tests cover independent windows/buckets, exact credit strings, absent data, over-limit usage, malformed usage, permission facts, expiry, failure retention and notification/account-switch races. Four JVM tests cover the scroll margin, hysteresis, measured corrections and reset.

Locally, all 91 Rust tests, formatting, Clippy with warnings denied, Release Kotlin/test compilation and Release lint passed. FFI library and bindings were generated together. Existing host, history, identity, trust, pairing and settings formats are unchanged; no old RPC endpoint or FFI fallback was added.

Public packages, native interaction results and live quota/disconnect checks are recorded after publication, with active-video streaming and setup cancellation reported separately.

The first Alpha 20 public package passed signature/alignment checks and an in-place install with 20 byte-identical private files and four profiles before launch. The initial native harness attempted to activate a description-only accessibility node. The final harness uses actionable ancestors or real touch. Alpha 21 also adds an explicit jump action and a growing-message distance lower bound; its regression grows a visible reply while a separate trailing row lies outside the viewport.

## Published Alpha 21 and native acceptance

[Alpha 21](https://github.com/wenshiqi0/Kite/releases/tag/v0.2.0-alpha.21), Android versionCode **23**, was published from **9d2f948b3dc8a8a2c5e2659069eb2d5a4f71b73f**. [Main CI](https://github.com/wenshiqi0/Kite/actions/runs/41225731674) and [tag CI](https://github.com/wenshiqi0/Kite/actions/runs/41225827015) passed. The [publication record](published-release.json) includes exact bytes/checksums, independent stable v1/v2/v3 signing and 16KiB alignment verification, and in-place installation with **22 byte-identical private files and four profiles** before launch. Device identity, SSH trust and pairings were retained.

The exact public APK passed two native presentation tests and one live quota/desktop test on the owned Android 16 / API 36 emulator. [Native measurements](native-validation.json) retain **501 complete display records**, keep the button hidden on touch and a 36dp drag, show it after a larger drag or reply growth, and hide it after jumping back. Native text editing retained a nine-line draft. The empty editor's accessible touch bounds measured **48dp**; its bottom gap measured **60.19dp including the system navigation area**. The actual visual composer has a separate bottom gutter. [Composer capture](composer-empty.png), [independent windows](quota-windows.png) and [explicit unavailable state](quota-unavailable.png) contain only owned synthetic display data.

The [live check](live-validation.json) created an owned empty Codex session and read actual allowance, without submitting a model prompt. The workspace menu had no allowance entry; the left model picker did. Touching **Disconnect desktop** cancelled media setup, retained the same connected chat/composer/session, and a later allowance request completed with a newer update timestamp. This run tested setup cancellation; it did **not** repeat active-video-frame acceptance. Existing media stop/input code is reused.

## Linux deployment and cleanup

The matching **835,438-byte** Linux package has SHA-256 **115e50cb122647fd513cf7eee3de843243a317484d181d35dea8aed7ab5daf8e**. Its running executable matched the package's binary digest. [Deployment evidence](linux-deployment.json) retains the node identity, original Herdr/Sunshine PIDs and 14 private configuration/identity/catalogue/registry files across installation. The status UI was restarted through its stable launcher, opened the same panel, returned to the tray and retained the node. [Installed Linux panel](linux-panel.png).

[Remote cleanup](remote-cleanup.json) closed only three owned workspaces/providers, removed their remote catalogue entries, restored the original authorization bytes and removed the private rate-response probe. [Local cleanup](local-cleanup.json) restored emulator rotation, removed public-key export and test harness, closed the owned SSH forward and stopped the emulator. The installed public APK/data and production Kite Node/status UI/Herdr/Sunshine remain. Original user conversations and phone authorization were not reset. Physical-phone ergonomics remain a separate acceptance boundary.
