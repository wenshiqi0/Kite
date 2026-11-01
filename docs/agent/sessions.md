# Saved sessions

`kite sessions` reads saved provider references and starts no model turn. `kite notifications` reads the host's notification page; retain its cursor for the next `kite notifications --after CURSOR`. `kite notification-get ID` retrieves an identified event.

These interfaces are usable from any shell-capable harness. They do not make that harness a supported chat provider. Managed chat providers depend on the target host's verified availability and the product's implementation status.

A saved session, an agent process and a connected client have distinct lifetimes. Reading a record is not authorization to send a message, approve a tool request, resume archived work or terminate its owner. Desktop control remains separate from pausing agents.
