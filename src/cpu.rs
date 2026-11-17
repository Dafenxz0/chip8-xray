extern crate alloc;

use alloc::format;
use alloc::string::String;

pub const MEMORY_SIZE: usize = 4096;
pub const DISPLAY_WIDTH: usize = 64;
pub const DISPLAY_HEIGHT: usize = 32;
pub const DISPLAY_SIZE: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT;
pub const FONTSET_START: u16 = 0x050;
pub const PROGRAM_START: u16 = 0x200;
pub const STACK_SIZE: usize = 16;
pub const KEYPAD_SIZE: usize = 16;

const DEFAULT_RNG_SEED: u32 = 0x1337_CAFE;

pub const FONTSET: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

const fn init_memory() -> [u8; MEMORY_SIZE] {
    let mut mem = [0u8; MEMORY_SIZE];
    let start = FONTSET_START as usize;
    let mut idx = 0;
    while idx < FONTSET.len() {
        mem[start + idx] = FONTSET[idx];
        idx += 1;
    }
    mem
}

#[derive(Clone)]
pub struct Cpu {
    pub memory: [u8; MEMORY_SIZE],
    pub heat: [u8; MEMORY_SIZE],
    pub v: [u8; 16],
    pub i: u16,
    pub pc: u16,
    pub sp: u8,
    pub stack: [u16; STACK_SIZE],
    pub delay_timer: u8,
    pub sound_timer: u8,
    pub display: [u8; DISPLAY_SIZE],
    pub keypad: [u8; KEYPAD_SIZE],
    pub cycles: u64,
    pub last_opcode: u16,
    rng_state: u32,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub const fn new() -> Self {
        Self {
            memory: init_memory(),
            heat: [0u8; MEMORY_SIZE],
            v: [0u8; 16],
            i: 0,
            pc: PROGRAM_START,
            sp: 0,
            stack: [0u16; STACK_SIZE],
            delay_timer: 0,
            sound_timer: 0,
            display: [0u8; DISPLAY_SIZE],
            keypad: [0u8; KEYPAD_SIZE],
            cycles: 0,
            last_opcode: 0,
            rng_state: DEFAULT_RNG_SEED,
        }
    }

    pub fn reset(&mut self) {
        self.memory = init_memory();
        self.heat.fill(0);
        self.v.fill(0);
        self.i = 0;
        self.pc = PROGRAM_START;
        self.sp = 0;
        self.stack.fill(0);
        self.delay_timer = 0;
        self.sound_timer = 0;
        self.display.fill(0);
        self.keypad.fill(0);
        self.cycles = 0;
        self.last_opcode = 0;
        self.rng_state = DEFAULT_RNG_SEED;
    }

    pub fn load_rom(&mut self, bytes: &[u8]) {
        self.reset();
        let start = PROGRAM_START as usize;
        let max_len = MEMORY_SIZE - start;
        let count = bytes.len().min(max_len);
        self.memory[start..start + count].copy_from_slice(&bytes[..count]);
        for h in &mut self.heat[start..start + count] {
            *h = 120;
        }
    }

    pub fn set_rng_seed(&mut self, seed: u32) {
        self.rng_state = if seed == 0 { DEFAULT_RNG_SEED } else { seed };
    }

    pub fn set_key(&mut self, key: u8, pressed: bool) {
        if let Some(slot) = self.keypad.get_mut(usize::from(key & 0x0F)) {
            *slot = u8::from(pressed);
        }
    }

    pub fn tick_timers(&mut self) {
        if self.delay_timer > 0 {
            self.delay_timer -= 1;
        }
        if self.sound_timer > 0 {
            self.sound_timer -= 1;
        }
    }

    pub fn peek_opcode(&self, addr: u16) -> u16 {
        let idx = usize::from(addr) % MEMORY_SIZE;
        let hi = u16::from(self.memory[idx]);
        let lo = u16::from(self.memory[(idx + 1) % MEMORY_SIZE]);
        (hi << 8) | lo
    }

