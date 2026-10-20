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

static void test_plan_burst() {
  struct Case {
    const char *name;
    std::vector<uint32_t> randoms;  // direction, duty, on, pause
    shaker::Burst want;
  };
  const Case cases[] = {
      {"odd draw is forward, zero draws give minimums", {1, 0, 0, 0}, {true, 0.50f, 150, 50}},
      {"even draw is reverse, top draws give maximums", {2, 50, 350, 150}, {false, 1.00f, 500, 200}},
  };
  for (const auto &c : cases) {
    size_t next = 0;
    auto burst = shaker::plan_burst({150, 500}, {50, 200}, {50, 100}, [&] { return c.randoms[next++]; });
    check(burst.forward == c.want.forward && std::fabs(burst.duty - c.want.duty) < 1e-6f &&
              burst.on_ms == c.want.on_ms && burst.pause_ms == c.want.pause_ms,
          c.name);
  }
}

int main() {
  test_pick_in_range();
  test_elapsed();
  test_plan_burst();
  if (failures == 0) std::printf("all tests passed\n");
  return failures == 0 ? 0 : 1;
}
