#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}

#[cfg(not(target_arch = "wasm32"))]
mod app {
    use std::io;
    use std::time::{Duration, Instant};

    use chip8_xray::cpu::{format_opcode, Cpu, DISPLAY_HEIGHT, DISPLAY_WIDTH, MEMORY_SIZE};
    use chip8_xray::roms::RomId;
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::execute;
    use crossterm::terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};
    use ratatui::{Frame, Terminal};

    struct CliOptions {
        headless: bool,
        cycles: u64,
        rom: RomId,
        help: bool,
    }

    impl CliOptions {
        fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
            let mut opts = Self {
                headless: false,
                cycles: 500,
                rom: RomId::Bounce,
                help: false,
            };

            let mut iter = args.peekable();
            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--help" | "-h" => opts.help = true,
                    "--headless" => opts.headless = true,
                    "--cycles" => {
                        let val = iter
                            .next()
                            .ok_or_else(|| String::from("--cycles requires a positive integer"))?;
                        opts.cycles = val
                            .parse::<u64>()
                            .map_err(|_| format!("invalid --cycles value: {val}"))?;
                    }
                    "--rom" => {
                        let val = iter
                            .next()
                            .ok_or_else(|| String::from("--rom requires 1, 2, 3, or a name"))?;
                        opts.rom = parse_rom_arg(&val)?;
                    }
                    other => return Err(format!("unknown argument: {other}")),
                }
            }
            Ok(opts)
        }
    }

    fn parse_rom_arg(val: &str) -> Result<RomId, String> {
        match val.to_ascii_lowercase().as_str() {
            "1" | "bounce" => Ok(RomId::Bounce),
            "2" | "maze" => Ok(RomId::Maze),
            "3" | "shooter" => Ok(RomId::Shooter),
            _ => Err(format!(
                "unknown ROM '{val}' (expected 1|2|3|bounce|maze|shooter)"
            )),
        }
    }

    struct TermGuard;

    impl Drop for TermGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
        }
    }

    struct DebuggerState {
        cpu: Cpu,
        rom_id: RomId,
        paused: bool,
        cycles_per_tick: u32,
        key_decay: [u8; 16],
    }

    impl DebuggerState {
        fn new(rom_id: RomId) -> Self {
            let mut cpu = Cpu::new();
            cpu.load_rom(&rom_id.build());
            Self {
                cpu,
                rom_id,
                paused: false,
                cycles_per_tick: 10,
                key_decay: [0u8; 16],
            }
        }

        fn switch_rom(&mut self, rom_id: RomId) {
            self.rom_id = rom_id;
            self.cpu.load_rom(&rom_id.build());
            self.key_decay.fill(0);
        }

        fn reset(&mut self) {
            self.cpu.load_rom(&self.rom_id.build());
            self.key_decay.fill(0);
        }

        fn press_key_pulse(&mut self, chip8_key: u8) {
            let idx = usize::from(chip8_key & 0x0F);
            self.key_decay[idx] = 8;
            self.cpu.set_key(chip8_key, true);
        }

        fn decay_keys(&mut self) {
            for (idx, timer) in self.key_decay.iter_mut().enumerate() {
                if *timer > 0 {
                    *timer -= 1;
                    if *timer == 0 {
                        self.cpu.set_key(idx as u8, false);
                    }
                }
            }
        }
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let opts = match CliOptions::parse(std::env::args().skip(1)) {
            Ok(o) => o,
            Err(err) => {
                eprintln!("error: {err}");
                print_help();
                std::process::exit(2);
            }
        };

        if opts.help {
            print_help();
            return Ok(());
        }

        if opts.headless {
            run_headless(opts.rom, opts.cycles);
            return Ok(());
        }

        run_tui(opts.rom)
    }

    fn print_help() {
        println!(
            "chip8-xray - CHIP-8 emulator with live CPU X-Ray debugger\n\n\
             USAGE:\n  \
               chip8-xray [--rom 1|2|3] [--headless --cycles N]\n\n\
             OPTIONS:\n  \
               --rom <1|2|3|bounce|maze|shooter>  Select initial built-in ROM (default: 1)\n  \
               --headless                         Run without TUI and print X-Ray state\n  \
               --cycles <N>                       Cycles to execute in headless mode (default: 500)\n  \
               -h, --help                         Show this help message"
        );
    }

    fn run_headless(rom_id: RomId, cycles: u64) {
        let mut cpu = Cpu::new();
        cpu.load_rom(&rom_id.build());

        for cycle in 0..cycles {
            cpu.step();
            if cycle % 10 == 0 {
                cpu.tick_timers();
            }
        }

        let next_op = cpu.peek_opcode(cpu.pc);
        let hot_bytes = cpu.heat.iter().filter(|&&h| h > 0).count();
        println!(
            "chip8-xray headless ok | rom={} | cycles={} | pc=0x{:03X} ({}) | i=0x{:03X} | sp={} | active_px={} | hot_ram_bytes={}",
            rom_id.name(),
            cpu.cycles,
            cpu.pc,
            format_opcode(next_op),
            cpu.i,
            cpu.sp,
            cpu.active_pixels(),
            hot_bytes
        );
        println!(
            "V0..V7 : {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
            cpu.v[0], cpu.v[1], cpu.v[2], cpu.v[3], cpu.v[4], cpu.v[5], cpu.v[6], cpu.v[7]
        );
        println!(
            "V8..VF : {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
            cpu.v[8], cpu.v[9], cpu.v[10], cpu.v[11], cpu.v[12], cpu.v[13], cpu.v[14], cpu.v[15]
        );
    }

    fn run_tui(initial_rom: RomId) -> Result<(), Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let _guard = TermGuard;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let mut state = DebuggerState::new(initial_rom);

        let frame_interval = Duration::from_millis(16);
        let mut last_tick = Instant::now();

        loop {
            terminal.draw(|frame| draw_ui(frame, &state))?;

            let timeout = frame_interval.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => break,
                            KeyCode::Char(' ') => state.paused = !state.paused,
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                state.paused = true;
                                state.cpu.step();
                            }
                            KeyCode::Char('r') | KeyCode::Char('R') => state.reset(),
                            KeyCode::Char('1') => state.switch_rom(RomId::Bounce),
                            KeyCode::Char('2') => state.switch_rom(RomId::Maze),
                            KeyCode::Char('3') => state.switch_rom(RomId::Shooter),
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                state.cycles_per_tick = (state.cycles_per_tick + 2).min(40);
                            }
                            KeyCode::Char('-') | KeyCode::Char('_') => {
                                state.cycles_per_tick =
                                    state.cycles_per_tick.saturating_sub(2).max(1);
                            }
                            // Gamepad bindings for Bounce & Shooter
                            KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                                state.press_key_pulse(0x1);
                            }
                            KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => {
                                state.press_key_pulse(0x4);
                            }
                            KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                                state.press_key_pulse(0x7);
                            }
                            KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                                state.press_key_pulse(0x9);
                            }
                            KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Enter => {
                                state.press_key_pulse(0x5);
                            }
                            _ => {}
                        }
                    }
                }
            }

            if last_tick.elapsed() >= frame_interval {
                if !state.paused {
                    for _ in 0..state.cycles_per_tick {
                        state.cpu.step();
                    }
                    state.cpu.tick_timers();
                }
                state.decay_keys();
                last_tick = Instant::now();
            }
        }

        // Explicit terminal restore before TermGuard drop ensures clean flush.
        drop(terminal);
        Ok(())
    }

    fn draw_ui(frame: &mut Frame, state: &DebuggerState) {
        let area = frame.area();
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(18),
                Constraint::Length(3),
            ])
            .split(area);

        draw_header(frame, outer[0], state);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(68), Constraint::Min(32)])
            .split(outer[1]);

        let left_col = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(18), Constraint::Min(8)])
            .split(body[0]);

        let right_col = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(13), Constraint::Min(8)])
            .split(body[1]);

        draw_display(frame, left_col[0], &state.cpu);
        draw_ram_heatmap(frame, left_col[1], &state.cpu);
        draw_registers_and_stack(frame, right_col[0], &state.cpu);
        draw_disassembler(frame, right_col[1], &state.cpu);
        draw_footer(frame, outer[2], state);
    }

    fn draw_header(frame: &mut Frame, area: Rect, state: &DebuggerState) {
        let status_span = if state.paused {
            Span::styled(
                " PAUSED ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                " RUNNING ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        };

        let line = Line::from(vec![
            Span::styled(
                " CHIP-8 X-RAY ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" | "),
            status_span,
            Span::raw(format!(
                " | ROM: {} | Cycles: {} | Speed: {} cyc/tick | Pixels: {}",
                state.rom_id.name(),
                state.cpu.cycles,
                state.cycles_per_tick,
                state.cpu.active_pixels()
            )),
        ]);

        let header = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(header, area);
    }

    fn draw_display(frame: &mut Frame, area: Rect, cpu: &Cpu) {
        let on_color = Color::Rgb(110, 255, 160);
        let off_color = Color::Rgb(10, 16, 20);
        let lines = render_halfblock_lines(&cpu.display, on_color, off_color);
        let screen = Paragraph::new(lines).block(
            Block::default()
                .title(" Display (64x32 Half-Block) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(screen, area);
    }

    pub(crate) fn render_halfblock_lines(
        display: &[u8],
        on_color: Color,
        off_color: Color,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(DISPLAY_HEIGHT / 2);
        for row_pair in 0..(DISPLAY_HEIGHT / 2) {
            let y_top = row_pair * 2;
            let y_bot = y_top + 1;
            let mut spans = Vec::with_capacity(DISPLAY_WIDTH);
            for x in 0..DISPLAY_WIDTH {
                let top = display[y_top * DISPLAY_WIDTH + x] != 0;
                let bot = display[y_bot * DISPLAY_WIDTH + x] != 0;
                let fg = if top { on_color } else { off_color };
                let bg = if bot { on_color } else { off_color };
                spans.push(Span::styled("▀", Style::default().fg(fg).bg(bg)));
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    fn draw_ram_heatmap(frame: &mut Frame, area: Rect, cpu: &Cpu) {
        // Group 4096 bytes into 64 columns x 16 rows (4 bytes per cell).
        let cols = 64usize;
        let rows = 16usize;
        let bytes_per_cell = MEMORY_SIZE / (cols * rows);
        let pc_cell = usize::from(cpu.pc) / bytes_per_cell;
        let i_cell = usize::from(cpu.i) / bytes_per_cell;

        let mut lines = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut spans = Vec::with_capacity(cols);
            for c in 0..cols {
                let cell = r * cols + c;
                let start = cell * bytes_per_cell;
                let max_heat = cpu.heat[start..start + bytes_per_cell]
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0);

                if cell == pc_cell {
                    spans.push(Span::styled(
                        "P",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else if cell == i_cell {
                    spans.push(Span::styled(
                        "I",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else {
                    let (ch, color) = heat_glyph(max_heat);
                    spans.push(Span::styled(ch, Style::default().fg(color)));
                }
            }
            lines.push(Line::from(spans));
        }

        let heatmap = Paragraph::new(lines).block(
            Block::default()
                .title(" RAM Activity Heatmap (4KB | P=PC I=Index) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        );
        frame.render_widget(heatmap, area);
    }

    fn heat_glyph(heat: u8) -> (&'static str, Color) {
        match heat {
            0 => ("·", Color::Rgb(45, 52, 64)),
            1..=63 => ("░", Color::Rgb(40, 120, 150)),
            64..=140 => ("▒", Color::Rgb(70, 200, 180)),
            141..=210 => ("▓", Color::Rgb(245, 190, 65)),
            _ => ("█", Color::Rgb(255, 85, 110)),
        }
    }

    fn draw_registers_and_stack(frame: &mut Frame, area: Rect, cpu: &Cpu) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("PC: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("0x{:03X}  ", cpu.pc),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled("I: ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("0x{:03X}  ", cpu.i)),
                Span::styled("SP: ", Style::default().fg(Color::Magenta)),
                Span::raw(format!("{:02}  ", cpu.sp)),
                Span::styled("DT/ST: ", Style::default().fg(Color::Green)),
                Span::raw(format!("{:02}/{:02}", cpu.delay_timer, cpu.sound_timer)),
            ]),
            Line::from(Span::styled(
                format!(
                    "Last OP: 0x{:04X} ({})",
                    cpu.last_opcode,
                    format_opcode(cpu.last_opcode)
                ),
                Style::default().fg(Color::Gray),
            )),
            Line::raw(""),
        ];

        for row in 0..8 {
            let left = row;
            let right = row + 8;
            lines.push(Line::from(vec![
                Span::styled(format!("V{left:X}: "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("0x{:02X} ({:3})   ", cpu.v[left], cpu.v[left]),
                    if cpu.v[left] != 0 {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(
                    format!("V{right:X}: "),
                    Style::default().fg(if right == 0xF {
                        Color::Yellow
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(
                    format!("0x{:02X} ({:3})", cpu.v[right], cpu.v[right]),
                    if cpu.v[right] != 0 {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
            ]));
        }

        let stack_items: Vec<String> = cpu.stack[..usize::from(cpu.sp)]
            .iter()
            .map(|addr| format!("0x{addr:03X}"))
            .collect();
        let stack_text = if stack_items.is_empty() {
            String::from("(empty)")
        } else {
            stack_items.join(" -> ")
        };
        lines.push(Line::from(vec![
            Span::styled("Stack: ", Style::default().fg(Color::Magenta)),
            Span::raw(stack_text),
        ]));

        let panel = Paragraph::new(lines).block(
            Block::default()
                .title(" CPU Registers & Stack ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
        frame.render_widget(panel, area);
    }

    fn draw_disassembler(frame: &mut Frame, area: Rect, cpu: &Cpu) {
        let visible_rows = area.height.saturating_sub(2) as usize;
        let half = visible_rows / 2;
        let start_addr = cpu.pc.saturating_sub((half * 2) as u16) & !1;
        let count = visible_rows.max(6);

        let mut lines = Vec::with_capacity(count);
        for idx in 0..count {
            let addr = start_addr.wrapping_add((idx * 2) as u16) % (MEMORY_SIZE as u16);
            let op = cpu.peek_opcode(addr);
            let asm = format_opcode(op);
            let is_pc = addr == cpu.pc;
            let prefix = if is_pc { "▶" } else { " " };
            let style = if is_pc {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            lines.push(Line::from(Span::styled(
                format!("{prefix} 0x{addr:03X}:  {op:04X}   {asm}"),
                style,
            )));
        }

        let disasm = Paragraph::new(lines).block(
            Block::default()
                .title(" Live Disassembler ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        );
        frame.render_widget(disasm, area);
    }

    fn draw_footer(frame: &mut Frame, area: Rect, state: &DebuggerState) {
        let active_keys: Vec<String> = state
            .cpu
            .keypad
            .iter()
            .enumerate()
            .filter_map(|(k, &v)| if v != 0 { Some(format!("{k:X}")) } else { None })
            .collect();
        let keys_str = if active_keys.is_empty() {
            String::from("-")
        } else {
            active_keys.join(",")
        };

        let text = Line::from(format!(
            " [Space] Pause  [N] Step  [R] Reset  [1-3] ROM  [+/-] Speed  [W/S/A/D/F] Keys({keys_str})  [Q] Quit"
        ));
        let footer = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(footer, area);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_headless_cli_flags() {
            let opts = CliOptions::parse(
                ["--headless", "--cycles", "500", "--rom", "2"]
                    .into_iter()
                    .map(String::from),
            )
            .expect("valid CLI flags should parse");
            assert!(opts.headless);
            assert_eq!(opts.cycles, 500);
            assert_eq!(opts.rom, RomId::Maze);
        }

        #[test]
        fn halfblock_renderer_emits_sixteen_rows() {
            let mut display = [0u8; DISPLAY_WIDTH * DISPLAY_HEIGHT];
            display[0] = 1;
            let lines = render_halfblock_lines(&display, Color::White, Color::Black);
            assert_eq!(lines.len(), 16);
            assert_eq!(lines[0].spans.len(), 64);
        }
    }
}
