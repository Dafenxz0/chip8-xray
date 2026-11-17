const DISPLAY_W = 64;
const DISPLAY_H = 32;
const RAM_SIZE = 4096;

// Classic COSMAC VIP 4x4 layout mapped to PC keyboard.
const KEYPAD_LAYOUT = [
  { hex: 0x1, label: "1", hint: "1" },
  { hex: 0x2, label: "2", hint: "2" },
  { hex: 0x3, label: "3", hint: "3" },
  { hex: 0xC, label: "C", hint: "4" },
  { hex: 0x4, label: "4", hint: "Q" },
  { hex: 0x5, label: "5", hint: "W" },
  { hex: 0x6, label: "6", hint: "E" },
  { hex: 0xD, label: "D", hint: "R" },
  { hex: 0x7, label: "7", hint: "A" },
  { hex: 0x8, label: "8", hint: "S" },
  { hex: 0x9, label: "9", hint: "D" },
  { hex: 0xE, label: "E", hint: "F" },
  { hex: 0xA, label: "A", hint: "Z" },
  { hex: 0x0, label: "0", hint: "X" },
  { hex: 0xB, label: "B", hint: "C" },
  { hex: 0xF, label: "F", hint: "V" },
];

const KEY_BINDINGS = new Map([
  ["1", 0x1], ["2", 0x2], ["3", 0x3], ["4", 0xC],
  ["q", 0x4], ["w", 0x5], ["e", 0x6], ["r", 0xD],
  ["a", 0x7], ["s", 0x8], ["d", 0x9], ["f", 0xE],
  ["z", 0xA], ["x", 0x0], ["c", 0xB], ["v", 0xF],
  ["ArrowUp", 0x1],
  ["ArrowDown", 0x4],
  ["ArrowLeft", 0x7],
  ["ArrowRight", 0x9],
]);

const decoder = new TextDecoder("utf-8");

async function loadWasm() {
  const url = "./chip8_xray.wasm";
  if (WebAssembly.instantiateStreaming) {
    try {
      const resp = await fetch(url);
      if (!resp.ok) throw new Error(`HTTP ${resp.status} loading ${url}`);
      const contentType = resp.headers.get("Content-Type") || "";
      if (contentType.includes("application/wasm")) {
        return (await WebAssembly.instantiateStreaming(resp, {})).instance.exports;
      }
      const bytes = await resp.arrayBuffer();
      return (await WebAssembly.instantiate(bytes, {})).instance.exports;
    } catch (err) {
      throw new Error(`Failed to instantiate WASM: ${err.message}`);
    }
  }
  const resp = await fetch(url);
  if (!resp.ok) throw new Error(`HTTP ${resp.status} loading ${url}`);
  const bytes = await resp.arrayBuffer();
  return (await WebAssembly.instantiate(bytes, {})).instance.exports;
}

function heatToRgb(heat) {
  if (heat === 0) return [20, 28, 39];
  if (heat < 70) return [30, 82, 110];
  if (heat < 150) return [45, 212, 191];
  if (heat < 220) return [245, 158, 11];
  return [251, 113, 133];
}

function ramRegion(addr) {
  if (addr >= 0x050 && addr < 0x0A0) return "Fontset";
  if (addr < 0x200) return "System";
  return "Program/Data";
}

function createBeepController() {
  let ctx = null;
  let osc = null;
  let gain = null;
  let enabled = false;

  function ensureContext() {
    if (!ctx) {
      const AudioCtx = window.AudioContext || window.webkitAudioContext;
      if (!AudioCtx) return false;
      ctx = new AudioCtx();
      osc = ctx.createOscillator();
      gain = ctx.createGain();
      osc.type = "square";
      osc.frequency.value = 440;
      gain.gain.value = 0;
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start();
    }
    if (ctx.state === "suspended") {
      ctx.resume().catch(() => {});
    }
    return true;
  }

  return {
    toggle() {
      enabled = !enabled;
      if (enabled) {
        ensureContext();
      } else if (gain && ctx) {
        gain.gain.setTargetAtTime(0, ctx.currentTime, 0.01);
      }
      return enabled;
    },
    update(soundTimer) {
      if (!enabled || !gain || !ctx) return;
      const target = soundTimer > 0 ? 0.04 : 0;
      gain.gain.setTargetAtTime(target, ctx.currentTime, 0.008);
    },
  };
}

