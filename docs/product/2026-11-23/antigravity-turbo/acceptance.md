# Antigravity session Turbo

Kite exposes `tool_permission` through the existing session permission controls. `default` uses host permissions; `turbo` starts the native ACP process with `--dangerously-skip-permissions`. This is independent of Antigravity's Default, Accept edits, Auto and Plan execution modes.

Changing the selection applies to the next submitted message. The adapter stops the previous native process, resumes the same native session ID and restores the execution mode before prompting. The requested model and effort continue through the existing turn input. The policy is stored with the native recovery metadata; global settings and other conversations are unchanged. A queued Turbo message does not approve an already pending request. Missing persisted native context fails before a model prompt or a new conversation can be created.

## Verification

- `bun run check`: TypeScript, Biome, package boundaries and 139 tests passed; one Darwin-only test was skipped on Linux. Biome reported one existing informational suggestion in the unchanged `packages/host/test/acp.test.ts`.
- The host integration regression covers invalid policy rejection, Turbo on the first message, default permission requests, queued policy isolation, tool results, unchanged owner/native session identity, old-process exit, cold recovery and refusal to replace missing context. Existing Codex, Ply and DSH tests passed in the same run.
- An isolated protocol check against installed Google Antigravity CLI **1.5.14** created an empty native session, selected Turbo and Auto, then returned to default tool permissions. Both prior processes exited, the native ID stayed unchanged and Auto remained selected. **No model turn or tool execution was started in this real-CLI check.** The tool-execution assertions above use an isolated ACP fixture.
- The settings use the existing generic Core/FFI contract. No native client sources or bindings changed; no new Android, macOS or Windows UI/device run was performed.

The native launch flag is documented in [Google's ACP permissions guide](https://antigravity.google/docs/ide/extensions/acp#permissions) and appears in this host's `agy acp --help`. It is not advertised as an ACP execution-mode option. Publication and installation are separate from this source validation.
