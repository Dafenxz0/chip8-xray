use std::sync::Mutex;

use crate::cpu::{format_opcode, Cpu};
use crate::roms::RomId;

struct WasmBridge {
    cpu: Cpu,
    rom_id: usize,
    disasm_buf: [u8; 64],
}

impl WasmBridge {
    const fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            rom_id: 0,
            disasm_buf: [0u8; 64],
        }
    }

    fn load(&mut self, index: usize) {
        let rom = RomId::from_index(index);
        self.rom_id = rom as usize;
        self.cpu.load_rom(&rom.build());
    }
}

static BRIDGE: Mutex<WasmBridge> = Mutex::new(WasmBridge::new());

fn with_bridge<R>(f: impl FnOnce(&mut WasmBridge) -> R) -> R {
    let mut guard = BRIDGE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

#[no_mangle]
pub extern "C" fn wasm_init(rom_index: u32) {
    with_bridge(|b| b.load(rom_index as usize));
}

#[no_mangle]
pub extern "C" fn wasm_reset() {
    with_bridge(|b| b.load(b.rom_id));
}

#[no_mangle]
pub extern "C" fn wasm_step() {
    with_bridge(|b| b.cpu.step());
}

#[no_mangle]
pub extern "C" fn wasm_step_cycles(n: u32) {
    with_bridge(|b| {
        for _ in 0..n {
            b.cpu.step();
        }
    });
}

#[no_mangle]
pub extern "C" fn wasm_tick_timers() {
    with_bridge(|b| b.cpu.tick_timers());
}

#[no_mangle]
pub extern "C" fn wasm_set_key(key: u32, pressed: u32) {
    with_bridge(|b| b.cpu.set_key((key & 0x0F) as u8, pressed != 0));
}

#[no_mangle]
pub extern "C" fn wasm_display_ptr() -> *const u8 {
    with_bridge(|b| b.cpu.display.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_ram_ptr() -> *const u8 {
    with_bridge(|b| b.cpu.memory.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_heat_ptr() -> *const u8 {
    with_bridge(|b| b.cpu.heat.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_regs_ptr() -> *const u8 {
    with_bridge(|b| b.cpu.v.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_stack_ptr() -> *const u16 {
    with_bridge(|b| b.cpu.stack.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_disasm_ptr() -> *const u8 {
    with_bridge(|b| b.disasm_buf.as_ptr())
}

#[no_mangle]
pub extern "C" fn wasm_disasm_at(addr: u32) -> u32 {
    with_bridge(|b| {
        let opcode = b.cpu.peek_opcode(addr as u16);
        let text = format_opcode(opcode);
        let bytes = text.as_bytes();
        let len = bytes.len().min(b.disasm_buf.len());
        b.disasm_buf[..len].copy_from_slice(&bytes[..len]);
        len as u32
    })
}

#[no_mangle]
pub extern "C" fn wasm_peek_opcode(addr: u32) -> u32 {
    with_bridge(|b| u32::from(b.cpu.peek_opcode(addr as u16)))
}

#[no_mangle]
pub extern "C" fn wasm_get_pc() -> u32 {
    with_bridge(|b| u32::from(b.cpu.pc))
}

#[no_mangle]
pub extern "C" fn wasm_get_i() -> u32 {
    with_bridge(|b| u32::from(b.cpu.i))
}

#[no_mangle]
pub extern "C" fn wasm_get_sp() -> u32 {
    with_bridge(|b| u32::from(b.cpu.sp))
}

#[no_mangle]
pub extern "C" fn wasm_get_dt() -> u32 {
    with_bridge(|b| u32::from(b.cpu.delay_timer))
}

#[no_mangle]
pub extern "C" fn wasm_get_st() -> u32 {
    with_bridge(|b| u32::from(b.cpu.sound_timer))
}

#[no_mangle]
pub extern "C" fn wasm_get_cycles() -> f64 {
    with_bridge(|b| b.cpu.cycles as f64)
}

#[no_mangle]
pub extern "C" fn wasm_get_rom_id() -> u32 {
    with_bridge(|b| b.rom_id as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_bridge_steps_and_disassembles() {
        wasm_init(0);
        assert_eq!(wasm_get_rom_id(), 0);
        assert_eq!(wasm_get_pc(), 0x200);
        wasm_step_cycles(10);
        assert_eq!(wasm_get_cycles(), 10.0);
        let len = wasm_disasm_at(wasm_get_pc()) as usize;
        assert!(len > 0);
        assert!(wasm_peek_opcode(0x200) != 0);
    }
}
