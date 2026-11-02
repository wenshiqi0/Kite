# Desktop client and host

Kite's desktop application targets **both client and host roles** on Linux and macOS. A desktop can connect to another machine's Kite node, chat with its agents and view or operate its desktop, while continuing to serve its own agents and desktop to other devices.

The main window is a workspace: a machine list, the selected machine's sessions and Chat, and its remote desktop. Local service management remains available in a separate panel and tray. Local service health and the selected remote machine's connection state are shown separately.

The roles have independent lifecycles. Disconnecting a remote machine releases its input without stopping its agents or this machine's host services. Client use does not require local Herdr or Sunshine; the remote host provides those services. A single desktop may serve incoming clients while maintaining an outgoing connection to another desktop.

## Current delivery

The Linux Alpha includes the [host node, panel and taskbar status](linux-desktop.md). The [macOS client](macos.md) now has a SwiftUI remote workspace with shared Core/FFI, Apple media decoding and physical input. The user requested Mac first; [RFC 0027](rfcs/0027-macos-client.md) records this change in implementation order. Its [dated acceptance](product/2026-11-02/macos-client/acceptance.md) distinguishes tested functionality from release and remaining platform scope. Linux client UI and macOS host adapters remain pending.

Desktop client acceptance requires a real connection between distinct machines: sessions and history, chat and interactive questions, actual decoded desktop video, mapped input, reconnection and independent local-service lifetime. Host status, CLI access and MCP queries alone do not demonstrate this capability.

The accepted contract and implementation boundaries are in [RFC 0015](rfcs/0015-desktop-client-and-host.md); delivered work and remaining stages are tracked in the [implementation plan](implementation-plan.md).
