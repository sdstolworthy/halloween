//! The ESP32-WROOM-32 HUB75 wiring, identical to flight-rs's `board/wroom32.rs`. The ribbon-by-ribbon
//! table and wiring warnings are in eyes/README.md. Confirm against the panel's silkscreen before
//! flashing; a wrong signal pin only garbles the image.
//!
//! GPIO12 (G2) is a strapping pin that selects the flash voltage and must read low at reset: if the board
//! won't boot or flash with the panel attached, unplug the ribbon to confirm. GPIO5 and GPIO15 are also
//! strapping pins, but their default states are compatible with normal boot here.

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
