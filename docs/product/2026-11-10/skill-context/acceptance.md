# Shared transcript rendering and Skill instruction cards

Skill instructions use a separate, initially folded card with the skill name. Expanding it shows its source path and Markdown body. Rust recognizes complete `<skill>` envelopes, including multiple skills and CRLF, and supplies read-only `SkillContext` blocks to native clients. Source records and caches retain the original text. Ordinary `$name` prompts, quoted examples, malformed envelopes and appended requests remain text; mixed provider blocks retain adjacent user text and attachments in order.

Android main chat and the related-task inspector both call `ChatEntryContent` for the complete row. The shared renderer owns message alignment and bubbles, compact global-instruction notices, Skill cards, tool groups and errors. Read-only history supplies its own attachment callback and disabled desktop capability. macOS also handles the new Skill block, but its native build and UI have not been exercised on this Linux host.

Validation:

- **179 Rust tests** pass across agent, app and FFI; one unrelated opt-in real-report test is skipped. Formatting and strict all-target Clippy pass.
- **85 Release JVM tests**, Release lint, Kotlin instrumentation compilation and the matching Rust/UniFFI debug build pass.
- **18 unique native Android API 36 x86_64 checks** pass on an isolated, network-disabled emulator with synthetic data: four Skill cases, nine related-task cases and five shared-content/inspection cases. The three workspace-mirror cases run with `qa_native_history=true`. These cover folding, independent expansion, Markdown/source display, identical global-instruction handling in the shared main row and actual inspector, ordinary text, mixed blocks, tools, errors, refresh, scroll anchoring and sheet settling. [Structured results](native-validation.json).
- Native screenshots: [shared main row](main-shared-message-rendering.png), [actual read-only inspector](subagent-shared-message-rendering.png), [multiple Skill cards](skill-context-expanded.png).

The first emulator attempt was obscured by an Android System UI ANR dialog during its initial startup. The unchanged APK passed after that system dialog was dismissed. No model turn or real host connection was used. Physical-handset installation and rendering remain untested.

Version: **0.2.0-alpha.91**, Android versionCode **93**, based on published Alpha 90. The host protocol is unchanged; this update only requires the Android client.
