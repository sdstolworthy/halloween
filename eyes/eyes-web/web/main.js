// Browser harness for the eyes emulator. The wasm module is the real eyes-core state machine and
// eyes-render renderer; this file only plays the sensor (a reading every 100 ms, like porch-sensor),
// drives frames from requestAnimationFrame, and draws the controls and diagrams.

const SENSOR_PERIOD_MS = 100;
const WALK_SPEED_M_S = 0.8;
const WALK_END_M = 0.3;
// The porch spec mounts the sensor about 1 m up; only the side diagram uses this.
const SENSOR_HEIGHT_M = 1.0;
const LED_SCALE = 6;
const READOUT_PERIOD_MS = 100;
const SLIDER_MAX_M = 5;
const SPEEDS = [1, 0.5, 0.25, 0.1];
// A background tab can stall requestAnimationFrame for seconds; don't replay that as one giant step.
const MAX_FRAME_GAP_MS = 100;

// Friendly labels and practical slider ranges; the number box still accepts the full validated range.
// "Right" throughout means: standing at the sensor, looking down the walkway towards visitors (the
// visitor's left as they walk up).
const PARAM_UI = {
  eyes_offset_x_m: {
    label: "Panel offset sideways: positive is to the right of the sensor's aim line",
    hint: "Right as seen standing at the sensor looking down the walkway, which is the approaching visitor's left.",
    unit: "m", slider: [-3, 3],
  },
  eyes_offset_y_m: {
    label: "Panel offset along the walkway: negative is behind the sensor, towards the door",
    hint: "Positive is further down the walkway towards the street.",
    unit: "m", slider: [-3, 3],
  },
  eyes_height_m: { label: "Panel centre height", unit: "m", slider: [0, 3] },
  panel_yaw_deg: {
    label: "Panel turned: positive turns its face to the right",
    hint: "0 faces straight down the walkway. Positive turns the panel's face to the right as seen standing at the sensor looking down the walkway (towards +x).",
    unit: "°",
  },
  panel_pitch_deg: { label: "Panel tilted up", unit: "°" },
  face_height_m: { label: "Visitor's face height", unit: "m" },
  pixel_pitch_mm: { label: "LED pitch", unit: "mm" },
  max_gaze_deg: { label: "Gaze angle at the edge of the eye", unit: "°" },
  max_range_m: { label: "Max range (at or beyond means nobody; the sensor's limit is 4.5 m)", unit: "m" },
  stale_timeout_s: { label: "Go idle after no reading for", unit: "s", slider: [0.2, 10] },
  max_evil_s: { label: "Evil mode gives up by itself after", unit: "s", slider: [10, 900] },
  brightness: { label: "LED brightness (device only)", unit: "/255" },
};

const $ = (id) => document.getElementById(id);

async function loadWasm() {
  try {
    const { instance } = await WebAssembly.instantiateStreaming(fetch("eyes_web.wasm"), {});
    return instance.exports;
  } catch {
    const bytes = await (await fetch("eyes_web.wasm")).arrayBuffer();
    const { instance } = await WebAssembly.instantiate(bytes, {});
    return instance.exports;
  }
}

