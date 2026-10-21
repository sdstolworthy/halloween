//! Porch eyes firmware: ESP32-WROOM-32 driving a 128x64 HUB75 panel.
//!
//! All behaviour lives in the host-tested crates: `eyes-core` (geometry, state machine), `eyes-render`
//! (pixels) and `eyes-hal` (the frame loop). This crate only supplies the board: the HUB75 matrix, a
//! clock, a hardware-seeded RNG and an MQTT inbox fed by the network thread.

mod board;
mod clock;
mod matrix;
mod net;
mod provision;

use std::time::{Duration, Instant};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use eyes_core::layout::{PANEL_HEIGHT, PANEL_WIDTH};
use eyes_core::SplitMix64;
use eyes_hal::App;
use log::{error, info};

use crate::clock::EspClock;
use crate::matrix::Hub75Matrix;
use crate::provision::Provisioning;

/// 30 fps. The DMA driver refreshes the panel independently of this, so the frame rate only sets how
/// smoothly the gaze moves; 30 fps is smooth for spring motion and leaves most of core 1 idle. Each frame
/// shades and pushes all 8192 pixels; the stats log shows the real cost so 60 fps can be tried on the bench.
const FRAME_PERIOD: Duration = Duration::from_millis(33);
// Wi-Fi, lwIP, esp-mqtt and the network thread are pinned to core 0 (sdkconfig.defaults, net.rs), so this
// loop shares core 1 only with the panel's DMA interrupt, which begin() registered here.
const STATS_PERIOD: Duration = Duration::from_secs(30);
/// Never sleep less than one FreeRTOS tick, even when a frame runs over, so IDLE1 runs and feeds the task
/// watchdog.
const MIN_SLEEP: Duration = Duration::from_millis(1);
/// Long enough to read the reason on the serial console, short enough to recover by itself.
const RESTART_DELAY: Duration = Duration::from_secs(10);

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();
    info!("porch eyes firmware {} starting", env!("CARGO_PKG_VERSION"));

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let provisioning = Provisioning::baked();

    // The matrix first, while internal RAM is still one unfragmented region: the DMA framebuffers are the
    // largest allocations on the board.
    let matrix = match Hub75Matrix::new(PANEL_WIDTH, PANEL_HEIGHT, board::WROOM32_PINS) {
        Ok(matrix) => matrix,
        Err(reason) => restart_after(RESTART_DELAY, &reason),
    };
    log_heap("after HUB75");
    let inbox = net::start(peripherals.modem, sysloop, nvs, &provisioning);

    let mut app = App::new(
        matrix,
        EspClock,
        inbox,
        provisioning.defaults,
        SplitMix64::new(hardware_seed()),
    );
    let mut stats = FrameStats::default();
    let mut next_frame = Instant::now();
    loop {
        let started = Instant::now();
        app.frame();
        stats.record(started.elapsed());

        next_frame += FRAME_PERIOD;
        let now = Instant::now();
        if next_frame <= now {
            // Running late: start the next frame now rather than trying to catch up with a burst.
            next_frame = now;
        }
        std::thread::sleep((next_frame - now).max(MIN_SLEEP));
    }
}

/// Nothing works without the panel; a fresh boot (with an unfragmented heap) is the best retry.
fn restart_after(delay: Duration, reason: &anyhow::Error) -> ! {
    error!("{reason:#}; restarting in {}s", delay.as_secs());
    std::thread::sleep(delay);
    // SAFETY: plain system call; it does not return.
    unsafe { esp_idf_svc::sys::esp_restart() }
}

fn hardware_seed() -> u64 {
    // SAFETY: reads the hardware RNG; true random once the radio is running, pseudo-random before, which
    // is plenty for blink timing.
    let (high, low) = unsafe {
        (
            esp_idf_svc::sys::esp_random(),
            esp_idf_svc::sys::esp_random(),
        )
    };
    u64::from(high) << 32 | u64::from(low)
}

fn log_heap(when: &str) {
    // SAFETY: pure heap queries.
    let free =
        unsafe { esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_INTERNAL) };
    info!(
        "heap {when}: internal free {free} B, largest DMA block {} B",
        matrix::largest_free_dma_block()
    );
}

/// Frame-time telemetry, so a bench flash shows whether the frame budget and RAM hold up.
#[derive(Default)]
struct FrameStats {
    frames: u32,
    total: Duration,
    worst: Duration,
    overruns: u32,
    since: Option<Instant>,
}

impl FrameStats {
    fn record(&mut self, elapsed: Duration) {
        let since = *self.since.get_or_insert_with(Instant::now);
        self.frames += 1;
        self.total += elapsed;
        self.worst = self.worst.max(elapsed);
        self.overruns += u32::from(elapsed > FRAME_PERIOD);
        if since.elapsed() >= STATS_PERIOD {
            info!(
                "{} frames: mean {:.1} ms, worst {:.1} ms, {} over budget",
                self.frames,
                self.total.as_secs_f32() * 1000.0 / self.frames as f32,
                self.worst.as_secs_f32() * 1000.0,
                self.overruns
            );
            log_heap("now");
            // SAFETY: a null handle means the calling task (the render loop).
            let unused =
                unsafe { esp_idf_svc::sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
            info!("render loop stack: {unused} B never used");
            *self = Self::default();
        }
    }
}
