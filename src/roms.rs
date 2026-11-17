extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::cpu::PROGRAM_START;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RomId {
    Bounce = 0,
    Maze = 1,
    Shooter = 2,
}

impl RomId {
    pub const ALL: [RomId; 3] = [RomId::Bounce, RomId::Maze, RomId::Shooter];

    pub fn from_index(index: usize) -> Self {
        match index % 3 {
            0 => RomId::Bounce,
            1 => RomId::Maze,
            _ => RomId::Shooter,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RomId::Bounce => "Bounce (Pong Demo)",
            RomId::Maze => "10PRINT Maze",
            RomId::Shooter => "Target Shooter",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            RomId::Bounce => "Bounce",
            RomId::Maze => "Maze",
            RomId::Shooter => "Shooter",
        }
    }

    pub fn build(self) -> Vec<u8> {
        match self {
            RomId::Bounce => build_bounce_rom(),
            RomId::Maze => build_maze_rom(),
            RomId::Shooter => build_shooter_rom(),
        }
    }
}

enum PatchKind {
    Addr12(u16),
}

struct RomBuilder {
    bytes: Vec<u8>,
    labels: BTreeMap<&'static str, u16>,
    patches: Vec<(usize, &'static str, PatchKind)>,
}

impl RomBuilder {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            labels: BTreeMap::new(),
            patches: Vec::new(),
        }
    }

    fn current_addr(&self) -> u16 {
        PROGRAM_START + self.bytes.len() as u16
    }

    fn label(&mut self, name: &'static str) {
        self.labels.insert(name, self.current_addr());
    }

    fn emit(&mut self, opcode: u16) {
        self.bytes.push((opcode >> 8) as u8);
        self.bytes.push((opcode & 0xFF) as u8);
    }

    fn emit_patched(&mut self, base_opcode: u16, target: &'static str) {
        let pos = self.bytes.len();
        self.emit(base_opcode);
        self.patches
            .push((pos, target, PatchKind::Addr12(base_opcode & 0xF000)));
    }

    fn data(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn cls(&mut self) {
        self.emit(0x00E0);
    }

    fn ret(&mut self) {
        self.emit(0x00EE);
    }

    fn jp(&mut self, target: &'static str) {
        self.emit_patched(0x1000, target);
    }

    fn call(&mut self, target: &'static str) {
        self.emit_patched(0x2000, target);
    }

    fn se_byte(&mut self, x: u8, kk: u8) {
        self.emit(0x3000 | (u16::from(x) << 8) | u16::from(kk));
    }

    fn sne_byte(&mut self, x: u8, kk: u8) {
        self.emit(0x4000 | (u16::from(x) << 8) | u16::from(kk));
    }

    fn ld_byte(&mut self, x: u8, kk: u8) {
        self.emit(0x6000 | (u16::from(x) << 8) | u16::from(kk));
    }

    fn add_byte(&mut self, x: u8, kk: u8) {
        self.emit(0x7000 | (u16::from(x) << 8) | u16::from(kk));
    }

    fn ld_reg(&mut self, x: u8, y: u8) {
        self.emit(0x8000 | (u16::from(x) << 8) | (u16::from(y) << 4));
    }

    fn add_reg(&mut self, x: u8, y: u8) {
        self.emit(0x8004 | (u16::from(x) << 8) | (u16::from(y) << 4));
    }

    fn ld_i(&mut self, target: &'static str) {
        self.emit_patched(0xA000, target);
    }

    fn rnd(&mut self, x: u8, mask: u8) {
        self.emit(0xC000 | (u16::from(x) << 8) | u16::from(mask));
    }

    fn drw(&mut self, x: u8, y: u8, n: u8) {
        self.emit(0xD000 | (u16::from(x) << 8) | (u16::from(y) << 4) | u16::from(n & 0x0F));
    }

    fn skp(&mut self, x: u8) {
        self.emit(0xE09E | (u16::from(x) << 8));
    }

    fn sknp(&mut self, x: u8) {
        self.emit(0xE0A1 | (u16::from(x) << 8));
    }

    fn ld_reg_dt(&mut self, x: u8) {
        self.emit(0xF007 | (u16::from(x) << 8));
    }

    fn ld_dt(&mut self, x: u8) {
        self.emit(0xF015 | (u16::from(x) << 8));
    }

    fn ld_st(&mut self, x: u8) {
        self.emit(0xF018 | (u16::from(x) << 8));
    }

    fn ld_font(&mut self, x: u8) {
        self.emit(0xF029 | (u16::from(x) << 8));
    }

    fn ld_bcd(&mut self, x: u8) {
        self.emit(0xF033 | (u16::from(x) << 8));
    }

    fn ld_regs_from_i(&mut self, x: u8) {
        self.emit(0xF065 | (u16::from(x) << 8));
    }

    fn finish(mut self) -> Vec<u8> {
        for (pos, target, kind) in self.patches {
            let addr = *self
                .labels
                .get(target)
                .unwrap_or_else(|| panic!("undefined ROM label: {target}"));
            match kind {
                PatchKind::Addr12(prefix) => {
                    let patched = prefix | (addr & 0x0FFF);
                    self.bytes[pos] = (patched >> 8) as u8;
                    self.bytes[pos + 1] = (patched & 0xFF) as u8;
                }
            }
        }
        self.bytes
    }
}

