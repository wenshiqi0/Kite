# Shared agent docs and X11 control acceptance

Kite now owns the CLI/docs/MCP desktop contract described in [RFC 0019](../../../rfcs/0019-agent-docs-and-linux-desktop.md). The previous deployed hot-upgrade changes were committed to remote main as `e3ab192` before this work. This revision adds compiled, single-source task guides and a thin standard Skill bundle; no harness-specific tool prefixes, prompt interpolation, approval settings or new chat-provider routing are required.

`kite docs desktop` is directly useful without loading the index or an entire reference manual. Advanced input, host, session, harness and upgrade details remain separate topics. Matching explicit known revisions omit only the retained bodies. A new context gets a fresh body; no global “read already” flag exists. Docs and tool discovery are usable while the Node is missing. CLI is a complete path for consumers that already have shell/image-viewer tools; MCP adds four compact tools and native image blocks for consumers that need that integration.

## Cost measurements

[Pinned reference tokenizer measurements](token-budget.json) distinguish discovery, first use, optional detail and unchanged rechecks. These are `cl100k_base` / `o200k_base` counts, not Claude/DeepSeek's native accounting. They exclude image tokens, model reasoning and unrelated harness context.

| Path | Reference token count |
| --- | ---: |
| Skill discovery name/description | 30–34 |
| Skill plus common desktop guide, including discovery | 643–668 |
| Explicit unchanged topic recheck | 18 |
| All guides loaded once as a comparison | 1,976–2,013 |
| Four MCP tool definitions, if eagerly included | 297–303 per included schema set |

The common first-use path consumes roughly 67% less text than loading the complete manual once; unchanged-body rechecks consume roughly 96% less than resending that guide. An advanced task can fetch its extra topic in the same request or later, instead of repeatedly loading a giant Skill. Retained instructions need no call on every action. The MCP setup instruction has its own measured cost; a harness that eagerly includes schemas may pay that fixed cost every step. These comparisons establish the chosen tradeoff rather than a universal mathematical optimum for every workflow or tokenizer.

Screenshots use image blocks rather than base64 text. Region captures preserve native pixels and report their global origin. A caller-retained image hash omits identical images; image costs depend on the consumer and are not mixed with the text-token claims.

## Runtime and compatibility

[Owned X11 tests](native-validation.json) verified real chords/F8, left/right clicks, wheel events, committed Chinese text and the 512-character text bound, graphical launch with Node supervisor/harness identity removed, private PNGs, actual crop pixels/origin, standard MCP image blocks, unchanged-image omission, conditional docs and action-override rejection. [A second owned-window test on actual Xfce/X11](xfce-validation.json) verified the same ordinary operations through a private Node on display `:10`; it used no user workspace or real session catalogue. Owned test windows were closed.

The Unicode test found that an overly short key delay could drop a remapped character. Non-ASCII text now uses the input helper's normal delay and a documented bound/deadline. No clipboard contents are read or changed.

[Consumer checks](consumers.json) used the actual installed Codex skill catalogue and DeepSeek Harness filesystem provider to discover/load the same standard bundle. No model turn, provider routing or harness configuration was changed. Claude compatibility is based on its common Agent Skills and standard MCP contract; a real Claude model session was not exercised. DeepSeek Harness is a consumer example, not a newly supported Kite chat provider.

[Host regression](host-regression.json) covers actual managed-skill installation, preserving local edits, live handover, notification state ownership, supervisor recovery while old streams remain open and cleanup of all uniquely named test services. Normal source checks include Rust, Python, formatting and Clippy. CI runs the native desktop test and pinned tokenizer budgets; a real user-manager-only regression is explicitly skipped on runners without that facility.

The next development Alpha is used because an already-published tag cannot refer to this new source. No old release tag or attachment is overwritten. Physical-phone desktop streaming, macOS/Wayland input and model-level Claude/DeepSeek end-to-end behavior are outside this acceptance.