// Seeded so a shared link reproduces the same noisy readings.
function mulberry32(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const decimals = (step) => Math.max(0, -Math.floor(Math.log10(step)));
const round = (value, step) => Number(value.toFixed(decimals(step)));

function createWasmApi(wasm) {
  const text = (ptr, len) => new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, ptr, len));
  const params = Array.from({ length: wasm.param_count() }, (_, index) => ({
    index,
    key: text(wasm.param_key_ptr(index), wasm.param_key_len(index)),
    min: wasm.param_min(index),
    max: wasm.param_max(index),
    step: wasm.param_step(index),
    fallback: wasm.param_default(index),
  }));
  const byKey = Object.fromEntries(params.map((param) => [param.key, param]));
  return {
    params,
    value: (key) => round(wasm.param_value(byKey[key].index), byKey[key].step),
    set: (key, value) => wasm.set_param(byKey[key].index, value) === 1,
    message: () => text(wasm.message_ptr(), wasm.message_len()),
    applyJson(json) {
      const bytes = new TextEncoder().encode(json);
      if (bytes.length > wasm.input_capacity()) return false;
      new Uint8Array(wasm.memory.buffer, wasm.input_ptr(), bytes.length).set(bytes);
      return wasm.apply_config_json(bytes.length) === 1;
    },
    eye: (eye) => [0, 1, 2].map((axis) => wasm.eye_position(eye, axis)),
    normal: () => [0, 1, 2].map((axis) => wasm.panel_normal(axis)),
    angles: (eye, distance) => [wasm.gaze_yaw_deg(eye, distance), wasm.gaze_pitch_deg(eye, distance)],
  };
}

function readUrlState(api) {
  const query = new URLSearchParams(location.search);
  for (const param of api.params) {
    if (query.has(param.key)) api.set(param.key, Number(query.get(param.key)));
  }
  const number = (name, fallback) => (query.has(name) && Number.isFinite(Number(query.get(name))) ? Number(query.get(name)) : fallback);
  return {
    distance: Math.max(0, number("d", 1.8)),
    nobody: query.get("nobody") === "1",
    online: query.get("online") !== "0",
    noise: Math.max(0, number("noise", 0.03)),
    seed: Math.trunc(number("seed", 1)) >>> 0,
    gamma: query.get("gamma") !== "0",
    speed: SPEEDS.includes(number("speed", 1)) ? number("speed", 1) : 1,
    look: query.get("look") === "evil" ? "evil" : "normal",
  };
}

// Every key, not just the ones changed here: the device merges over its *provisioned* defaults, which
// may differ from the built-in ones the emulator starts from.
function fullConfig(api) {
  return Object.fromEntries(api.params.map((param) => [param.key, api.value(param.key)]));
}

function nonDefaultConfig(api) {
  return Object.fromEntries(
    api.params
      .map((param) => [param.key, api.value(param.key), round(param.fallback, param.step)])
      .filter(([, value, fallback]) => value !== fallback)
      .map(([key, value]) => [key, value]),
  );
}

function writeUrlState(api, state) {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(nonDefaultConfig(api))) query.set(key, String(value));
  query.set("d", state.distance.toFixed(2));
  if (state.nobody) query.set("nobody", "1");
  if (!state.online) query.set("online", "0");
  query.set("noise", String(state.noise));
  query.set("seed", String(state.seed));
  if (!state.gamma) query.set("gamma", "0");
  if (state.speed !== 1) query.set("speed", String(state.speed));
  if (state.look === "evil") query.set("look", "evil");
  history.replaceState(null, "", `${location.pathname}?${query}`);
}

function createPanelPainter(wasm, canvas, state) {
  const width = wasm.width();
  const height = wasm.height();
  const source = document.createElement("canvas");
  source.width = width;
  source.height = height;
  const sourceContext = source.getContext("2d");
  const image = sourceContext.createImageData(width, height);

  canvas.width = width * LED_SCALE;
  canvas.height = height * LED_SCALE;
  const context = canvas.getContext("2d");
  context.imageSmoothingEnabled = false;

  // Dark gaps between LEDs, as on the real panel up close.
  const tile = document.createElement("canvas");
  tile.width = tile.height = LED_SCALE;
  const tileContext = tile.getContext("2d");
  tileContext.fillStyle = "#050409";
  tileContext.fillRect(0, 0, LED_SCALE, LED_SCALE);
  tileContext.clearRect(0, 0, LED_SCALE - 1, LED_SCALE - 1);
  const gaps = context.createPattern(tile, "repeat");

  const lut = new Uint8Array(wasm.memory.buffer, wasm.panel_display_lut_ptr(), 256).slice();
  return () => {
    image.data.set(new Uint8Array(wasm.memory.buffer, wasm.framebuffer_ptr(), wasm.framebuffer_len()));
    if (state.gamma) {
      // Display stage only: the renderer's output (and the golden hashes) stay untouched.
      const data = image.data;
      for (let i = 0; i < data.length; i += 4) {
        data[i] = lut[data[i]];
        data[i + 1] = lut[data[i + 1]];
        data[i + 2] = lut[data[i + 2]];
      }
    }
    sourceContext.putImageData(image, 0, 0);
    context.drawImage(source, 0, 0, canvas.width, canvas.height);
    context.fillStyle = gaps;
    context.fillRect(0, 0, canvas.width, canvas.height);
  };
}

