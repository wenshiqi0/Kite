# Development

Kite shares a Rust core between native Android and iOS clients. Rust owns SSH, herdr and Sunshine protocols, domain state and persistence; Kotlin and Swift own UI, lifecycle, secure storage and media presentation. The full rules are in [SPEC](../SPEC.md) and the [RFC index](rfcs/README.md).

## Source layout

| Path | Responsibility |
| --- | --- |
| `crates/kite-ssh` | SSH sessions, device keys and strict host verification |
| `crates/kite-herdr` | herdr discovery, bridge checks and JSON API |
| `crates/kite-store` | Host, public trust and pairing state |
| `crates/kite-stream` | Sunshine control and native Moonlight transport |
| `crates/kite-app` | Persistent Core, snapshots and task lifecycle |
| `crates/kite-ffi` | UniFFI, Android JNI and platform adapters |
| `android` | Kotlin and Jetpack Compose app |
| `ios` | SwiftUI app for iPhone and iPad |

Initialize the pinned native sources, including Moonlight's ENet and nanors submodules:

```bash
git submodule update --init --recursive
```

Build output and generated bindings are ignored. Do not commit `target/`, Android `build/` directories, `ios/build/`, `ios/Generated/` or the generated Xcode project. FFI libraries and their generated Kotlin / Swift bindings must come from the same build.

## Android

Use JDK 21, Rust stable, Android SDK 37, Build Tools 37.0.0 and NDK 30.0.16248370. Perl and make are required for vendored OpenSSL.

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
sdkmanager 'platforms;android-37.0' 'build-tools;37.0.0' 'ndk;30.0.16248370'
```

Set `ANDROID_HOME` to your SDK directory and `JAVA_HOME` to the JDK. Gradle builds the optimized Rust library and regenerates Kotlin bindings before compiling the app:

```bash
./android/gradlew -p android :app:assembleDebug
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

The default ABI is `arm64-v8a`. To use an x86_64 emulator, install the `x86_64-linux-android` Rust target and set `kite.rustAbis=arm64-v8a,x86_64` in `android/gradle.properties`. The APK and Rust build use the same ABI setting.

Android's app version is maintained in `android/version.properties`. User deliveries use the signed Release variant and the [Alpha release workflow](releases.md); a debug build is for local development.

In the Android emulator, `10.0.2.2` addresses the development computer. It does not address a separate Sunshine host. Video traffic must reach the host's private address directly.

## iOS

Use Xcode 16 or newer, XcodeGen, and the ARM64 iOS targets:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
brew install xcodegen
ios/scripts/build-rust.sh
cd ios
xcodegen generate
open Kite.xcodeproj
```

The Xcode build phase rebuilds the current platform's Rust static library and Swift bindings. Command-line simulator compilation without code signing is:

```bash
xcodebuild -project ios/Kite.xcodeproj -target Kite -sdk iphonesimulator ARCHS=arm64 ONLY_ACTIVE_ARCH=YES CODE_SIGNING_ALLOWED=NO build
```

The current script builds the ARM64 simulator target. A successful iOS build validates compilation, not an iOS desktop renderer. iPhone and iPad layouts adapt to window width, including narrow split-screen windows; see [RFC 0002](rfcs/0002-ios.md).

## Checks

Run the [testing checks](testing.md) before delivery. GitHub Actions also builds the Android Release variant and checks Rust and Android lint; signing and publishing behavior are described in [Alpha releases](releases.md).

## Conversation and input presentation

The host projection in backend/kite_codex.py installs as backend-v5.py through Kite Node. It sends gzip-compressed record batches with source positions; kite-agent keeps message identity independent of transport metadata, and kite-app automatically builds and persists complete conversations. Rust snapshots hold shared immutable messages. CoreSnapshot contains metadata only; conversation_since supplies versioned range edits or an explicit reset. Kotlin maintains a read-only presentation mirror and stable tool groups. Video telemetry carries no message body. Scrolling is a native-only operation and never starts a history SSH request. See [RFC 0013](rfcs/0013-complete-transcript-and-dark-workspace.md).

`WorkspaceGeometry` budgets portrait desktop, controls, optional trackpad, Chat and composer in device-independent units. `DesktopViewport` remains the single source-to-view coordinate transform. `RemoteInputConnection` preserves IME composition; `RemoteTextBuffer` batches committed text over a fixed 200 ms window and invalidates pending callbacks when a destination closes. Keyboard, pointer and modifier transitions flush committed text in order.

The opt-in `LiveDesktopInputTest` requires an explicitly named isolated 1080p QA desktop on Sunshine base port 48200, a test-only host relay and a synthetic saved session. It skips by default and must never be pointed at a user's active desktop. Ordinary native rendering tests need no remote host.

LongTranscriptPresentationTest is an opt-in owned-host check for 10,000 records, bounded native view creation and message-object stability across real video-statistic updates. Chat uses an explicit one-screen-ahead / half-screen-behind cache window. Long literal tool output is also rendered as virtual chunks.
