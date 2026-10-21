// Headless proof that the wasm build works and renders exactly what the host renders.
// Run: node web/smoke.mjs [path/to/eyes_web.wasm]
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// Keep in sync with scripted_runs_match_the_smoke_test_hashes in eyes-web/src/lib.rs: one frame mid-way
// through the coupled acquisition blink (pins blink timing), one settled.
const MID_BLINK_MS = 297;
const MID_BLINK_HASH = 0x2f020ba7;
const SETTLED_HASH = 0xf99b39d7;
// Keep in sync with the_evil_scripted_run_matches_the_smoke_test_hash: the same run with the house turned
// evil at 990 ms, hashed at the end.
const EVIL_FROM_MS = 990;
const EVIL_HASH = 0xbc769620;

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
  "frame", "mode", "lid", "gaze_moving", "publish_mode", "look", "requested_look", "gaze_yaw_deg",
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
const frameHash = () => {
  const frame = new Uint8Array(wasm.memory.buffer, wasm.framebuffer_ptr(), wasm.framebuffer_len());
  let hash = 0x811c9dc5;
  for (const byte of frame) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  return hash;
};
let midBlinkHash = null;
wasm.reset(1, 0);
for (let now = 0; now < 3000;) {
  if (now % 100 < 33) wasm.sensor_reading(1.2, now);
  now += 33;
  wasm.frame(now);
  if (now === MID_BLINK_MS) midBlinkHash = frameHash();
}
const settledHash = frameHash();

wasm.reset(1, 0);
for (let now = 0; now < 3000;) {
  if (now === EVIL_FROM_MS) wasm.publish_mode(1, now);
  if (now % 100 < 33) wasm.sensor_reading(1.2, now);
  now += 33;
  wasm.frame(now);
}
const evilHash = frameHash();
check(wasm.look() === 1, "the evil run should have switched to evil");
check(evilHash === EVIL_HASH, `evil frame hash 0x${evilHash.toString(16)} != 0x${EVIL_HASH.toString(16)}`);
wasm.publish_mode(0, 3100);
const hex = (value) => `0x${(value ?? 0).toString(16)}`;
check(midBlinkHash === MID_BLINK_HASH, `mid-blink frame hash ${hex(midBlinkHash)} != ${hex(MID_BLINK_HASH)}`);
check(settledHash === SETTLED_HASH, `settled frame hash ${hex(settledHash)} != ${hex(SETTLED_HASH)}`);
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
console.log(`PASS: ${required.length} exports, ${keys.length} params, scripted frame hashes match host (mid-blink, settled and evil)`);
