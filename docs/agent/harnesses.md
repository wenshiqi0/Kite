# Harness integration

The same commands and docs serve Codex, Claude, DeepSeek Harness and other consumers; no private tool namespace or prompt interpolation is required.

With shell access, use `kite docs desktop` directly and open screenshot PNGs with the harness's image viewer. Without image support, report that limitation instead of claiming to see the desktop.

For standard MCP stdio, configure the installed executable with arguments `["mcp"]`. Use its absolute path if the host's PATH omits the install directory. Kite exposes short `kite_docs` and `kite_desktop` tools alongside host/session queries. Tool namespaces assigned by the client are not part of Kite's contract. Image rendering depends on the current harness/model; screenshots are standard PNG image blocks.

An optional standard Agent Skills bundle is installed with `install.py --skill-dir DIRECTORY`. Use a discovery root supported by your harness, for example a shared `.agents/skills` root or Claude's `.claude/skills`. It contains only a thin `SKILL.md`; full instructions remain in `kite docs`. `kite docs skill` emits that same bundle text. No configuration, permissions, model/provider routing or tool approval is changed by copying a skill.

Docs calls accept `topics:["desktop"]` and `known:{"desktop":"REV"}` in MCP. Keep only revisions whose bodies are still in context; omit `known` after losing a body to compaction. Multiple related topics can be requested together, without loading the complete manual.
