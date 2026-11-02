# Saved sessions

`kite sessions` reads saved provider references and starts no model turn. `kite notifications` reads the host's notification page; retain its cursor for the next `kite notifications --after CURSOR`. `kite notification-get ID` retrieves an identified event.

These interfaces are usable from any shell-capable harness. They do not make that harness a supported chat provider. Managed chat providers depend on the target host's verified availability and the product's implementation status.

A saved session, an agent process and a connected client have distinct lifetimes. Reading a record is not authorization to send a message, approve a tool request, resume archived work or terminate its owner. Desktop control remains separate from pausing agents.

Resource release is a shared harness capability. The host checks managed owners every 30 seconds and wakes after catalog or session-state updates. Archived owners are released once current work finishes and restorable context is persisted. Unarchived owners never expire for inactivity. `kite sessions release-archived` explicitly runs the same cleanup and returns `reclaimed`, `retained` and `failed` counts; running, blocked, unknown or unpersisted work is retained. Android exposes this command in the archived session list. History and provider identity survive process reclamation; explicitly continuing a saved session recreates the owner on its original thread. `kite status` reports `sessionLifecycle` archive-only policy and scan facts.

Android also offers **Delete session** in active and archived lists and the current session menu. Confirming deletion explicitly stops that session’s managed work and releases its owner and local tools before removing the Kite record and local conversation cache. A failed release retains the record for retry. Shared owners and unverified running terminals are refused. Private identity markers prevent stale clients from recreating deleted entries; original provider history remains in place. The client and host must both include this operation.