async function main() {
  const errBanner = document.getElementById("wasm-error");
  let wasm;
  try {
    wasm = await loadWasm();
  } catch (err) {
    errBanner.hidden = false;
    errBanner.textContent = `${err.message}. Serve web/ over HTTP (or view via GitHub Pages).`;
    return;
  }

  const screenCanvas = document.getElementById("screen-canvas");
  const screenCtx = screenCanvas.getContext("2d");
  const screenImage = screenCtx.createImageData(DISPLAY_W, DISPLAY_H);
  const screenWrap = screenCanvas.parentElement;

  const heatmapCanvas = document.getElementById("heatmap-canvas");
  const heatmapCtx = heatmapCanvas.getContext("2d");
  const heatmapImage = heatmapCtx.createImageData(64, 64);

  const romSelect = document.getElementById("rom-select");
  const btnToggle = document.getElementById("btn-toggle");
  const btnStep = document.getElementById("btn-step");
  const btnReset = document.getElementById("btn-reset");
  const btnSound = document.getElementById("btn-sound");
  const speedSlider = document.getElementById("speed-slider");
  const speedValue = document.getElementById("speed-value");
  const crtToggle = document.getElementById("crt-toggle");
  const statusBadge = document.getElementById("status-badge");
  const cycleReadout = document.getElementById("cycle-readout");
  const opcodeReadout = document.getElementById("opcode-readout");
  const heatHover = document.getElementById("heat-hover");

  const regPc = document.getElementById("reg-pc");
  const regI = document.getElementById("reg-i");
  const regSp = document.getElementById("reg-sp");
  const regDt = document.getElementById("reg-dt");
  const regSt = document.getElementById("reg-st");
  const stackReadout = document.getElementById("stack-readout");
  const vRegsGrid = document.getElementById("v-regs-grid");
  const disasmFeed = document.getElementById("disasm-feed");
  const keypadGrid = document.getElementById("keypad-grid");

  // Build V0..VF DOM cells once.
  const vCells = [];
  const prevRegs = new Uint8Array(16);
  for (let i = 0; i < 16; i++) {
    const cell = document.createElement("div");
    cell.className = i === 0xF ? "v-cell vf" : "v-cell";
    const name = document.createElement("span");
    name.className = "reg-name";
    name.textContent = `V${i.toString(16).toUpperCase()}`;
    const val = document.createElement("span");
    val.textContent = "00";
    cell.append(name, val);
    vRegsGrid.appendChild(cell);
    vCells.push({ cell, val });
  }

  // Build 16-key interactive keypad.
  const keyButtons = new Map();
  for (const item of KEYPAD_LAYOUT) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "key-btn";
    btn.innerHTML = `<span>${item.label}</span><span class="key-hint">${item.hint}</span>`;
    const press = () => {
      wasm.wasm_set_key(item.hex, 1);
      btn.classList.add("active");
    };
    const release = () => {
      wasm.wasm_set_key(item.hex, 0);
      btn.classList.remove("active");
    };
    btn.addEventListener("pointerdown", press);
    btn.addEventListener("pointerup", release);
    btn.addEventListener("pointerleave", release);
    btn.addEventListener("pointercancel", release);
    keypadGrid.appendChild(btn);
    keyButtons.set(item.hex, btn);
  }

  let paused = false;
  let cyclesPerFrame = Number(speedSlider.value);
  const audio = createBeepController();

  function setPaused(next) {
    paused = next;
    btnToggle.textContent = paused ? "Resume" : "Pause";
    statusBadge.textContent = paused ? "PAUSED" : "RUNNING";
    statusBadge.className = paused ? "badge badge-paused" : "badge badge-run";
  }

  function switchRom(id) {
    romSelect.value = String(id);
    wasm.wasm_init(id);
    prevRegs.fill(0);
    render();
  }

  function readDisasm(addr) {
    const len = wasm.wasm_disasm_at(addr);
    const ptr = wasm.wasm_disasm_ptr();
    const bytes = new Uint8Array(wasm.memory.buffer, ptr, len);
    return decoder.decode(bytes);
  }

  function render() {
    const memBuf = wasm.memory.buffer;
    const display = new Uint8Array(memBuf, wasm.wasm_display_ptr(), DISPLAY_W * DISPLAY_H);
    const heat = new Uint8Array(memBuf, wasm.wasm_heat_ptr(), RAM_SIZE);
    const regs = new Uint8Array(memBuf, wasm.wasm_regs_ptr(), 16);
    const stack = new Uint16Array(memBuf, wasm.wasm_stack_ptr(), 16);

    const pc = wasm.wasm_get_pc();
    const idxReg = wasm.wasm_get_i();
    const sp = wasm.wasm_get_sp();
    const dt = wasm.wasm_get_dt();
    const st = wasm.wasm_get_st();
    const cycles = wasm.wasm_get_cycles();

    // 1. Render 64x32 display
    const sData = screenImage.data;
    for (let i = 0; i < display.length; i++) {
      const on = display[i] !== 0;
      const off = i * 4;
      sData[off] = on ? 77 : 10;
      sData[off + 1] = on ? 255 : 20;
      sData[off + 2] = on ? 158 : 16;
      sData[off + 3] = 255;
    }
    screenCtx.putImageData(screenImage, 0, 0);

    // 2. Render 4KB RAM heatmap (64x64)
    const hData = heatmapImage.data;
    for (let addr = 0; addr < RAM_SIZE; addr++) {
      let rgb = heatToRgb(heat[addr]);
      if (addr === pc || addr === (pc + 1) % RAM_SIZE) {
        rgb = [56, 189, 248];
      } else if (addr === idxReg) {
        rgb = [245, 158, 11];
      }
      const off = addr * 4;
      hData[off] = rgb[0];
      hData[off + 1] = rgb[1];
      hData[off + 2] = rgb[2];
      hData[off + 3] = 255;
    }
    heatmapCtx.putImageData(heatmapImage, 0, 0);

    // 3. Special registers & V0..VF
    regPc.textContent = `0x${pc.toString(16).toUpperCase().padStart(3, "0")}`;
    regI.textContent = `0x${idxReg.toString(16).toUpperCase().padStart(3, "0")}`;
    regSp.textContent = String(sp);
    regDt.textContent = String(dt);
    regSt.textContent = String(st);
    cycleReadout.textContent = `${cycles.toLocaleString()} cycles`;

    const curOp = wasm.wasm_peek_opcode(pc);
    opcodeReadout.textContent = `NEXT: 0x${curOp.toString(16).toUpperCase().padStart(4, "0")}`;

    for (let i = 0; i < 16; i++) {
      const v = regs[i];
      vCells[i].val.textContent = v.toString(16).toUpperCase().padStart(2, "0");
      vCells[i].cell.classList.toggle("changed", v !== prevRegs[i]);
      prevRegs[i] = v;
    }

    if (sp === 0) {
      stackReadout.textContent = "(empty)";
    } else {
      const items = [];
      for (let i = 0; i < sp && i < 16; i++) {
        items.push(`0x${stack[i].toString(16).toUpperCase().padStart(3, "0")}`);
      }
      stackReadout.textContent = items.join(" → ");
    }

    // 4. Disassembly feed (14 rows centered around PC)
    const rows = 14;
    const startAddr = Math.max(0, (pc - 12) & ~1);
    let html = "";
    for (let r = 0; r < rows; r++) {
      const addr = (startAddr + r * 2) % RAM_SIZE;
      const op = wasm.wasm_peek_opcode(addr);
      const asm = readDisasm(addr);
      const isCur = addr === pc;
      html += `<div class="disasm-row${isCur ? " current" : ""}">
        <span class="disasm-ptr">${isCur ? "▶" : " "}</span>
        <span class="disasm-addr">0x${addr.toString(16).toUpperCase().padStart(3, "0")}</span>
        <span class="disasm-op">${op.toString(16).toUpperCase().padStart(4, "0")}</span>
        <span>${asm}</span>
      </div>`;
    }
    disasmFeed.innerHTML = html;
  }

  // Event wiring
  romSelect.addEventListener("change", () => {
    switchRom(Number(romSelect.value));
  });

  btnToggle.addEventListener("click", () => setPaused(!paused));

  btnStep.addEventListener("click", () => {
    setPaused(true);
    wasm.wasm_step();
    render();
  });

  btnReset.addEventListener("click", () => {
    wasm.wasm_reset();
    prevRegs.fill(0);
    render();
  });

  btnSound.addEventListener("click", () => {
    const on = audio.toggle();
    btnSound.textContent = on ? "Audio: On" : "Audio: Off";
  });

  speedSlider.addEventListener("input", () => {
    cyclesPerFrame = Number(speedSlider.value);
    speedValue.textContent = `${cyclesPerFrame * 60} Hz`;
  });

  crtToggle.addEventListener("change", () => {
    screenWrap.classList.toggle("crt-on", crtToggle.checked);
  });
  screenWrap.classList.toggle("crt-on", crtToggle.checked);

  heatmapCanvas.addEventListener("mousemove", (e) => {
    const rect = heatmapCanvas.getBoundingClientRect();
    const x = Math.min(63, Math.max(0, Math.floor(((e.clientX - rect.left) / rect.width) * 64)));
    const y = Math.min(63, Math.max(0, Math.floor(((e.clientY - rect.top) / rect.height) * 64)));
    const addr = y * 64 + x;
    const ram = new Uint8Array(wasm.memory.buffer, wasm.wasm_ram_ptr(), RAM_SIZE);
    const heat = new Uint8Array(wasm.memory.buffer, wasm.wasm_heat_ptr(), RAM_SIZE);
    heatHover.textContent = `0x${addr.toString(16).toUpperCase().padStart(3, "0")} (${ramRegion(addr)}) · Val: 0x${ram[addr].toString(16).toUpperCase().padStart(2, "0")} · Heat: ${heat[addr]}`;
  });

  heatmapCanvas.addEventListener("mouseleave", () => {
    heatHover.textContent = "Hover cell to inspect";
  });

  window.addEventListener("keydown", (e) => {
    if (e.target && (e.target.tagName === "INPUT" || e.target.tagName === "SELECT")) {
      return;
    }
    if (e.code === "Space") {
      e.preventDefault();
      setPaused(!paused);
      return;
    }
    const lower = e.key.toLowerCase();
    if (lower === "n") {
      setPaused(true);
      wasm.wasm_step();
      render();
      return;
    }
    if (lower === "r" && !e.ctrlKey && !e.metaKey) {
      wasm.wasm_reset();
      prevRegs.fill(0);
      render();
      return;
    }
    if (e.key === "1") {
      switchRom(0);
      return;
    }
    if (e.key === "2") {
      switchRom(1);
      return;
    }
    if (e.key === "3") {
      switchRom(2);
      return;
    }
    const mapped = KEY_BINDINGS.get(e.key) ?? KEY_BINDINGS.get(lower);
    if (mapped !== undefined) {
      if (e.key.startsWith("Arrow")) e.preventDefault();
      wasm.wasm_set_key(mapped, 1);
      keyButtons.get(mapped)?.classList.add("active");
    }
  });

  window.addEventListener("keyup", (e) => {
    const lower = e.key.toLowerCase();
    const mapped = KEY_BINDINGS.get(e.key) ?? KEY_BINDINGS.get(lower);
    if (mapped !== undefined) {
      wasm.wasm_set_key(mapped, 0);
      keyButtons.get(mapped)?.classList.remove("active");
    }
  });

  // Boot ROM 0 (Bounce)
  switchRom(0);

  function loop() {
    if (!paused) {
      wasm.wasm_step_cycles(cyclesPerFrame);
      wasm.wasm_tick_timers();
      audio.update(wasm.wasm_get_st());
      render();
    } else {
      audio.update(0);
    }
    requestAnimationFrame(loop);
  }

  requestAnimationFrame(loop);
}

main();
