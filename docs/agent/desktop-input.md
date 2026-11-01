# Desktop input and capture details

`status` returns `display`, source `width`/`height`, and the active window. `windows` lists window geometry when needed. Capture full-screen first when orientation is unknown, then use a region. A crop has `origin:{x,y}` and native pixel dimensions; click coordinates remain global. Refreshed geometry is required after display/layout changes.

Desktop MCP parameters:

- `status`: no parameters.
- `windows`: optional `limit` (1…64, default 16).
- `screenshot`: optional `region:{x,y,width,height}` and `known_sha256`.
- `move`: `x`, `y`.
- `click`: `x`, `y`, optional `button` (`left`, `middle`, `right`; default left).
- `scroll`: integer `dx`, `dy`; each within -50…50. Optional paired `x`,`y` positions the pointer first.
- `key`: `keys`, an array of at most 16 complete chords. Symbols use X11 key names (`Return`, `Escape`, `ctrl+shift+t`, `space`). Separate held-key ownership is not exposed.
- `type`: `text`, at most 512 characters / 2,048 UTF-8 bytes. CLI takes it only as `type --stdin`; stdout never repeats it.
- `launch`: `argv`, 1…32 arguments and at most 8,192 UTF-8 bytes, optional absolute `cwd`. Characters are literal arguments; no shell is inserted automatically.

CLI scroll accepts `DX DY` plus optional `--at X Y`. Screenshot optional `--output FILE.png` copies its owned artifact to your destination. Do not pass `--output` through MCP; its image block is already usable.

Capture failure, invalid parameters and missing input helpers return errors. A successful dispatch is not proof that the app reacted. Images and input are sensitive desktop data; local artifacts are private. Older Kite-owned captures may be cleaned up after an hour; copy an image if it must be retained. Text input and screenshots are not written to diagnostic logs. Use file transfer or an app API for large text rather than many simulated keystrokes.
