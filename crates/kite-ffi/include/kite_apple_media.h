#ifndef KITE_APPLE_MEDIA_H
#define KITE_APPLE_MEDIA_H
#include <stdint.h>

// Callbacks must be thread safe and must not wait for the main thread. Frame
// bytes are borrowed only for the call. A successful register takes one retain;
// release runs after detach AND the last in-flight stream/callback owner exits.
typedef struct {
    void (*release)(void *context);
    uint8_t (*supports)(void *context, uint32_t width, uint32_t height, uint32_t fps);
    uint8_t (*configure)(void *context, uint32_t width, uint32_t height, uint32_t fps, uint64_t generation);
    uint8_t (*frame)(void *context, const uint8_t *bytes, uint32_t length, uint64_t pts_us, uint32_t flags, uint64_t generation);
    void (*audio_configure)(void *context, uint32_t rate, uint32_t channels, uint32_t samples, uint64_t generation);
    void (*audio_frame)(void *context, const uint8_t *bytes, uint32_t length, uint64_t generation);
    void (*stopped)(void *context, uint64_t generation);
} KiteAppleMediaCallbacks;
uint64_t kite_apple_media_register(void *context, const KiteAppleMediaCallbacks *callbacks);
void kite_apple_media_detach(uint64_t handle);
void kite_apple_media_frame_rendered(uint64_t handle, uint64_t generation);
void kite_apple_media_decoder_failed(uint64_t handle, uint64_t generation, int32_t error);
uint8_t kite_apple_media_mouse_position(uint64_t handle, int16_t x, int16_t y, int16_t width, int16_t height);
uint8_t kite_apple_media_mouse_button(uint64_t handle, uint8_t button, uint8_t down);
uint8_t kite_apple_media_scroll(uint64_t handle, int16_t horizontal, int16_t vertical);
uint8_t kite_apple_media_key(uint64_t handle, int16_t key, uint8_t down, uint8_t modifiers);
uint8_t kite_apple_media_text(uint64_t handle, const uint8_t *text, uint32_t length);
void kite_apple_media_release_input(uint64_t handle);
void kite_apple_media_request_idr(uint64_t handle);
#endif
