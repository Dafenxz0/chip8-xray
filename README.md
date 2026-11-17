# chip8-xray

[![CI & GitHub Pages](https://github.com/Dafenxz0/chip8-xray/actions/workflows/ci.yml/badge.svg)](https://github.com/Dafenxz0/chip8-xray/actions/workflows/ci.yml)

A complete **CHIP-8 emulator and live CPU X-Ray debugger** written in Rust, shipping both a **Terminal TUI** (`ratatui` + `crossterm`) and a **zero-JS-dependency WebAssembly browser edition** (`wasm32-unknown-unknown`).

**Live Web Demo:** <https://dafenxz0.github.io/chip8-xray/>

## Features

- **Pure-Rust CHIP-8 VM (`src/cpu.rs`)**: Zero OS dependencies, implements all 35 classic opcodes, 4 KB RAM, `V0..VF`, `I`, `PC`, `SP`, 16-level stack, delay/sound timers, 64×32 monochrome framebuffer, 16-key hex keypad, deterministic xorshift PRNG, instruction disassembler (`format_opcode(u16) -> String`), and a per-byte **RAM Activity Heatmap** (`heat[4096]`).
- **Built-in Original Mini-ROMs (`src/roms.rs`)**: Assembled in pure Rust via a 2-pass label builder (no copyrighted ROMs needed):
  1. **Bounce (Pong Demo)** — Bouncing ball, interactive/auto-tracking paddles, BCD score HUD (`Fx33` + `Fx29`), sound trigger.
  2. **10PRINT Maze** — Procedural diagonal maze generator exercising `RND`, `CALL`/`RET`, `DRW`, and delay timer pacing.
  3. **Target Shooter** — Moving ship, drifting targets, projectile collision detection (`VF`), and live BCD score counter.
- **Terminal X-Ray TUI (`src/main.rs`)**: Split-pane dashboard with Unicode half-block (`▀`) 64×32 display, live `V0..VF` & stack inspector, PC-centered disassembler, 4 KB RAM heatmap, and `--headless --cycles N` CLI verification mode.
- **Browser WASM Edition (`web/`)**: Compiles the exact same Rust VM to `chip8_xray.wasm` with shared linear memory telemetry, interactive 64×64 RAM heatmap inspector, clickable/keyboard hex keypad, CRT grid toggle, and optional Web Audio square-wave beep.

## Keybindings

### Terminal TUI
| Key | Action |
| --- | --- |
| `Space` | Pause / Resume |
| `N` | Step single CPU cycle |
| `R` | Reset current ROM |
| `1` / `2` / `3` | Switch built-in ROM (Bounce / Maze / Shooter) |
| `+` / `-` | Adjust execution speed (`cycles/tick`) |
| `W` / `S` / `Up` / `Down` | Keypad `0x1` / `0x4` (Bounce paddle) |
| `A` / `D` / `Left` / `Right` | Keypad `0x7` / `0x9` (Shooter move) |
| `F` / `Enter` | Keypad `0x5` (Shooter fire) |
| `Q` / `Esc` | Quit |

### Browser WASM UI
- `Space` Pause/Resume, `N` Step, `R` Reset, `1`/`2`/`3` Switch ROM.
- Hex keypad mapped to `1234` / `QWER` / `ASDF` / `ZXCV` plus `Arrow` keys and clickable on-screen buttons.

## Quick Start

```bash
# Run Terminal X-Ray TUI
cargo run --release

# Run headless verification (500 cycles)
cargo run -- --headless --cycles 500

# Run unit tests
cargo test

# Build WebAssembly module and serve locally
rustup target add wasm32-unknown-unknown
cargo build --lib --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/chip8_xray.wasm web/chip8_xray.wasm
python3 -m http.server 8080 --directory web
```

## Architecture

```text
src/
├── cpu.rs    # Pure-Rust CHIP-8 VM, 35 opcodes, disassembler, RAM heat tracker, unit tests
├── roms.rs   # 2-pass Rust assembler + 3 original built-in ROMs + determinism tests
├── wasm.rs   # Safe #[no_mangle] extern "C" FFI bridge for wasm32-unknown-unknown
├── lib.rs    # Library root (cdylib + rlib)
└── main.rs   # Terminal X-Ray TUI (ratatui + crossterm) & --headless CLI runner
web/
├── index.html
├── style.css
└── app.js    # Zero-dependency WebAssembly loader, canvases, keypad & disassembler UI
```
