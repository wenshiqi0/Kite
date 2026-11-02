# Installed Mac client / real-host Computer Use — 2026-11-02

This run uses Codex's built-in `cua_repl` against `/Users/wenshiqi/Applications/Kite.app`, the user's saved `wen-linux.local` host and its normal Sunshine port 47989. All desktop UI actions use that driver. The earlier isolated port-48200 results are a separate validation scope.

## Passed in the installed UI

1. Clicked **Connect** and observed the machine card and footer change from disconnected to connected.
2. Clicked **Open desktop**, observed 1920×1080 streaming state and an actual Xfce desktop picture. An owned GTK test window later appeared in the same real capture display, with its frame counter advancing visibly.
3. Created the dedicated `Mac CUA 连通验证 20261102` Codex conversation in an owned project directory through the native form.
4. Pasted a complete Chinese test prompt, sent it through the visible Send button and observed the actual assistant response **MAC_CUA_OK**. No tool execution or file modification was requested from the model.
5. Disconnected/reconnected via the native toolbar and Connect button. The same conversation and reply remained visible. Reopened the real desktop and observed its picture again.
6. Verified Watch/Operate controls and expanded/restored the video panel through accessibility actions; restored Watch for cleanup. The Keychain authorization flow was not re-entered.

## Found and corrected

The local app inventory contained installed, DerivedData and signing-probe bundles with the same bundle ID. A bundle-ID-only selection was ambiguous. Unregistered only the owned build/probe copies and retained the installed app's registration. The project instructions now default to built-in Computer Use and identify the installed app by full path; the setup guide installs/opens that canonical bundle.

CUA's direct `typeText` under the current input-method state produced an incomplete Chinese draft. The driver paste method entered the full prompt correctly. This is recorded as a driver/input-method observation, rather than accepted physical IME coverage.

## Not accepted

CUA accessibility button actions worked, but coordinate clicks repeatedly returned **Computer Use server error -10005: noWindowsAvailable**, including after rebinding and in the expanded view. The controlled remote GTK fixture recorded zero clicks, scrolling or F8 events in this UI run. Physical mouse/keyboard mapping through the installed UI is therefore **not passed**, despite the earlier native media-API test succeeding. No Tactero or alternate desktop driver was used as a fallback.

The reported initial connection failure was not reproduced for SSH, chat or visible desktop video in this run. The user-visible distinction remains: connect the machine first, then open its desktop separately. Complete physical input/IME/clipboard testing, manual questions/approvals, physical audio/Bluetooth/A-V sync, WAN/Tailscale quality, Intel Macs and Developer ID/notarized distribution remain outstanding.

The own test fixture has a bounded lifetime and has closed; the original Sunshine service remains running. The completed test conversation is retained so the user can inspect the actual reply.
