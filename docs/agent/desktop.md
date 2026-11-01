# Linux desktop

Discover the configured target and inspect actual pixels before input:

```sh
kite desktop status
kite desktop windows
kite desktop screenshot
kite desktop click 400 300
kite desktop move 500 400
kite desktop scroll 0 3
kite desktop key ctrl+l
printf '%s' 'example text' | kite desktop type --stdin
kite desktop launch -- firefox
```

Coordinates are global source pixels reported by status, not device-independent units. A positive scroll count moves right/down; left/up is negative. Key arguments are complete press/release chords such as `ctrl+l`, `Return` or `alt+Tab`. Text is committed UTF-8 input, sent over stdin without changing the clipboard. Launch uses an argv array and the matching graphical session environment; no shell expansion occurs.

CLI results are JSON. Screenshot returns a private PNG path, size, SHA and global origin; open the file with your harness's image viewer. The standard MCP `kite_desktop` tool returns actual image blocks. Never paste base64 into chat or claim visual verification from a path alone. Read back after an action; if its result is uncertain, inspect before repeating it.

For fewer image tokens, capture just the relevant region:

```sh
kite desktop screenshot --region 100 200 640 480
```

Add the returned origin to coordinates read inside that crop. Pass `--known-sha256 SHA` only while you still have that exact image; identical pixels return `unchanged` without another image. Blinking UI can legitimately change the hash.

MCP uses `kite_desktop` with `action` and an optional `params` object, for example `{"action":"click","params":{"x":400,"y":300}}`. Actions match the CLI names; key uses `keys:["ctrl+l"]`, text uses `text`, launch uses `argv`. Advanced fields and limits: `kite docs desktop/input`.

Operations affect the configured host, including its signed-in apps. Keep them within the user's task. X11 access errors and an unsupported session are real failures; do not select an unrelated display or infer success from healthy host services.