    pub fn active_pixels(&self) -> usize {
        self.display.iter().filter(|&&px| px != 0).count()
    }

    fn read_ram(&mut self, addr: u16) -> u8 {
        let idx = usize::from(addr) % MEMORY_SIZE;
        self.heat[idx] = self.heat[idx].max(200);
        self.memory[idx]
    }

    fn write_ram(&mut self, addr: u16, value: u8) {
        let idx = usize::from(addr) % MEMORY_SIZE;
        self.memory[idx] = value;
        self.heat[idx] = 255;
    }

    fn fetch_opcode(&mut self) -> u16 {
        let hi_addr = usize::from(self.pc) % MEMORY_SIZE;
        let lo_addr = (hi_addr + 1) % MEMORY_SIZE;
        self.heat[hi_addr] = self.heat[hi_addr].max(160);
        self.heat[lo_addr] = self.heat[lo_addr].max(160);
        let opcode = (u16::from(self.memory[hi_addr]) << 8) | u16::from(self.memory[lo_addr]);
        self.pc = ((hi_addr + 2) % MEMORY_SIZE) as u16;
        opcode
    }

    fn decay_heat(&mut self) {
        for h in &mut self.heat {
            *h = h.saturating_sub(2);
        }
    }

    fn next_random_byte(&mut self) -> u8 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        (x >> 16) as u8
    }

    pub fn step(&mut self) {
        self.decay_heat();
        let opcode = self.fetch_opcode();
        self.last_opcode = opcode;
        self.cycles = self.cycles.wrapping_add(1);
        self.execute(opcode);
    }

    fn execute(&mut self, opcode: u16) {
        let nnn = opcode & 0x0FFF;
        let n = (opcode & 0x000F) as u8;
        let x = usize::from((opcode >> 8) & 0x0F);
        let y = usize::from((opcode >> 4) & 0x0F);
        let kk = (opcode & 0x00FF) as u8;

        match opcode & 0xF000 {
            0x0000 => match opcode {
                0x00E0 => self.display.fill(0),
                0x00EE => {
                    if self.sp > 0 {
                        self.sp -= 1;
                        self.pc = self.stack[usize::from(self.sp)];
                    }
                }
                _ => {
                    // 0nnn (SYS addr) is a no-op on modern CHIP-8 interpreters.
                }
            },
            0x1000 => self.pc = nnn,
            0x2000 => {
                if usize::from(self.sp) < STACK_SIZE {
                    self.stack[usize::from(self.sp)] = self.pc;
                    self.sp += 1;
                    self.pc = nnn;
                }
            }
            0x3000 => {
                if self.v[x] == kk {
                    self.pc = self.pc.wrapping_add(2);
                }
            }
            0x4000 => {
                if self.v[x] != kk {
                    self.pc = self.pc.wrapping_add(2);
                }
            }
            0x5000 => {
                if n == 0 && self.v[x] == self.v[y] {
                    self.pc = self.pc.wrapping_add(2);
                }
            }
            0x6000 => self.v[x] = kk,
            0x7000 => self.v[x] = self.v[x].wrapping_add(kk),
            0x8000 => match n {
                0x0 => self.v[x] = self.v[y],
                0x1 => self.v[x] |= self.v[y],
                0x2 => self.v[x] &= self.v[y],
                0x3 => self.v[x] ^= self.v[y],
                0x4 => {
                    let (sum, carry) = self.v[x].overflowing_add(self.v[y]);
                    self.v[x] = sum;
                    self.v[0xF] = u8::from(carry);
                }
                0x5 => {
                    let not_borrow = self.v[x] >= self.v[y];
                    self.v[x] = self.v[x].wrapping_sub(self.v[y]);
                    self.v[0xF] = u8::from(not_borrow);
                }
                0x6 => {
                    let lsb = self.v[x] & 1;
                    self.v[x] >>= 1;
                    self.v[0xF] = lsb;
                }
                0x7 => {
                    let not_borrow = self.v[y] >= self.v[x];
                    self.v[x] = self.v[y].wrapping_sub(self.v[x]);
                    self.v[0xF] = u8::from(not_borrow);
                }
                0xE => {
                    let msb = (self.v[x] >> 7) & 1;
                    self.v[x] <<= 1;
                    self.v[0xF] = msb;
                }
                _ => {}
            },
            0x9000 => {
                if n == 0 && self.v[x] != self.v[y] {
                    self.pc = self.pc.wrapping_add(2);
                }
            }
            0xA000 => self.i = nnn,
            0xB000 => self.pc = nnn.wrapping_add(u16::from(self.v[0])) & 0x0FFF,
            0xC000 => self.v[x] = self.next_random_byte() & kk,
            0xD000 => {
                let x_origin = usize::from(self.v[x]) % DISPLAY_WIDTH;
                let y_origin = usize::from(self.v[y]) % DISPLAY_HEIGHT;
                self.v[0xF] = 0;

                for row in 0..usize::from(n) {
                    let sprite_byte = self.read_ram(self.i.wrapping_add(row as u16));
                    let py = (y_origin + row) % DISPLAY_HEIGHT;
                    for col in 0..8usize {
                        if (sprite_byte & (0x80 >> col)) != 0 {
                            let px = (x_origin + col) % DISPLAY_WIDTH;
                            let idx = py * DISPLAY_WIDTH + px;
                            if self.display[idx] == 1 {
                                self.v[0xF] = 1;
                            }
                            self.display[idx] ^= 1;
                        }
                    }
                }
            }
            0xE000 => match kk {
                0x9E => {
                    if self.keypad[usize::from(self.v[x] & 0x0F)] != 0 {
                        self.pc = self.pc.wrapping_add(2);
                    }
                }
                0xA1 => {
                    if self.keypad[usize::from(self.v[x] & 0x0F)] == 0 {
                        self.pc = self.pc.wrapping_add(2);
                    }
                }
                _ => {}
            },
            0xF000 => match kk {
                0x07 => self.v[x] = self.delay_timer,
                0x0A => {
                    if let Some((key_idx, _)) =
                        self.keypad.iter().enumerate().find(|(_, &state)| state != 0)
                    {
                        self.v[x] = key_idx as u8;
                    } else {
                        self.pc = self.pc.wrapping_sub(2);
                    }
                }
                0x15 => self.delay_timer = self.v[x],
                0x18 => self.sound_timer = self.v[x],
                0x1E => self.i = self.i.wrapping_add(u16::from(self.v[x])),
                0x29 => self.i = FONTSET_START + u16::from(self.v[x] & 0x0F) * 5,
                0x33 => {
                    let val = self.v[x];
                    self.write_ram(self.i, val / 100);
                    self.write_ram(self.i.wrapping_add(1), (val / 10) % 10);
                    self.write_ram(self.i.wrapping_add(2), val % 10);
                }
                0x55 => {
                    for offset in 0..=x {
                        let addr = self.i.wrapping_add(offset as u16);
                        self.write_ram(addr, self.v[offset]);
                    }
                }
                0x65 => {
                    for offset in 0..=x {
                        let addr = self.i.wrapping_add(offset as u16);
                        let val = self.read_ram(addr);
                        self.v[offset] = val;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

pub fn format_opcode(opcode: u16) -> String {
    let nnn = opcode & 0x0FFF;
    let n = opcode & 0x000F;
    let x = (opcode >> 8) & 0x0F;
    let y = (opcode >> 4) & 0x0F;
    let kk = opcode & 0x00FF;

    match opcode & 0xF000 {
        0x0000 => match opcode {
            0x00E0 => String::from("CLS"),
            0x00EE => String::from("RET"),
            _ => format!("SYS 0x{nnn:03X}"),
        },
        0x1000 => format!("JP 0x{nnn:03X}"),
        0x2000 => format!("CALL 0x{nnn:03X}"),
        0x3000 => format!("SE V{x:X}, 0x{kk:02X}"),
        0x4000 => format!("SNE V{x:X}, 0x{kk:02X}"),
        0x5000 if n == 0 => format!("SE V{x:X}, V{y:X}"),
        0x6000 => format!("LD V{x:X}, 0x{kk:02X}"),
        0x7000 => format!("ADD V{x:X}, 0x{kk:02X}"),
        0x8000 => match n {
            0x0 => format!("LD V{x:X}, V{y:X}"),
            0x1 => format!("OR V{x:X}, V{y:X}"),
            0x2 => format!("AND V{x:X}, V{y:X}"),
            0x3 => format!("XOR V{x:X}, V{y:X}"),
            0x4 => format!("ADD V{x:X}, V{y:X}"),
            0x5 => format!("SUB V{x:X}, V{y:X}"),
            0x6 => format!("SHR V{x:X}"),
            0x7 => format!("SUBN V{x:X}, V{y:X}"),
            0xE => format!("SHL V{x:X}"),
            _ => format!("DW 0x{opcode:04X}"),
        },
        0x9000 if n == 0 => format!("SNE V{x:X}, V{y:X}"),
        0xA000 => format!("LD I, 0x{nnn:03X}"),
        0xB000 => format!("JP V0, 0x{nnn:03X}"),
        0xC000 => format!("RND V{x:X}, 0x{kk:02X}"),
        0xD000 => format!("DRW V{x:X}, V{y:X}, {n}"),
        0xE000 => match kk {
            0x9E => format!("SKP V{x:X}"),
            0xA1 => format!("SKNP V{x:X}"),
            _ => format!("DW 0x{opcode:04X}"),
        },
        0xF000 => match kk {
            0x07 => format!("LD V{x:X}, DT"),
            0x0A => format!("LD V{x:X}, K"),
            0x15 => format!("LD DT, V{x:X}"),
            0x18 => format!("LD ST, V{x:X}"),
            0x1E => format!("ADD I, V{x:X}"),
            0x29 => format!("LD F, V{x:X}"),
            0x33 => format!("LD B, V{x:X}"),
            0x55 => format!("LD [I], V{x:X}"),
            0x65 => format!("LD V{x:X}, [I]"),
            _ => format!("DW 0x{opcode:04X}"),
        },
        _ => format!("DW 0x{opcode:04X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_with_opcodes(opcodes: &[u16]) -> Cpu {
        let mut cpu = Cpu::new();
        let mut rom = alloc::vec::Vec::with_capacity(opcodes.len() * 2);
        for &op in opcodes {
            rom.push((op >> 8) as u8);
            rom.push((op & 0xFF) as u8);
        }
        cpu.load_rom(&rom);
        cpu
    }

    #[test]
    fn arithmetic_and_shift_flags() {
        let mut cpu = cpu_with_opcodes(&[
            0x60C8, // LD V0, 200
            0x6164, // LD V1, 100
            0x8014, // ADD V0, V1 -> 44, VF = 1
            0x620A, // LD V2, 10
            0x8025, // SUB V0, V2 -> 34, VF = 1 (no borrow)
            0x6330, // LD V3, 48
            0x8035, // SUB V0, V3 -> 242, VF = 0 (borrow)
            0x6405, // LD V4, 0b0000_0101
            0x8406, // SHR V4 -> 2, VF = 1
            0x6581, // LD V5, 0b1000_0001
            0x850E, // SHL V5 -> 2, VF = 1
        ]);

        cpu.step();
        cpu.step();
        cpu.step();
        assert_eq!(cpu.v[0], 44);
        assert_eq!(cpu.v[0xF], 1);

        cpu.step();
        cpu.step();
        assert_eq!(cpu.v[0], 34);
        assert_eq!(cpu.v[0xF], 1);

        cpu.step();
        cpu.step();
        assert_eq!(cpu.v[0], 242);
        assert_eq!(cpu.v[0xF], 0);

        cpu.step();
        cpu.step();
        assert_eq!(cpu.v[4], 2);
        assert_eq!(cpu.v[0xF], 1);

        cpu.step();
        cpu.step();
        assert_eq!(cpu.v[5], 2);
        assert_eq!(cpu.v[0xF], 1);
    }

    #[test]
    fn sprite_draw_and_collision_flag() {
        let mut cpu = cpu_with_opcodes(&[
            0xA20E, // LD I, 0x20E (sprite byte 0x80)
            0x6004, // LD V0, 4
            0x6103, // LD V1, 3
            0xD011, // DRW V0, V1, 1 -> draws (4,3), VF = 0
            0xD011, // DRW V0, V1, 1 -> XORs (4,3) off, VF = 1
            0x0000, // padding
            0x0000, // padding
            0x8000, // sprite byte 0x80 at 0x20E
        ]);

        for _ in 0..4 {
            cpu.step();
        }
        assert_eq!(cpu.display[3 * DISPLAY_WIDTH + 4], 1);
        assert_eq!(cpu.v[0xF], 0);

        cpu.step();
        assert_eq!(cpu.display[3 * DISPLAY_WIDTH + 4], 0);
        assert_eq!(cpu.v[0xF], 1);
    }

    #[test]
    fn bcd_stores_three_digits_and_heats_memory() {
        let mut cpu = cpu_with_opcodes(&[
            0x60E7, // LD V0, 231
            0xA300, // LD I, 0x300
            0xF033, // LD B, V0
        ]);

        cpu.step();
        cpu.step();
        cpu.step();

        assert_eq!(cpu.memory[0x300], 2);
        assert_eq!(cpu.memory[0x301], 3);
        assert_eq!(cpu.memory[0x302], 1);
        assert_eq!(cpu.heat[0x300], 255);
        assert_eq!(cpu.heat[0x301], 255);
        assert_eq!(cpu.heat[0x302], 255);
    }

    #[test]
    fn stack_call_and_return() {
        let mut cpu = cpu_with_opcodes(&[
            0x2206, // 0x200: CALL 0x206
            0x6142, // 0x202: LD V1, 0x42
            0x1204, // 0x204: JP 0x204
            0x6007, // 0x206: LD V0, 0x07
            0x00EE, // 0x208: RET
        ]);

        cpu.step(); // CALL
        assert_eq!(cpu.pc, 0x206);
        assert_eq!(cpu.sp, 1);
        assert_eq!(cpu.stack[0], 0x202);

        cpu.step(); // LD V0, 0x07
        assert_eq!(cpu.v[0], 0x07);

        cpu.step(); // RET
        assert_eq!(cpu.pc, 0x202);
        assert_eq!(cpu.sp, 0);

        cpu.step(); // LD V1, 0x42
        assert_eq!(cpu.v[1], 0x42);
    }

    #[test]
    fn reg_dump_and_load_memory() {
        let mut cpu = cpu_with_opcodes(&[
            0x6011, // LD V0, 0x11
            0x6122, // LD V1, 0x22
            0x6233, // LD V2, 0x33
            0xA350, // LD I, 0x350
            0xF255, // LD [I], V2
            0x6000, // LD V0, 0
            0x6100, // LD V1, 0
            0x6200, // LD V2, 0
            0xF265, // LD V2, [I]
        ]);

        for _ in 0..9 {
            cpu.step();
        }
        assert_eq!(cpu.v[0], 0x11);
        assert_eq!(cpu.v[1], 0x22);
        assert_eq!(cpu.v[2], 0x33);
    }

    #[test]
    fn disassembler_formats_key_opcodes() {
        assert_eq!(format_opcode(0x00E0), "CLS");
        assert_eq!(format_opcode(0x00EE), "RET");
        assert_eq!(format_opcode(0x1234), "JP 0x234");
        assert_eq!(format_opcode(0x8124), "ADD V1, V2");
        assert_eq!(format_opcode(0xD345), "DRW V3, V4, 5");
        assert_eq!(format_opcode(0xFA33), "LD B, VA");
        assert_eq!(format_opcode(0xFFFF), "DW 0xFFFF");
    }
}
