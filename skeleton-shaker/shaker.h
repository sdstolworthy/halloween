#pragma once

#include <cstdint>
#include <utility>

// Pure dance logic, kept free of ESPHome types so it can be unit-tested on the host (see tests/).
namespace shaker {

struct Range {
  uint32_t lo;
  uint32_t hi;
};

struct Burst {
  bool forward;
  float duty;  // 0..1
  uint32_t on_ms;
  uint32_t pause_ms;
};

// Inclusive. Tolerates lo > hi because the min/max tuning sliders are independent and can be crossed.
inline uint32_t pick_in_range(uint32_t lo, uint32_t hi, uint32_t random_value) {
  if (lo > hi) std::swap(lo, hi);
  const uint64_t span = uint64_t(hi) - lo + 1;
  return lo + uint32_t(random_value % span);
}

inline uint32_t pick_in_range(Range range, uint32_t random_value) {
  return pick_in_range(range.lo, range.hi, random_value);
}

// Unsigned subtraction keeps this correct across the 49-day millis() wraparound.
inline bool elapsed(uint32_t now_ms, uint32_t start_ms, uint32_t length_ms) {
  return now_ms - start_ms >= length_ms;
}

template <typename RandomFn>
Burst plan_burst(Range on_ms, Range pause_ms, Range duty_pct, RandomFn next_random) {
  // Braced initialization guarantees left-to-right evaluation, so the random draws are in a fixed order.
  return Burst{
      (next_random() & 1u) != 0,
      pick_in_range(duty_pct, next_random()) / 100.0f,
      pick_in_range(on_ms, next_random()),
      pick_in_range(pause_ms, next_random()),
  };
}

}  // namespace shaker