// Maps world metres onto an SVG viewBox with a uniform scale. `flip` puts larger values at the top.
function projection(box, bounds) {
  const scale = Math.min(box.width / (bounds.maxU - bounds.minU), box.height / (bounds.maxV - bounds.minV));
  const offsetU = (box.width - (bounds.maxU - bounds.minU) * scale) / 2;
  const offsetV = (box.height - (bounds.maxV - bounds.minV) * scale) / 2;
  return {
    scale,
    toSvg: (u, v) => [offsetU + (u - bounds.minU) * scale, box.height - offsetV - (v - bounds.minV) * scale],
    fromSvgV: (y) => (box.height - offsetV - y) / scale + bounds.minV,
  };
}

const line = (a, b, cls) => `<line class="${cls}" x1="${a[0]}" y1="${a[1]}" x2="${b[0]}" y2="${b[1]}"/>`;
const ARROW = `<defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="#f0a63a"/></marker></defs>`;

function drawTopView(svg, api, state, present) {
  const range = Math.max(api.value("max_range_m"), 4.5) + 0.3;
  const eyes = [api.eye(0), api.eye(1)];
  const centre = [api.value("eyes_offset_x_m"), api.value("eyes_offset_y_m")];
  const reach = Math.max(1.2, Math.abs(centre[0]) + 0.6);
  const view = projection({ width: 320, height: 320 }, {
    minU: -reach, maxU: reach, minV: Math.min(-0.6, centre[1] - 0.6), maxV: Math.max(range, centre[1] + 0.6),
  });
  const normal = api.normal();
  const p = (x, y) => view.toSvg(x, y);
  const visitor = p(0, Math.min(state.distance, range));
  const panelHalf = (64 * api.value("pixel_pitch_mm")) / 1000;
  const right = [-normal[1], normal[0]];
  const [walkLeft, walkTop] = p(-0.5, range);
  const [walkRight, walkBottom] = p(0.5, -0.6);

  svg.innerHTML = [
    ARROW,
    `<rect class="walkway" x="${walkLeft}" y="${walkTop}" width="${walkRight - walkLeft}" height="${walkBottom - walkTop}"/>`,
    line(p(0, 0), p(0, range), "axis"),
    `<rect class="sensor" x="${p(0, 0)[0] - 6}" y="${p(0, 0)[1] - 4}" width="12" height="8"/>`,
    `<text x="${p(0, 0)[0] + 10}" y="${p(0, 0)[1] + 4}">sensor</text>`,
    line(p(centre[0] - right[0] * panelHalf * 1.6, centre[1] - right[1] * panelHalf * 1.6),
      p(centre[0] + right[0] * panelHalf * 1.6, centre[1] + right[1] * panelHalf * 1.6), "panel"),
    line(p(...centre), p(centre[0] + normal[0] * 0.6, centre[1] + normal[1] * 0.6), "normal"),
    ...(present ? eyes.map((eye) => line(p(eye[0], eye[1]), visitor, "gaze")) : []),
    `<circle id="visitor" class="visitor${present ? "" : " absent"}" cx="${visitor[0]}" cy="${visitor[1]}" r="9"/>`,
    `<text x="${visitor[0] + 13}" y="${visitor[1] + 4}">${state.distance.toFixed(2)} m</text>`,
    `<text x="160" y="14" text-anchor="middle">street end (visitors come from here)</text>`,
    `<text x="160" y="312" text-anchor="middle">door / house</text>`,
    `<text x="312" y="160" text-anchor="end">+x →</text>`,
    `<text x="8" y="160">← −x</text>`,
  ].join("");
  return view;
}

