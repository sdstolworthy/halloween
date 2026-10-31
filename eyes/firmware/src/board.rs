//! HUB75 wiring tables. The active board is the Adafruit MatrixPortal S3 (product 5778): the
//! HUB75 socket is hardwired on the board, so this map mirrors Adafruit's routing (verified
//! against the CircuitPython board definition, ports/espressif/boards/adafruit_matrixportal_s3)
//! and must not be edited to "fix" an image problem — the copper decides. The original
//! WROOM-32 map is kept below for reverting to a hand-wired DevKit.

use crate::matrix::Hub75Pins;

pub const MATRIX_PORTAL_S3_PINS: Hub75Pins = Hub75Pins {
    r1: 42,
    g1: 41,
    b1: 40,
    r2: 38,
    g2: 39,
    b2: 37,
    a: 45,
    b: 36,
    c: 48,
    d: 35,
    // 128x64 panels are 1/32 scan and need the E address line. On the MatrixPortal S3 the E line
    // is GPIO21, brought to the socket's pin 8.
    e: 21,
    clk: 2,
    lat: 47,
    oe: 14,
    // The library default. With the wrong phase the image shifts a pixel and loses an edge column.
    clkphase: 1,
};

/// The ESP32-WROOM-32 wiring, identical to flight-rs's `board/wroom32.rs`. GPIO12 (G2) is a
/// strapping pin that selects the flash voltage and must read low at reset: if that board won't
/// boot or flash with the panel attached, unplug the ribbon to confirm.
#[allow(dead_code)]
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
    e: 32,
    clk: 16,
    lat: 4,
    oe: 15,
    clkphase: 1,
};
