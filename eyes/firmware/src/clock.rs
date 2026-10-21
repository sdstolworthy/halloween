use esp_idf_svc::sys::esp_timer_get_time;
use eyes_hal::Clock;

/// Microseconds since boot from `esp_timer`: monotonic and running before any network time sync.
pub struct EspClock;

impl Clock for EspClock {
    fn uptime_ms(&self) -> u64 {
        // SAFETY: a pure read of the monotonic timer.
        let micros = unsafe { esp_timer_get_time() };
        micros.max(0) as u64 / 1000
    }
}
