// Thin forwarding shim: maps the plain-C surface in matrix_shim.h onto the
// mrcodetastic ESP32-HUB75-MatrixPanel-I2S-DMA C++ driver. Kept tiny on purpose
// — the durable logic (the eyes_hal::Matrix impl + its contract) lives in
// Rust (firmware/src/matrix.rs); this only bridges the language boundary.

#include "matrix_shim.h"

#include <ESP32-HUB75-MatrixPanel-I2S-DMA.h>

static MatrixPanel_I2S_DMA *s_panel = nullptr;
static bool s_double_buff = false;

int hub75_init(int32_t width, int32_t height, const hub75_pins_t *p,
               bool double_buff) {
    if (s_panel != nullptr) {
        return 1; // already initialised this boot
    }

    // Assign by member (not a brace list) so we don't depend on the library's
    // internal i2s_pins field order.
    HUB75_I2S_CFG::i2s_pins pins;
    pins.r1 = p->r1; pins.g1 = p->g1; pins.b1 = p->b1;
    pins.r2 = p->r2; pins.g2 = p->g2; pins.b2 = p->b2;
    pins.a = p->a; pins.b = p->b; pins.c = p->c; pins.d = p->d; pins.e = p->e;
    pins.lat = p->lat; pins.oe = p->oe; pins.clk = p->clk;

    HUB75_I2S_CFG cfg(
        static_cast<uint16_t>(width),
        static_cast<uint16_t>(height),
        1, // one panel in the chain
        pins);
    cfg.double_buff = double_buff;
    // Panel-dependent clock phase (see matrix_shim.h): the wrong value shows
    // as the image shifted one pixel with an edge column cut off.
    cfg.clkphase = p->clkphase != 0;

    s_panel = new MatrixPanel_I2S_DMA(cfg);
    if (s_panel == nullptr || !s_panel->begin()) {
        delete s_panel; // frees any partially-allocated DMA buffers
        s_panel = nullptr;
        return 0;
    }
    s_double_buff = double_buff;
    s_panel->clearScreen();
    return 1;
}

void hub75_draw_pixel(int32_t x, int32_t y, uint8_t r, uint8_t g, uint8_t b) {
    if (s_panel != nullptr) {
        s_panel->drawPixelRGB888(
            static_cast<int16_t>(x), static_cast<int16_t>(y), r, g, b);
    }
}

void hub75_flip_and_clear(void) {
    // Single-buffer mode: no-op. Pixels are already live and clearing would
    // blank the visible frame; the renderer repaints every pixel each frame.
    if (s_panel != nullptr && s_double_buff) {
        s_panel->flipDMABuffer();
        s_panel->clearScreen();
    }
}

void hub75_flip(void) {
    if (s_panel != nullptr && s_double_buff) {
        s_panel->flipDMABuffer();
    }
}

void hub75_set_brightness(uint8_t level) {
    if (s_panel != nullptr) {
        s_panel->setBrightness8(level);
    }
}
