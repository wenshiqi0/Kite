# Cross-machine sessions and tasks

`kite acp` exposes existing Kite session owners through authenticated ACP v1 JSON-RPC at WebSocket `/acp`. This is the WebSocket profile of the active remote-transport RFD; HTTP/2/SSE and full history replay are not implemented. New network sessions use verified Codex/Ply/DSH providers. Existing Codex/Ply/DSH sessions can be attached with `session/resume`; check negotiated capabilities. Normal Kite history remains available through its host connection.

On the executing machine:

```sh
kite acp serve --workspace /absolute/workspace --port 49940
```

The endpoint binds loopback. It creates a private token at `~/.config/kite/acp/token`; keep tokens and provider credentials out of source, messages and logs. The server uses this host's existing provider configuration and resource budget.

On the calling machine, create a private `~/.config/kite/acp/peers.json`:

```json
{"peers":{"mini":{"sshTarget":"wenshiqi@wen-mini","url":"ws://127.0.0.1:49940/acp","tokenFile":"/absolute/private/mini.token"}}}
```

Copy the endpoint token through an already trusted SSH connection and keep the peer file/token owner-only. SSH host-key checking stays strict. Provider keys remain on the executing host.

Kite's peer client checks a nonce/HMAC endpoint proof before sending its Bearer token. Other ACP clients can use an authenticated WebSocket upgrade directly; ACP messages and IDs remain unchanged.

```sh
kite acp task --peer mini --cwd /absolute/workspace/project \
  --provider ply --expected-revision 0123456789012345678901234567890123456789 \
  --allow-command 'bun run typecheck' < task.txt
```

The prompt comes from stdin; stdout is one result containing the remote session ID, stop reason and reply. Progress and permission events use stderr. The optional exact command authorization applies only in the named directory; other permissions are rejected. `end_turn` means the agent finished, so inspect actual tool results before calling a build successful. A requested revision must match a clean tracked checkout.

Use `--session ID` to continue the same remote context. Transport failure does not replay input or cancel the owner. Explicit cancellation is forwarded to the active prompt. For another ACP client use `kite acp proxy --peer mini`; for MCP use `kite acp mcp`, which provides `kite_remote_task`. Every task names its peer, directory and optional session; no automatic load balancing is enabled.
