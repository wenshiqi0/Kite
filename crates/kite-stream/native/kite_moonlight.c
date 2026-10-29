#include "kite_moonlight.h"
#include "Limelight.h"
#include <stdlib.h>
#include <string.h>
#include <stdatomic.h>

#define MAX_FRAME (8 * 1024 * 1024)
typedef struct {
    KiteCallbacks callbacks;
    void* opaque;
    uint8_t* buffer;
    char *address, *app_version, *gfe_version, *rtsp_url;
} KiteContext;
static _Atomic(KiteContext*) current;
static atomic_int cancelled;

static void free_context(KiteContext* c) {
    if (!c) return;
    free(c->buffer); free(c->address); free(c->app_version); free(c->gfe_version); free(c->rtsp_url); free(c);
}
static int setup(int format, int width, int height, int fps, void* context, int flags) {
    (void)flags;
    KiteContext* c = context;
    if (format != VIDEO_FORMAT_H264) return -1;
    return c->callbacks.configure(c->opaque, width, height, fps) ? 0 : -1;
}
static int submit(PDECODE_UNIT frame) {
    KiteContext* c = atomic_load(&current);
    if (!c || frame->fullLength <= 0 || frame->fullLength > MAX_FRAME) return DR_NEED_IDR;
    // Reuse one native allocation. Parameter sets precede picture data in decoder order.
    for (int config = 1; config >= 0; config--) {
        int length = 0;
        for (PLENTRY e = frame->bufferList; e; e = e->next) {
            if ((e->bufferType != BUFFER_TYPE_PICDATA) != config) continue;
            if (e->length <= 0 || e->length > MAX_FRAME - length) return DR_NEED_IDR;
            memcpy(c->buffer + length, e->data, e->length); length += e->length;
        }
        if (length && !c->callbacks.frame(c->opaque, c->buffer, length,
            frame->presentationTimeUs, config ? 1u : (frame->frameType == FRAME_TYPE_IDR ? 2u : 0u))) return DR_NEED_IDR;
    }
    return DR_OK;
}
static void stage_start(int stage) {
    KiteContext* c = atomic_load(&current);
    if (c) c->callbacks.stage(c->opaque, stage, 0);
    if (atomic_load(&cancelled)) LiInterruptConnection();
}
static void stage_failed(int stage, int error) {
    KiteContext* c = atomic_load(&current);
    if (c) c->callbacks.stage(c->opaque, stage, error);
}
static void terminated(int error) {
    KiteContext* c = atomic_load(&current);
    if (c) c->callbacks.terminated(c->opaque, error);
}
static void discard_log(const char* format, ...) { (void)format; }
static int audio_init(int config, const POPUS_MULTISTREAM_CONFIGURATION opus, void* context, int flags) {
    (void)config; (void)opus; (void)context; (void)flags; return 0;
}
static void discard_audio(char* sample, int size) { (void)sample; (void)size; }

int kite_ml_start(const char* address, const char* app, const char* gfe, const char* rtsp, int codecs,
    int width, int height, int fps, int bitrate, const uint8_t* key, const uint8_t* iv,
    void* opaque, const KiteCallbacks* callbacks) {
    atomic_store(&cancelled, 0);
    KiteContext* c = calloc(1, sizeof(*c));
    if (!c) return -12;
    c->callbacks = *callbacks; c->opaque = opaque;
    c->buffer = malloc(MAX_FRAME);
    c->address = strdup(address); c->app_version = strdup(app); c->rtsp_url = strdup(rtsp);
    c->gfe_version = gfe && gfe[0] ? strdup(gfe) : NULL;
    if (!c->buffer || !c->address || !c->app_version || !c->rtsp_url) { free_context(c); return -12; }
    atomic_store(&current, c);
    SERVER_INFORMATION server; LiInitializeServerInformation(&server);
    server.address = c->address; server.serverInfoAppVersion = c->app_version;
    server.serverInfoGfeVersion = c->gfe_version; server.rtspSessionUrl = c->rtsp_url;
    server.serverCodecModeSupport = codecs;
    STREAM_CONFIGURATION stream; LiInitializeStreamConfiguration(&stream);
    stream.width = width; stream.height = height; stream.fps = fps; stream.bitrate = bitrate;
    stream.packetSize = 1024; stream.streamingRemotely = STREAM_CFG_LOCAL;
    stream.audioConfiguration = AUDIO_CONFIGURATION_STEREO;
    stream.supportedVideoFormats = VIDEO_FORMAT_H264; stream.clientRefreshRateX100 = fps * 100;
    stream.colorSpace = COLORSPACE_REC_709; stream.colorRange = COLOR_RANGE_LIMITED;
    stream.encryptionFlags = ENCFLG_ALL;
    memcpy(stream.remoteInputAesKey, key, 16); memcpy(stream.remoteInputAesIv, iv, 16);
    DECODER_RENDERER_CALLBACKS video; LiInitializeVideoCallbacks(&video);
    video.setup = setup; video.submitDecodeUnit = submit;
    video.capabilities = CAPABILITY_DIRECT_SUBMIT | CAPABILITY_SLICES_PER_FRAME(1);
    AUDIO_RENDERER_CALLBACKS audio; LiInitializeAudioCallbacks(&audio);
    audio.init = audio_init; audio.decodeAndPlaySample = discard_audio; audio.capabilities = CAPABILITY_DIRECT_SUBMIT;
    CONNECTION_LISTENER_CALLBACKS listener; LiInitializeConnectionCallbacks(&listener);
    listener.stageStarting = stage_start; listener.stageFailed = stage_failed;
    listener.connectionTerminated = terminated; listener.logMessage = discard_log;
    int result = LiStartConnection(&server, &stream, &listener, &video, &audio, c, 0, c, 0);
    if (result) { atomic_store(&current, NULL); free_context(c); }
    return result;
}
void kite_ml_stop(void) {
    KiteContext* c = atomic_load(&current);
    if (c) { LiStopConnection(); atomic_store(&current, NULL); free_context(c); }
}
void kite_ml_interrupt(void) { atomic_store(&cancelled, 1); if (atomic_load(&current)) LiInterruptConnection(); }
const char* kite_ml_launch_query(void) { return LiGetLaunchUrlQueryParameters(); }
int kite_ml_mouse_position(int16_t x, int16_t y, int16_t w, int16_t h) { return LiSendMousePositionEvent(x, y, w, h); }
int kite_ml_mouse_move(int16_t x, int16_t y) { return LiSendMouseMoveEvent(x, y); }
int kite_ml_mouse_button(int button, int down) { return LiSendMouseButtonEvent(down ? BUTTON_ACTION_PRESS : BUTTON_ACTION_RELEASE, button); }
int kite_ml_scroll(int16_t horizontal, int16_t vertical) {
    int result = vertical ? LiSendHighResScrollEvent(vertical) : 0;
    if (!result && horizontal) result = LiSendHighResHScrollEvent(horizontal);
    return result;
}
int kite_ml_key(int16_t key, int down, uint8_t modifiers) { return LiSendKeyboardEvent(key, down ? KEY_ACTION_DOWN : KEY_ACTION_UP, modifiers); }
int kite_ml_text(const uint8_t* text, unsigned int length) { return LiSendUtf8TextEvent((const char*)text, length); }
void kite_ml_request_idr(void) { LiRequestIdrFrame(); }
