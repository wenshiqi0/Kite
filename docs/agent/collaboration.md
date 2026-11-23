# Cross-machine sessions and tasks

`kite acp` exposes existing Kite session owners through authenticated ACP v1 JSON-RPC at WebSocket `/acp`. This is the WebSocket profile of the active remote-transport RFD; HTTP/2/SSE and full history replay are not implemented. New network sessions use verified Codex, Ply, DSH and Antigravity providers. Use the Kite session ID to continue an existing conversation. Normal Kite history remains available through its host connection.

Use session control v2 for every new remote task, including short tasks. Use the `kite_session_*` MCP tools or the CLI below. If a peer lacks v2, report that its gateway needs updating. The deprecated `kite_remote_task` tool and `kite acp task` command are reserved for existing automation.

On a Linux executing machine, run in its existing systemd user hierarchy so agent resource admission remains enforced:

```sh
systemd-run --user --unit kite-acp --collect \
  kite acp serve --workspace /absolute/workspace --port 49940
```

The endpoint binds loopback. It creates a private token at `~/.config/kite/acp/token`; keep tokens and provider credentials out of source, messages and logs. The server uses this host's existing provider configuration and resource budget.

On macOS, run `kite acp serve` directly. Stop only the owned gateway service; disconnecting a client does not stop its model owner.

On the calling machine, create a private `~/.config/kite/acp/peers.json`:

```json
{"peers":{"mini":{"sshTarget":"wenshiqi@wen-mini","url":"ws://127.0.0.1:49940/acp","tokenFile":"/absolute/private/mini.token"}}}
```

Copy the endpoint token through an already trusted SSH connection and keep the peer file/token owner-only. SSH host-key checking stays strict. Provider keys remain on the executing host.

Kite's peer client checks a nonce/HMAC endpoint proof before sending its Bearer token. Other ACP clients can use an authenticated WebSocket upgrade directly; ACP messages and IDs remain unchanged.

Use `kite acp proxy --peer mini` to attach another ACP client.

## Run a remote task

A peer advertising `controlPlaneVersion: 2` supports independent submission and observation. Kite binds the parent automatically when the CLI or MCP runs within a verified session. Leave the parent unset in that workflow. External callers can supply a parent machine and a stable session ID. Use `--standalone` (MCP `standalone: true`) for an explicitly requested independent session.

```sh
kite acp spawn --peer mini --cwd /absolute/workspace/project --provider ply \
  --label "Run checks" --write-lock --policy human_in_the_loop
kite acp send --peer mini --session CHILD_ID < task.txt
kite acp status --peer mini --session CHILD_ID
kite acp wait --target mini/CHILD_ID --timeout-ms 30000
kite acp read --peer mini --session CHILD_ID --turn TURN_ID
kite acp release --peer mini --session CHILD_ID
```

- `spawn` and `send` accept `--model`, `--effort`, and advertised Codex `--service-tier` values. `send --attachment PATH` accepts a file already inside the executing host's allowed workspaces. `--wait-ms` waits up to 30 seconds for a reply.
- Use `send --session ID` to continue the same remote context. The prompt comes from stdin. For revision-pinned builds, pass `send --expected-revision SHA` and use a clean checkout on the executing host.
- Recover from uncertain creation with `list --parent-machine caller-host --parent-session PARENT_ID --label "Run checks"`, following any pagination cursor. Recover from uncertain submission with `status` and `read` before deciding whether to send again.
- `wait` accepts multiple `--target PEER/SESSION` arguments. Append `/REVISION` from `status` to wait past a previously observed state. `read` returns public text and tool summaries with a truncation flag. Check structured tool status and exit codes before reporting a build as successful.
- `strict` is the default. Use `--allow-command` for an exact command in the session directory. `human_in_the_loop` needs a reachable Kite Node on the executing host. A person opens the target session to decide an approval or answer a question.
- Use `cancel` for an explicit interruption. `release` reclaims eligible idle owners while preserving history. `release --force-cancel` explicitly requests interruption before release.
- MCP exposes `kite_session_spawn`, `kite_session_send`, `kite_session_status`, `kite_session_read`, `kite_session_list`, `kite_session_wait_any`, `kite_session_cancel`, and `kite_session_release`.

A running, independently managed Linux ACP unit loads its adapter at startup. Schedule its refresh separately after its work has settled when an upgraded host still advertises an older ACP capability.
