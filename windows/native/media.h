#pragma once
#include <stdint.h>
#ifdef KITE_MEDIA_IMPORT
#define KITE_API __declspec(dllimport)
#else
#define KITE_API __declspec(dllexport)
#endif
#ifdef __cplusplus
extern "C" {
#endif
typedef void (*KitePixels)(void*, const uint8_t*, uint32_t, uint32_t, uint32_t, uint64_t);
typedef void (*KiteMediaError)(void*, int32_t, uint64_t);
KITE_API void* kite_decoder_create(void* context, KitePixels pixels, KiteMediaError error);
KITE_API void kite_decoder_destroy(void* decoder);
KITE_API uint8_t kite_decoder_configure(void* decoder, uint32_t width, uint32_t height, uint32_t fps, uint64_t generation);
KITE_API uint8_t kite_decoder_frame(void* decoder, const uint8_t* data, uint32_t length, uint64_t pts, uint32_t flags, uint64_t generation);
KITE_API void kite_decoder_audio_configure(void* decoder, uint32_t rate, uint32_t channels, uint32_t samples, uint64_t generation);
KITE_API void kite_decoder_audio(void* decoder, const uint8_t* data, uint32_t length, uint64_t generation);
KITE_API void kite_decoder_mute(void* decoder, uint8_t mute);
KITE_API void kite_decoder_stop(void* decoder, uint64_t generation);
#ifdef __cplusplus
}
#endif
