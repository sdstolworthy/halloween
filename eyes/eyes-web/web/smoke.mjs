// Headless proof that the wasm build works and renders exactly what the host renders.
// Run: node web/smoke.mjs [path/to/eyes_web.wasm]
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// Keep in sync with scripted_run_matches_the_smoke_test_hash in eyes-web/src/lib.rs.
const SCRIPTED_HASH = 0xf99b39d7;

const dir = dirname(fileURLToPath(import.meta.url));
const wasmPath = process.argv[2] ?? join(dir, "eyes_web.wasm");
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});
const wasm = instance.exports;
const failures = [];
const check = (ok, message) => { if (!ok) failures.push(message); };

const required = [
  "memory", "width", "height", "framebuffer_ptr", "framebuffer_len", "reset", "param_count",
  "param_key_ptr", "param_key_len", "param_min", "param_max", "param_step", "param_default",
  "param_value", "set_param", "input_ptr", "input_capacity", "apply_config_json", "message_ptr",
  "message_len", "sensor_reading", "sensor_payload_ptr", "sensor_payload_len", "panel_display_lut_ptr",
  "frame", "mode", "lid", "gaze_moving", "gaze_yaw_deg",
  "gaze_pitch_deg", "eye_position", "panel_normal",
];
const missing = required.filter((name) => !(name in wasm));
if (missing.length) {
  console.error(`FAIL: missing exports: ${missing.join(", ")}`);
  process.exit(1);
}

const text = (ptr, len) => new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, ptr, len));
const keys = Array.from({ length: wasm.param_count() }, (_, i) => text(wasm.param_key_ptr(i), wasm.param_key_len(i)));
check(keys.includes("eyes_offset_x_m") && keys.includes("brightness"), `unexpected param keys ${keys}`);
check(wasm.width() === 128 && wasm.height() === 64, "panel is not 128x64");
check(wasm.framebuffer_len() === 128 * 64 * 4, "framebuffer is not 128x64 RGBA");

// Same script as the host test: seed 1, visitor at 1.2 m every 100 ms, frames every 33 ms for 3 s.
wasm.reset(1, 0);
for (let now = 0; now < 3000;) {
  if (now % 100 < 33) wasm.sensor_reading(1.2, now);
  now += 33;
  wasm.frame(now);
}
const frame = new Uint8Array(wasm.memory.buffer, wasm.framebuffer_ptr(), wasm.framebuffer_len());
let hash = 0x811c9dc5;
for (const byte of frame) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
check(hash === SCRIPTED_HASH, `scripted frame hash 0x${hash.toString(16)} != 0x${SCRIPTED_HASH.toString(16)}`);
check(wasm.mode() === 1, "scripted run should be tracking");

wasm.sensor_reading(NaN, 3100);
check(text(wasm.sensor_payload_ptr(), wasm.sensor_payload_len()) === "none", "no echo should send none");
wasm.sensor_reading(4.6, 3200);
check(text(wasm.sensor_payload_ptr(), wasm.sensor_payload_len()) === "none", "beyond 4.5 m should send none");

const maxGaze = keys.indexOf("max_gaze_deg");
check(wasm.set_param(maxGaze, 120) === 0, "max_gaze_deg 120 should be rejected");
check(text(wasm.message_ptr(), wasm.message_len()).includes("max_gaze_deg"), "rejection should name the key");

const writeInput = (json) => {
  const bytes = new TextEncoder().encode(json);
  new Uint8Array(wasm.memory.buffer, wasm.input_ptr(), bytes.length).set(bytes);
  return bytes.length;
};
check(wasm.apply_config_json(writeInput('{"eyes_height_m": 2.2}')) === 1, "valid JSON patch rejected");
check(Math.abs(wasm.param_value(keys.indexOf("eyes_height_m")) - 2.2) < 1e-6, "JSON patch not applied");
check(wasm.apply_config_json(writeInput('{"nope": 1}')) === 0, "unknown key accepted");

if (failures.length) {
  failures.forEach((failure) => console.error(`FAIL: ${failure}`));
  process.exit(1);
}
console.log(`PASS: ${required.length} exports, ${keys.length} params, scripted frame hash 0x${hash.toString(16)} matches host`);
