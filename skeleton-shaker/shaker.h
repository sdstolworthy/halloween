#pragma once

#include <cstdint>
#include <utility>

// Pure dance logic, kept free of ESPHome types so it can be unit-tested on the host (see tests/).
namespace shaker {

struct Range {
  uint32_t lo;
  uint32_t hi;
};

struct Move {
  float level;  // servo level, -1..1
  uint32_t dwell_ms;
};

// Inclusive. Tolerates lo > hi so a swapped pair of substitutions still works.
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

// ESPHome's servo.write takes -1..1 for its full 0..180 deg range.
inline float degrees_to_level(uint32_t degrees) {
  if (degrees > 180) degrees = 180;
  return (float(degrees) - 90.0f) / 90.0f;
}

// Each move slams exactly to one end of the sweep, alternating sides: full deflection on every
// stroke. Only the dwell is randomized (and a fixed dwell range makes it a metronome).
template <typename RandomFn>
Move plan_move(Range degrees, Range dwell_ms, bool go_high, RandomFn next_random) {
  if (degrees.lo > degrees.hi) std::swap(degrees.lo, degrees.hi);
  return Move{
      degrees_to_level(go_high ? degrees.hi : degrees.lo),
      pick_in_range(dwell_ms, next_random()),
  };
}

}  // namespace shaker