function drawSideView(svg, api, state, present) {
  const range = Math.max(api.value("max_range_m"), 4.5) + 0.3;
  const eyes = [api.eye(0), api.eye(1)];
  const centre = [api.value("eyes_offset_y_m"), api.value("eyes_height_m")];
  const face = api.value("face_height_m");
  const bounds = {
    minU: Math.min(-0.6, centre[0] - 0.6), maxU: Math.max(range, centre[0] + 0.6),
    minV: -0.1, maxV: Math.max(2.2, centre[1] + 0.4, face + 0.3),
  };
  const view = projection({ width: 320, height: 200 }, bounds);
  const p = (y, z) => view.toSvg(y, z);
  const normal = api.normal();
  const visitorY = Math.min(state.distance, range);
  const sensor = p(0, SENSOR_HEIGHT_M);

  svg.innerHTML = [
    ARROW,
    line(p(bounds.minU, 0), p(bounds.maxU, 0), "ground"),
    line(p(0, 0), p(0, SENSOR_HEIGHT_M), "axis"),
    `<rect class="sensor" x="${sensor[0] - 4}" y="${sensor[1] - 4}" width="8" height="8"/>`,
    line(p(centre[0], centre[1] - 0.1), p(centre[0], centre[1] + 0.1), "panel"),
    line(p(...centre), p(centre[0] + normal[1] * 0.6, centre[1] + normal[2] * 0.6), "normal"),
    ...(present ? eyes.map((eye) => line(p(eye[1], eye[2]), p(visitorY, face), "gaze")) : []),
    line(p(visitorY, 0), p(visitorY, face - 0.12), present ? "body" : "axis"),
    `<circle class="visitor${present ? "" : " absent"}" cx="${p(visitorY, face)[0]}" cy="${p(visitorY, face)[1]}" r="7"/>`,
    `<text x="${p(centre[0], centre[1])[0] + 8}" y="${p(centre[0], centre[1])[1] - 8}">eyes ${api.value("eyes_height_m").toFixed(2)} m</text>`,
  ].join("");
}

function buildParamControls(api, onChange) {
  const container = $("params");
  const inputs = {};
  for (const param of api.params) {
    const ui = PARAM_UI[param.key] ?? { label: param.key, unit: "" };
    const [sliderMin, sliderMax] = ui.slider ?? [param.min, param.max];
    const field = document.createElement("label");
    field.className = "field";
    if (ui.hint) field.title = ui.hint;
    field.innerHTML = `<span>${ui.label}</span><span class="pair">
      <input type="range" min="${sliderMin}" max="${sliderMax}" step="${param.step}">
      <input type="number" min="${param.min}" max="${param.max}" step="${param.step}" aria-label="${param.key}">
      <span class="unit">${ui.unit}</span></span>${ui.hint ? `<small class="hint">${ui.hint}</small>` : ""}`;
    const [range, number] = field.querySelectorAll("input");
    const commit = (value) => {
      if (!api.set(param.key, Number(value))) $("json-status").textContent = api.message();
      onChange();
    };
    range.addEventListener("input", () => commit(range.value));
    number.addEventListener("change", () => commit(number.value));
    inputs[param.key] = [range, number];
    container.appendChild(field);
  }
  return () => {
    for (const [key, [range, number]] of Object.entries(inputs)) {
      range.value = number.value = api.value(key);
    }
  };
}

