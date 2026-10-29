#pragma once
#include <stdint.h>

typedef struct {
    int (*configure)(void*, int, int, int);
    int (*frame)(void*, const uint8_t*, int, uint64_t, unsigned int);
    void (*stage)(void*, int, int);
    void (*terminated)(void*, int);
} KiteCallbacks;

int kite_ml_start(const char*, const char*, const char*, const char*, int,
    int, int, int, int, const uint8_t*, const uint8_t*, void*, const KiteCallbacks*);
void kite_ml_stop(void);
void kite_ml_interrupt(void);
const char* kite_ml_launch_query(void);
int kite_ml_mouse_position(int16_t, int16_t, int16_t, int16_t);
int kite_ml_mouse_move(int16_t, int16_t);
int kite_ml_mouse_button(int, int);
int kite_ml_scroll(int16_t, int16_t);
int kite_ml_key(int16_t, int, uint8_t);
int kite_ml_text(const uint8_t*, unsigned int);
void kite_ml_request_idr(void);
