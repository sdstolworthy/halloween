#include "../shaker.h"

#include <cmath>
#include <cstdio>
#include <vector>

static int failures = 0;

static void check(bool ok, const char *name) {
  if (!ok) {
    std::printf("FAIL: %s\n", name);
    failures++;
  }
}

static void test_pick_in_range() {
  struct Case {
    const char *name;
    uint32_t lo, hi, random_value, want;
  };
  const Case cases[] = {
      {"zero random gives lo", 150, 500, 0, 150},
      {"wraps within span", 150, 500, 351, 150},
      {"top of span gives hi", 150, 500, 350, 500},
      {"lo equals hi", 42, 42, 12345, 42},
      {"crossed bounds are swapped", 500, 150, 0, 150},
      {"full uint32 range does not overflow", 0, UINT32_MAX, UINT32_MAX, UINT32_MAX},
  };
  for (const auto &c : cases) check(shaker::pick_in_range(c.lo, c.hi, c.random_value) == c.want, c.name);
}

static void test_elapsed() {
  struct Case {
    const char *name;
    uint32_t now, start, length;
    bool want;
  };
  const Case cases[] = {
      {"before deadline", 1000, 0, 6000, false},
      {"exactly at deadline", 6000, 0, 6000, true},
      {"after deadline", 7000, 0, 6000, true},
      {"across millis wraparound, not yet", 100, UINT32_MAX - 1000, 6000, false},
      {"across millis wraparound, done", 5000, UINT32_MAX - 1000, 6000, true},
  };
  for (const auto &c : cases) check(shaker::elapsed(c.now, c.start, c.length) == c.want, c.name);
}

static void test_degrees_to_level() {
  struct Case {
    const char *name;
    uint32_t degrees;
    float want;
  };
  const Case cases[] = {
      {"0 deg is full negative", 0, -1.0f},
      {"40 deg", 40, -50.0f / 90},
      {"90 deg is centre", 90, 0.0f},
      {"140 deg", 140, 50.0f / 90},
      {"180 deg is full positive", 180, 1.0f},
      {"beyond 180 clamps", 270, 1.0f},
  };
  for (const auto &c : cases) check(std::fabs(shaker::degrees_to_level(c.degrees) - c.want) < 1e-6f, c.name);
}

static void test_plan_move() {
  struct Case {
    const char *name;
    bool go_high;
    std::vector<uint32_t> randoms;  // angle, dwell
    float want_level;
    uint32_t want_dwell_ms;
  };
  // The angle is deterministic (the sweep end for the chosen side); the one draw picks the dwell.
  const Case cases[] = {
      {"low side slams to the low end, zero draw gives shortest dwell", false, {0}, -50.0f / 90, 80},
      {"high side slams to the high end, top draw gives longest dwell", true, {220}, 50.0f / 90, 300},
      {"fixed dwell range is a metronome", true, {12345}, 50.0f / 90, 120},
  };
  for (const auto &c : cases) {
    size_t next = 0;
    const shaker::Range dwell = c.want_dwell_ms == 120 ? shaker::Range{120, 120} : shaker::Range{80, 300};
    auto move = shaker::plan_move({40, 140}, dwell, c.go_high, [&] { return c.randoms[next++]; });
    check(std::fabs(move.level - c.want_level) < 1e-6f && move.dwell_ms == c.want_dwell_ms, c.name);
  }
  // Crossed sweep bounds still slam to the true ends.
  size_t next = 0;
  std::vector<uint32_t> randoms = {0};
  auto move = shaker::plan_move({140, 40}, {80, 300}, false, [&] { return randoms[next++]; });
  check(std::fabs(move.level - (-50.0f / 90)) < 1e-6f, "crossed bounds are swapped");
}

int main() {
  test_pick_in_range();
  test_elapsed();
  test_degrees_to_level();
  test_plan_move();
  if (failures == 0) std::printf("all tests passed\n");
  return failures == 0 ? 0 : 1;
}