/// Registers:
/// V4 = ball_x, V5 = ball_y, V6 = dx, V7 = dy,
/// V8 = left_paddle_y, V9 = right_paddle_y, VA = score
/// V0..V3 = scratch (safe for Fx33 + Fx65 BCD score rendering)
fn build_bounce_rom() -> Vec<u8> {
    let mut b = RomBuilder::new();

    b.cls();
    b.ld_byte(4, 20);
    b.ld_byte(5, 14);
    b.ld_byte(6, 1);
    b.ld_byte(7, 1);
    b.ld_byte(8, 13);
    b.ld_byte(9, 13);
    b.ld_byte(0xA, 0);

    // Draw initial score, paddles, and ball
    b.call("draw_score");
    b.call("draw_paddles");
    b.ld_i("ball_sprite");
    b.drw(4, 5, 1);

    b.label("main_loop");

    // Erase old ball & paddles before updating state
    b.ld_i("ball_sprite");
    b.drw(4, 5, 1);
    b.call("draw_paddles");

    // Check left paddle keys: 0x1 = up, 0x4 = down
    b.ld_byte(0, 0x01);
    b.sknp(0);
    b.jp("pad_up");
    b.ld_byte(0, 0x04);
    b.sknp(0);
    b.jp("pad_down");
    // Auto-track lightly when ball approaches left side and no key is pressed
    b.jp("pad_auto");

    b.label("pad_up");
    b.se_byte(8, 7);
    b.add_byte(8, 0xFF);
    b.jp("after_left_pad");

    b.label("pad_down");
    b.se_byte(8, 25);
    b.add_byte(8, 1);
    b.jp("after_left_pad");

    b.label("pad_auto");
    b.ld_reg(0, 5);
    b.add_byte(0, 0xFE); // target_y = ball_y - 2
    b.ld_reg(3, 8);
    b.emit(0x8305); // SUB V3, V0 -> V3 = V8 - target_y, VF = 1 if V8 >= target_y
    b.sne_byte(3, 0);
    b.jp("after_left_pad");
    b.se_byte(0xF, 1);
    b.jp("auto_down");
    b.se_byte(8, 7);
    b.add_byte(8, 0xFF);
    b.jp("after_left_pad");

    b.label("auto_down");
    b.se_byte(8, 25);
    b.add_byte(8, 1);

    b.label("after_left_pad");
    // Right paddle smoothly tracks ball_y clamped to [7, 25]
    b.ld_reg(9, 5);
    b.add_byte(9, 0xFE);
    b.ld_byte(0, 7);
    b.ld_reg(3, 9);
    b.emit(0x8305); // VF=1 if V9 >= 7
    b.se_byte(0xF, 1);
    b.ld_byte(9, 7);
    b.ld_byte(0, 25);
    b.ld_reg(3, 9);
    b.emit(0x8305); // VF=1 if V9 >= 25
    b.sne_byte(0xF, 1);
    b.jp("right_pad_ok");
    b.ld_byte(9, 25);

    b.label("right_pad_ok");
    // Update ball position
    b.add_reg(4, 6);
    b.add_reg(5, 7);

    // Bounce top/bottom in [7, 30]
    b.se_byte(5, 7);
    b.jp("check_bottom");
    b.ld_byte(7, 1);
    b.jp("check_left");

    b.label("check_bottom");
    b.se_byte(5, 30);
    b.jp("check_left");
    b.ld_byte(7, 0xFF);

    b.label("check_left");
    b.se_byte(4, 3);
    b.jp("check_right");
    b.ld_byte(6, 1);
    b.jp("score_hit");

    b.label("check_right");
    b.se_byte(4, 60);
    b.jp("draw_frame");
    b.ld_byte(6, 0xFF);

    b.label("score_hit");
    // Erase old score, increment score (wrap at 99), draw new score, beep briefly
    b.call("draw_score");
    b.add_byte(0xA, 1);
    b.sne_byte(0xA, 100);
    b.ld_byte(0xA, 0);
    b.call("draw_score");
    b.ld_byte(0, 2);
    b.ld_st(0);

    b.label("draw_frame");
    b.call("draw_paddles");
    b.ld_i("ball_sprite");
    b.drw(4, 5, 1);

    // 1-tick frame pacer
    b.ld_byte(0, 1);
    b.ld_dt(0);
    b.label("wait_dt");
    b.ld_reg_dt(0);
    b.se_byte(0, 0);
    b.jp("wait_dt");

    b.jp("main_loop");

    // Subroutine: draw left & right paddles
    b.label("draw_paddles");
    b.ld_i("paddle_sprite");
    b.ld_byte(0, 2);
    b.drw(0, 8, 6);
    b.ld_byte(0, 61);
    b.drw(0, 9, 6);
    b.ret();

    // Subroutine: XOR 2-digit BCD score of VA at (27, 1) and (33, 1)
    b.label("draw_score");
    b.ld_i("bcd_buf");
    b.ld_bcd(0xA);
    b.ld_regs_from_i(2); // V0 = hundreds, V1 = tens, V2 = ones
    b.ld_reg(3, 2);      // save ones in V3
    b.ld_byte(0, 27);
    b.ld_byte(2, 1);
    b.ld_font(1);
    b.drw(0, 2, 5);
    b.ld_byte(0, 33);
    b.ld_font(3);
    b.drw(0, 2, 5);
    b.ret();

    b.label("ball_sprite");
    b.data(&[0x80]);

    b.label("paddle_sprite");
    b.data(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80]);

    b.label("bcd_buf");
    b.data(&[0x00, 0x00, 0x00, 0x00]);

    b.finish()
}

