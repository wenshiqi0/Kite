# Multimodal Items and shared author presentation

The same Item/Block contract carries human, agent and tool content. Android uses
one Markdown renderer, card layout, copy action and attachment component. Authors
are labelled without changing content capabilities, alignment or colours.

The host preserves text/media order from Codex, Claude, DSH and Ply, including
media-only ACP replies and media in tool results. Native inline bytes and existing
Markdown image/file references become immutable attachment descriptors; the
provider's original history is never rewritten. Network resources download only
when viewed, with a 25 MiB bound, transport checksums and local caching. Existing
uploaded images retain their envelope identity instead of appearing twice.

Android images display inline and open the shared zoom/save preview. Audio and
video open native playback controls without autoplay. Text files have a bounded
preview; other files can be saved. The macOS client receives the same attachment
blocks and uses the existing image/save controls; no macOS execution is claimed.

Regression coverage includes stable record identity across presentation changes,
source-order pagination, cache round trips, optimistic input acknowledgement,
native ACP chunks, media-only messages, legacy attachment envelopes, local files
and lazy remote downloads. A generated two-second video fixture and synthetic
PCM audio are used only by native tests.

Static checks and native test results are recorded with the publication below.
No physical-handset, authenticated remote-media or real-model generation result
is inferred from the isolated fixtures.
