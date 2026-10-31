# Chat tail, allowance and desktop disconnect

The latest-message affordance is separate from pausing automatic follow. Touching history stops follow without showing the button; local scroll distance opens it at 120dp, with a 48dp near-tail threshold for hiding. A measured tail gap can correct local distance after layout or message changes. The complete conversation and native virtualization remain intact; scroll gestures do not fetch history.

The message field uses a compact native editor, four visible lines at most, a bounded height and bottom/side gutters. The model label no longer sits inside the empty editor. The existing left-hand model button opens model/reasoning controls and an allowance card. Allowance is absent from the upper-right menu. Details show independent provider-supplied windows, remaining percentages, local reset times, optional extra credits and explicit unavailable/stale states.

The upper-right menu adds **Disconnect desktop**, calling the existing media stop path. It releases input and removes the desktop layout while leaving the chat connection, conversation and remote agent intact.

## Data and local checks

Read-only queries against an existing Codex 0.164.0 owner confirmed ChatGPT account metadata and a rate-limit response. No model prompt was sent. The generated schema and [official protocol](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt) define integer used percentages, optional periods/reset times and optional multi-bucket/credit facts. Account email and identifiers are discarded. Actual allowance values are not included in public evidence.

Quota reads use a separately owned task on the existing JSON-RPC transport; chat does not wait for them. Client drop cancels both quota task and reader. Account epochs and notification revisions reject stale results. Five domain tests cover independent windows/buckets, exact credit strings, absent data, over-limit usage, malformed usage, permission facts, expiry, failure retention and notification/account-switch races. Four JVM tests cover the scroll margin, hysteresis, measured corrections and reset.

Locally, all 91 Rust tests, formatting, Clippy with warnings denied, Release Kotlin/test compilation and Release lint passed. FFI library and bindings were generated together. Existing host, history, identity, trust, pairing and settings formats are unchanged; no old RPC endpoint or FFI fallback was added.

Public packages, native interaction results and live quota/disconnect checks are recorded after publication, with active-video streaming and setup cancellation reported separately.
