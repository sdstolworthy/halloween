# Provenance

Copied from `firmware/components/matrix_shim/` in [sdstolworthy/flight-rs](https://github.com/sdstolworthy/flight-rs)
at rev `2ebde02a4941749724e1132f0514557120769526`. Changes since: a comment names `eyes_hal::Matrix` instead of
`flight_hal::Matrix`, and `hub75_flip()` was added (present without `clearScreen()`, since the eyes repaint every
pixel each frame). flight-rs is the same author's private repository, so the links in this file aren't public.

It is copied rather than referenced because flight-rs is a private repository and this one is public, and
ESP-IDF components cannot come from a Cargo git dependency anyway. The C++ driver it wraps,
mrcodetastic/ESP32-HUB75-MatrixPanel-DMA tag 3.0.11, is fetched at build time by the IDF component manager
(`idf_component.yml`, mirrored in `firmware/Cargo.toml`). If flight-rs fixes the shim, port the fix here.
