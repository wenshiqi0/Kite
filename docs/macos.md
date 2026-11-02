# macOS client

Kite's Mac client uses SwiftUI/AppKit and the same Rust Core as Android. It connects to a remote Linux Kite host over SSH, shows sessions and Chat, and receives Sunshine desktop video and sound. It does not require local Herdr, Sunshine or Kite Node. macOS hosting is a separate, pending stage.

## Build and run

Requires macOS 14+, Xcode 16+, Rust stable, Bun and XcodeGen. Initialize the native submodules and workspace dependencies first:

```sh
git submodule update --init --recursive
bun install --frozen-lockfile
security find-identity -v -p codesigning
macos/scripts/configure-signing.sh <certificate-SHA-1>
macos/scripts/install-keychain-helper.sh
macos/scripts/build.sh
open macos/build/DerivedData/Build/Products/Debug/Kite.app
```

The build generates Swift bindings from the same Rust static library it links. Generated projects, bindings and build output stay outside version control. The build script serializes the Rust/binding stage across concurrent Xcode builds. Local installation pins a persistent code-signing certificate in the user's application-support directory. The certificate and bundle ID keep the app's designated requirement stable across updates. An unavailable identity fails the signing step; CI explicitly sets `KITE_MAC_ADHOC=1` for disposable compilation builds. Developer ID distribution and notarization are separate.

The native Keychain component installs once in the user's private Application Support directory. Normal builds leave it unchanged. It accepts reads/writes only from the certificate-signed Kite parent process, and handles only the existing SSH and Sunshine identity items. Secret bytes travel through private pipes; normal operations disable authorization UI and return an error when access is unavailable. There is no daemon or network listener.

For identities created by an earlier ad hoc build, the explicit `install-keychain-helper.sh` setup checks existing authorization without UI, then requests access only for outstanding items. Enter the login Keychain password in the system dialog and choose **Always Allow** for the fixed component. Re-running setup after authorization is complete does not request it again. No identity data is rotated or exported. This also addresses the Keychain partition check, which can retain a per-build hash for non-Apple signing identities even when the app's designated requirement is stable. [Design and limits](rfcs/0028-macos-keychain-access.md).

## Connect a machine

Add the remote machine's resolvable hostname, SSH username and port. The editor's advanced settings accept an explicit Herdr path, separate stream address and Sunshine base port. Copy this Mac's public key from the editor or Settings and add it to the remote account's `~/.ssh/authorized_keys`. Verify the SSH fingerprint through an independent trusted connection before accepting it.

This Mac creates its own Keychain identity and stores host/trust/conversation data in `~/Library/Application Support/io.github.wenshiqi0.kite.mac/core`. It never imports another device's private key. Editing a machine and connecting it are separate actions.

For a local network, use the machine's actual mDNS name, such as `my-linux.local`. On a Linux machine using Avahi, check `host-name` in `/etc/avahi/avahi-daemon.conf` and the currently published name in `ps -C avahi-daemon -o args=`. An SSH-config alias is not necessarily a resolvable network hostname. Same-LAN connections do not require Tailscale; the Mac must resolve and reach both SSH and Sunshine on that LAN.

For Surge Mac users who want all Kite traffic to connect directly, place this before broader rules:

```ini
[Rule]
PROCESS-NAME,Kite,DIRECT
DOMAIN,my-linux.local,DIRECT

[Host]
my-linux.local = server:force-syslib
```

Also include the specific local name in `[General]`'s `always-real-ip` list to avoid a fake address on the media path. Merge these entries into an existing profile rather than replacing it. Validate and reload the profile, then check TCP and UDP with `surge-cli rule explain ... process-path=/path/to/Kite.app/Contents/MacOS/Kite`. See Surge's [process rules](https://manual.nssurge.com/rules/process.html) and [local DNS mapping](https://manual.nssurge.com/dns/local-dns-mapping.html).

## Workspace

The sidebar contains machines and remote sessions, including archived sessions. New sessions use the host's reported harness catalog. Chat shows full synchronized history, native Markdown, collapsible tool output, attachments, approvals and questions. The composer offers the selected harness's models, reasoning levels, attachments and skills. Initialization gates sending until the actual session is ready. Command-Return sends a message.

The desktop opens separately and defaults to Watch. Operate enables physical mouse, keyboard and committed IME text. Watch-mode dragging/scrolling pans a zoomed image without sending remote input. Video and pointer mapping share the same aspect-fit/zoom transform. Window deactivation or closure releases keys and buttons; Command-Option-W exits operation from the video surface. Agent pause is a separate control.

Text clipboard sharing is off by default and can be enabled in Settings. Synchronization requires the foreground workspace and Operate mode. Disconnecting/closing the Mac client leaves remote agents and independently running host services alive.

The Dock/Finder icon is the existing four-panel orange Kite mark in native Mac icon sizes. Its editable generator is `macos/scripts/generate-icon.swift`.

## Validation boundaries

Run `macos/scripts/test.sh` for offline native H.264 decoding/restart, Opus packet decoding, Annex B framing and display/input geometry. Live tests skip unless an explicit owned-host configuration is supplied. They require an isolated 1920×1080 QA Sunshine instance on port 48200 and a disposable project; never point them at an active user desktop. The test host uses a distinct bundle identifier.

See the dated [Mac client acceptance record](product/2026-11-02/macos-client/acceptance.md) for the actual local and cross-machine results. Intel Macs, Developer ID/notarized distribution, physical audio devices, WAN/Tailscale quality and macOS hosting require separate validation.