/// Procedural 10PRINT Maze generator using RND, CALL/RET, DRW, and DT.
/// Registers: V4 = x (0..60 step 4), V5 = y (0..28 step 4)
fn build_maze_rom() -> Vec<u8> {
    let mut b = RomBuilder::new();

    b.label("restart");
    b.cls();
    b.ld_byte(4, 0);
    b.ld_byte(5, 0);

    b.label("tile_loop");
    b.call("draw_tile");
    b.add_byte(4, 4);
    b.se_byte(4, 64);
    b.jp("tile_loop");

    b.ld_byte(4, 0);
    b.add_byte(5, 4);
    b.se_byte(5, 32);
    b.jp("tile_loop");

    // Pause briefly on completed maze before generating a new one
    b.ld_byte(0, 25);
    b.ld_dt(0);
    b.label("wait_maze");
    b.ld_reg_dt(0);
    b.se_byte(0, 0);
    b.jp("wait_maze");
    b.jp("restart");

    b.label("draw_tile");
    b.rnd(0, 0x01);
    b.se_byte(0, 0);
    b.jp("use_slash");
    b.ld_i("backslash_sprite");
    b.jp("do_draw");

    b.label("use_slash");
    b.ld_i("slash_sprite");

    b.label("do_draw");
    b.drw(4, 5, 4);
    b.ret();

    b.label("backslash_sprite");
    b.data(&[0x80, 0x40, 0x20, 0x10]);

    b.label("slash_sprite");
    b.data(&[0x10, 0x20, 0x40, 0x80]);

    b.finish()
}

