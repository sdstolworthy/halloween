// Plain-C surface over the C++ ESP32-HUB75-MatrixPanel-I2S-DMA library, so the
// Rust FFI in `firmware/src/matrix.rs` stays `extern "C"`. The symbols and
// signatures here must match `mod ffi` in matrix.rs exactly.
#pragma once

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Mirrors `Hub75Pins` (#[repr(C)]) in matrix.rs — SAME field order.
typedef struct {
    int32_t r1, g1, b1;
    int32_t r2, g2, b2;
    int32_t a, b, c, d, e;
    int32_t clk, lat, oe;
    // HUB75 clock phase (0 or 1). Panel-dependent: with the wrong phase the
    // whole image shifts one pixel horizontally and the edge column is lost
    // ("content runs off the left side"). The library default is 1.
    int32_t clkphase;
} hub75_pins_t;

// Construct + begin() the panel. `double_buff` selects tear-free flips (two
// DMA framebuffers) vs the single-buffer fallback for boards whose DMA heap
// can't fit both. Returns 1 on success, 0 if the driver could not allocate
// DMA buffers (safe to call again with a cheaper config). Once it has
// returned 1, later calls are no-ops returning 1 (the `double_buff` argument
// is ignored on repeat calls).
int hub75_init(int32_t width, int32_t height, const hub75_pins_t *pins,
               bool double_buff);

// Write one pixel to the draw buffer (the back buffer in double-buffer mode,
// the live buffer in single-buffer mode; the library clips out-of-range
// coords).
void hub75_draw_pixel(int32_t x, int32_t y, uint8_t r, uint8_t g, uint8_t b);

// Double-buffer mode: present the back buffer (flip) and clear the new back
// buffer for the next frame. Single-buffer mode: no-op — pixels are already
// live, and clearing would blank the visible image.
void hub75_flip_and_clear(void);

// Global brightness, 0..=255.
void hub75_set_brightness(uint8_t level);

#ifdef __cplusplus
}
#endif
