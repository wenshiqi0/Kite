# Plan allowance

Open the **model and reasoning** button immediately to the left of the message field. Its **Plan allowance** card shows the current remote Codex account's plan and available quota windows. Open the card for reset times and detailed allowances. This entry is kept with model selection, outside the workspace's upper-right menu.

Every window is independent. The window length comes from the provider, so the UI can show hours, days or another reported duration. Remaining percentages are computed from the provider's used percentage. Optional extra credits are shown in the provider's units. An absent value stays unavailable; it is not displayed as zero or unlimited. Reset times use the phone's local time zone, and an expired window is marked for refresh rather than automatically filled.

Queries use `account/read` with `refreshToken=false` and `account/rateLimits/read` on the same app-server owner as the selected conversation. No additional account login or API key is requested. The feature requires a connected Codex conversation with a ChatGPT account and quota data exposed by that runtime. API-key-only accounts and providers that do not expose plan allowance receive an explanation. Claude allowance is not connected.

Account queries run independently of chat. They refresh at most once per minute automatically, update on provider notifications and can be refreshed from the detail panel. A failed query shows an unavailable state or labels the previous result as stale; it does not disable chat or infer permission to send from a percentage. Account changes clear old allowance data. Email addresses, account identifiers, credentials and allowance history are not persisted by Kite.

The [protocol decision](rfcs/0006-rich-composer.md) records ownership and lifecycle details. The implementation uses the [official Codex account/limits API](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt) and the running Codex 0.164.0 schema.