/// Target Shooter:
/// V4 = ship_x (2..58), V5 = target_x (2..56), V6 = target_dx (1 or 255),
/// V7 = bullet_x, V8 = bullet_y, V9 = bullet_active (0/1), VA = score, VB = auto_tick
fn build_shooter_rom() -> Vec<u8> {
    let mut b = RomBuilder::new();

    b.cls();
    b.ld_byte(4, 30);
    b.ld_byte(5, 16);
    b.ld_byte(6, 1);
    b.ld_byte(7, 0);
    b.ld_byte(8, 0);
    b.ld_byte(9, 0);
    b.ld_byte(0xA, 0);
    b.ld_byte(0xB, 0);

    b.call("draw_score");
    b.call("draw_ship");
    b.call("draw_target");

    b.label("loop");
    // Erase ship and target
    b.call("draw_ship");
    b.call("draw_target");

    // Erase bullet if active
    b.se_byte(9, 1);
    b.jp("input_check");
    b.call("draw_bullet");

    b.label("input_check");
    // Move left with key 0x7, right with key 0x9, fire with key 0x5
    b.ld_byte(0, 0x07);
    b.sknp(0);
    b.jp("ship_left");
    b.ld_byte(0, 0x09);
    b.sknp(0);
    b.jp("ship_right");

    // Gentle auto-patrol when idle so demo stays animated
    b.add_byte(0xB, 1);
    b.ld_reg(0, 0xB);
    b.ld_byte(1, 0x10);
    b.emit(0x8012); // AND V0, V1
    b.se_byte(0, 0);
    b.jp("ship_right");
    b.jp("ship_left");

    b.label("ship_left");
    b.se_byte(4, 2);
    b.add_byte(4, 0xFF);
    b.jp("fire_check");

    b.label("ship_right");
    b.se_byte(4, 58);
    b.add_byte(4, 1);

    b.label("fire_check");
    b.se_byte(9, 0);
    b.jp("move_bullet");
    // Fire if key 0x5 pressed OR every 16th tick automatically
    b.ld_byte(0, 0x05);
    b.sknp(0);
    b.jp("spawn_bullet");
    b.ld_reg(0, 0xB);
    b.ld_byte(1, 0x0F);
    b.emit(0x8012); // AND V0, V1
    b.se_byte(0, 0);
    b.jp("move_target");

    b.label("spawn_bullet");
    b.ld_reg(7, 4);
    b.add_byte(7, 1); // center of 3px ship
    b.ld_byte(8, 27);
    b.ld_byte(9, 1);
    b.jp("move_target");

    b.label("move_bullet");
    b.add_byte(8, 0xFE); // bullet moves up 2px
    b.ld_byte(0, 8);
    b.ld_reg(1, 8);
    // Deactivate bullet if bullet_y <= 8
    b.emit(0x8015); // SUB V0, V1 -> VF = 1 if 8 >= bullet_y
    b.se_byte(0xF, 1);
    b.jp("move_target");
    b.ld_byte(9, 0);

    b.label("move_target");
    b.add_reg(5, 6);
    b.se_byte(5, 2);
    b.jp("target_right_chk");
    b.ld_byte(6, 1);
    b.jp("redraw");

    b.label("target_right_chk");
    b.se_byte(5, 56);
    b.jp("redraw");
    b.ld_byte(6, 0xFF);

    b.label("redraw");
    b.call("draw_ship");
    b.call("draw_target");
    b.se_byte(9, 1);
    b.jp("pace_frame");
    b.call("draw_bullet");
    // If bullet collided with target (VF == 1), score!
    b.se_byte(0xF, 1);
    b.jp("pace_frame");

    // Hit! Erase bullet and old target, update score, respawn target
    b.call("draw_bullet");
    b.call("draw_target");
    b.ld_byte(9, 0);
    b.call("draw_score");
    b.add_byte(0xA, 1);
    b.sne_byte(0xA, 100);
    b.ld_byte(0xA, 0);
    b.call("draw_score");
    b.rnd(5, 0x2F);
    b.add_byte(5, 4);
    b.call("draw_target");
    b.ld_byte(0, 3);
    b.ld_st(0);

    b.label("pace_frame");
    b.ld_byte(0, 1);
    b.ld_dt(0);
    b.label("wait_tick");
    b.ld_reg_dt(0);
    b.se_byte(0, 0);
    b.jp("wait_tick");
    b.jp("loop");

    b.label("draw_ship");
    b.ld_i("ship_sprite");
    b.ld_byte(0, 29);
    b.drw(4, 0, 2);
    b.ret();

    b.label("draw_target");
    b.ld_i("target_sprite");
    b.ld_byte(0, 8);
    b.drw(5, 0, 2);
    b.ret();

    b.label("draw_bullet");
    b.ld_i("bullet_sprite");
    b.drw(7, 8, 2);
    b.ret();

    b.label("draw_score");
    b.ld_i("bcd_buf");
    b.ld_bcd(0xA);
    b.ld_regs_from_i(2);
    b.ld_reg(3, 2);
    b.ld_byte(0, 27);
    b.ld_byte(2, 1);
    b.ld_font(1);
    b.drw(0, 2, 5);
    b.ld_byte(0, 33);
    b.ld_font(3);
    b.drw(0, 2, 5);
    b.ret();

    b.label("ship_sprite");
    b.data(&[0x40, 0xE0]);

    b.label("target_sprite");
    b.data(&[0xF0, 0x90]);

    b.label("bullet_sprite");
    b.data(&[0x80, 0x80]);

    b.label("bcd_buf");
    b.data(&[0x00, 0x00, 0x00, 0x00]);

    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{format_opcode, Cpu};

    #[test]
    fn all_builtin_roms_run_deterministically_and_render_pixels() {
        for rom_id in RomId::ALL {
            let rom = rom_id.build();
            assert!(!rom.is_empty(), "{:?} ROM should not be empty", rom_id);

            let mut cpu_a = Cpu::new();
            cpu_a.load_rom(&rom);

            let mut cpu_b = Cpu::new();
            cpu_b.load_rom(&rom);

            for cycle in 0..500 {
                cpu_a.step();
                cpu_b.step();
                if cycle % 10 == 0 {
                    cpu_a.tick_timers();
                    cpu_b.tick_timers();
                }
                let disasm = format_opcode(cpu_a.last_opcode);
                assert!(
                    !disasm.starts_with("DW "),
                    "{:?} executed unknown opcode 0x{:04X} at cycle {}",
                    rom_id,
                    cpu_a.last_opcode,
                    cycle
                );
            }

            assert_eq!(cpu_a.cycles, 500);
            assert_eq!(cpu_a.pc, cpu_b.pc);
            assert_eq!(cpu_a.v, cpu_b.v);
            assert_eq!(cpu_a.display, cpu_b.display);
            assert!(
                cpu_a.active_pixels() > 0,
                "{:?} should draw active pixels within 500 cycles",
                rom_id
            );
            assert!(
                cpu_a.heat.iter().any(|&h| h > 0),
                "{:?} should produce RAM heat activity",
                rom_id
            );
        }
    }
}
