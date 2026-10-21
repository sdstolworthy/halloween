//! The ESP32-WROOM-32 HUB75 wiring, identical to flight-rs's `board/wroom32.rs` (see
//! flight-rs firmware/ESP32_WROOM32.md for the ribbon-by-ribbon table). Confirm against the panel's
//! silkscreen before flashing; a wrong pin only garbles the image.

use crate::matrix::Hub75Pins;

pub const WROOM32_PINS: Hub75Pins = Hub75Pins {
    r1: 25,
    g1: 26,
    b1: 27,
    r2: 14,
    g2: 12,
    b2: 13,
    a: 23,
    b: 19,
    c: 5,
    d: 17,
    // 128x64 panels are 1/32 scan and need the E address line.
    e: 32,
    clk: 16,
    lat: 4,
    oe: 15,
    // The library default. With the wrong phase the image shifts a pixel and loses an edge column.
    clkphase: 1,
};