async function main() {
  const wasm = await loadWasm();
  const api = createWasmApi(wasm);
  const state = readUrlState(api);
  let random = mulberry32(state.seed);
  let walk = null;
  // Simulated time fed to the core: real time scaled by the speed control, so slow motion slows the eyes
  // themselves rather than the display.
  let simNow = 0;
  let lastRafMs = null;
  // When a simulated scare ends (simulated ms), or null.
  let showEndsAt = null;
  const SHOW_LENGTH_MS = 10_000;
  const publishLook = (look) => {
    state.look = look;
    const accepted = wasm.publish_mode(look === "evil" ? 1 : 0, simNow) === 1;
    const status = $("look-status");
    status.textContent = accepted ? "" : `The device ignores this: ${api.message()}`;
    status.className = `status ${accepted ? "" : "error"}`;
    changed();
  };
  let lastSensorMs = -Infinity;
  let lastReadoutMs = -Infinity;
  let topView = null;

  const paint = createPanelPainter(wasm, $("panel"), state);
  const syncConfigJson = () => {
    $("config-json").textContent = JSON.stringify(fullConfig(api), null, 2);
  };
  const syncVisitorControls = () => {
    $("distance").max = SLIDER_MAX_M;
    $("gamma").checked = state.gamma;
    $("speed").value = String(state.speed);
    for (const radio of document.querySelectorAll('input[name="look"]')) radio.checked = radio.value === state.look;
    $("distance").value = $("distance-number").value = state.distance.toFixed(2);
    $("nobody").checked = state.nobody;
    $("online").checked = state.online;
    $("noise").value = state.noise;
    $("noise-value").textContent = `±${Math.round(state.noise * 100)} cm`;
  };
  const changed = () => {
    syncParams();
    syncVisitorControls();
    syncConfigJson();
    writeUrlState(api, state);
    lastReadoutMs = -Infinity;
  };
  const syncParams = buildParamControls(api, changed);

  const setDistance = (metres) => {
    state.distance = Math.max(0, Math.min(10, metres));
    state.nobody = false;
    changed();
  };
  $("distance").addEventListener("input", (event) => { walk = null; setDistance(Number(event.target.value)); });
  $("distance-number").addEventListener("change", (event) => { walk = null; setDistance(Number(event.target.value)); });
  $("nobody").addEventListener("change", (event) => { walk = null; state.nobody = event.target.checked; changed(); });
  $("online").addEventListener("change", (event) => { state.online = event.target.checked; changed(); });
  for (const radio of document.querySelectorAll('input[name="look"]')) {
    radio.addEventListener("change", () => { showEndsAt = null; publishLook(radio.value); });
  }
  $("show").addEventListener("click", () => {
    showEndsAt = simNow + SHOW_LENGTH_MS;
    publishLook("evil");
  });
  $("speed").addEventListener("change", (event) => { state.speed = Number(event.target.value); changed(); });
  $("gamma").addEventListener("change", (event) => { state.gamma = event.target.checked; changed(); });
  $("noise").addEventListener("input", (event) => { state.noise = Number(event.target.value); changed(); });
  $("walk").addEventListener("click", () => {
    walk = { startMs: simNow, from: api.value("max_range_m") + 0.3 };
    state.online = true;
  });
  $("reseed").addEventListener("click", () => {
    state.seed = (Math.random() * 2 ** 32) >>> 0;
    random = mulberry32(state.seed);
    wasm.reset(state.seed, simNow);
    if (state.look === "evil") wasm.publish_mode(1, simNow);
    changed();
  });
  $("defaults").addEventListener("click", () => { api.applyJson("{}"); changed(); });
  $("copy-json").addEventListener("click", () => navigator.clipboard?.writeText($("config-json").textContent));
  $("share").addEventListener("click", async () => {
    await navigator.clipboard?.writeText(location.href);
    $("share").textContent = "Link copied";
    setTimeout(() => { $("share").textContent = "Copy link to this setup"; }, 1500);
  });
  $("apply-json").addEventListener("click", () => {
    const applied = api.applyJson($("json-input").value);
    const status = $("json-status");
    status.textContent = applied ? "Applied. The device would accept this payload." : `Rejected: ${api.message()}`;
    status.className = `status ${applied ? "ok" : "error"}`;
    changed();
  });

  const svg = $("top-view");
  svg.addEventListener("pointerdown", (event) => {
    if (event.target.id !== "visitor") return;
    svg.setPointerCapture(event.pointerId);
    walk = null;
  });
  svg.addEventListener("pointermove", (event) => {
    if (!svg.hasPointerCapture(event.pointerId) || !topView) return;
    const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(svg.getScreenCTM().inverse());
    setDistance(topView.fromSvgV(point.y));
  });

  const sensorPayload = () =>
    new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, wasm.sensor_payload_ptr(), wasm.sensor_payload_len()));

  function updateReadout() {
    const tracking = wasm.mode() === 1;
    const present = !state.nobody && state.distance < Math.min(api.value("max_range_m"), 4.5);
    let mode = `Watching a visitor ${state.distance.toFixed(2)} m from the sensor`;
    if (!tracking) {
      if (!state.online) mode = "Idle: the sensor has gone quiet";
      else if (!present) mode = "Idle: nobody within range";
      else mode = "Idle";
    }
    $("mode").textContent = mode;
    $("payload").textContent = state.online ? sensorPayload() : "nothing (sensor offline)";
    $("lid").textContent = `${Math.round(wasm.lid() * 100)}%`;
    $("motion").textContent = wasm.gaze_moving() ? "moving" : "settled";
    const lookName = (evil) => (evil ? "evil" : "normal");
    const drawn = lookName(wasm.look() === 1);
    const requested = lookName(wasm.requested_look() === 1);
    $("look").textContent = drawn === requested ? drawn : `${drawn}, switching to ${requested} at the next blink`;
    if (showEndsAt !== null) $("look").textContent += ` (scare ends in ${Math.ceil((showEndsAt - simNow) / 1000)} s)`;
    $("mode").classList.toggle("tracking", tracking);
    for (const eye of [0, 1]) {
      const [yaw, pitch] = api.angles(eye, state.distance);
      $(`yaw-${eye}`).textContent = present ? `${yaw.toFixed(1)}°` : "–";
      $(`pitch-${eye}`).textContent = present ? `${pitch.toFixed(1)}°` : "–";
    }
    topView = drawTopView(svg, api, state, present);
    drawSideView($("side-view"), api, state, present);
  }

  function frame(rafNow) {
    const gap = lastRafMs === null ? 0 : Math.min(rafNow - lastRafMs, MAX_FRAME_GAP_MS);
    lastRafMs = rafNow;
    simNow += gap * state.speed;
    const now = simNow;
    if (showEndsAt !== null && now >= showEndsAt) {
      showEndsAt = null;
      publishLook("normal");
    }
    if (walk) {
      const distance = walk.from - (WALK_SPEED_M_S * (now - walk.startMs)) / 1000;
      state.distance = Math.max(WALK_END_M, distance);
      state.nobody = false;
      if (distance <= WALK_END_M) {
        walk = null;
        changed();
      } else {
        syncVisitorControls();
      }
    }
    if (state.online && now - lastSensorMs >= SENSOR_PERIOD_MS) {
      lastSensorMs = now;
      // Formatted and parsed exactly as porch-sensor and the device do; NaN means no echo.
      const noisy = state.distance + (random() * 2 - 1) * state.noise;
      wasm.sensor_reading(state.nobody ? NaN : Math.max(0, noisy), now);
    }
    wasm.frame(now);
    paint();
    if (rafNow - lastReadoutMs >= READOUT_PERIOD_MS) {
      lastReadoutMs = rafNow;
      updateReadout();
    }
    requestAnimationFrame(frame);
  }

  wasm.reset(state.seed, simNow);
  if (state.look === "evil") wasm.publish_mode(1, simNow);
  changed();
  requestAnimationFrame(frame);
}

main().catch((error) => {
  console.error(error);
  $("mode").textContent = "The renderer failed to load. Check the browser console.";
});
