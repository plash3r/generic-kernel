use crate::{
    arch::{
        apic,
        graphics::{Color, Display, Point, Rect, TextStyle},
        keyboard::{Key, Keyboard},
        mouse, timer,
    },
    vfs,
};
use alloc::vec::Vec;
use kernel_core::vfs::DirEntry;

const TOP_BAR: i32 = 32;
const TASKBAR_HEIGHT: i32 = 44;
const TITLE_HEIGHT: i32 = 30;

const DESKTOP_BG: Color = Color::rgb(17, 27, 47);
const DESKTOP_ACCENT: Color = Color::rgb(69, 161, 255);
const PANEL: Color = Color::rgb(24, 35, 56);
const PANEL_LIGHT: Color = Color::rgb(38, 53, 79);
const WINDOW_BG: Color = Color::rgb(29, 42, 64);
const WINDOW_BORDER: Color = Color::rgb(70, 91, 121);
const TEXT: Color = Color::rgb(232, 238, 247);
const TEXT_MUTED: Color = Color::rgb(164, 178, 199);
const CLOSE: Color = Color::rgb(196, 76, 76);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopExit {
    Console,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowKind {
    Welcome,
    Kernel,
    Files,
}

#[derive(Clone, Copy)]
struct Window {
    kind: WindowKind,
    title: &'static str,
    rect: Rect,
    visible: bool,
    minimized: bool,
}

struct Desktop {
    windows: [Window; 3],
    z_order: [usize; 3],
    cursor: Point,
    buttons: u8,
    dragging: Option<(usize, i32, i32)>,
    start_menu: bool,
    files: Vec<DirEntry>,
    width: i32,
    height: i32,
    last_second: u64,
}

pub fn smoke(mut display: Display<'_>) {
    let desktop = Desktop::new(display.width(), display.height());
    desktop.draw(&mut display);
    let checksum = display.checksum();
    let bytes = display.backbuffer_bytes();
    let width = display.width();
    let height = display.height();
    display.present();
    crate::log!(
        "[ok] desktop compositor {}x{}, backbuffer={} KiB, checksum={:#x}\n",
        width,
        height,
        bytes / 1024,
        checksum
    );
}

pub fn run(mut display: Display<'_>) -> DesktopExit {
    let mut keyboard = Keyboard::new();
    keyboard.drain();
    mouse::drain();

    let mut desktop = Desktop::new(display.width(), display.height());
    desktop.draw(&mut display);
    display.present();

    let mut dirty = false;
    let mut last_present_tick = timer::ticks();

    loop {
        while let Some(key) = keyboard.poll_key() {
            match key {
                Key::F12 => return DesktopExit::Console,
                Key::Escape => {
                    desktop.start_menu = false;
                    dirty = true;
                }
                Key::ArrowLeft => {
                    desktop.cursor.x = (desktop.cursor.x - 8).max(0);
                    dirty = true;
                }
                Key::ArrowRight => {
                    desktop.cursor.x = (desktop.cursor.x + 8).min(desktop.width - 1);
                    dirty = true;
                }
                Key::ArrowUp => {
                    desktop.cursor.y = (desktop.cursor.y - 8).max(0);
                    dirty = true;
                }
                Key::ArrowDown => {
                    desktop.cursor.y = (desktop.cursor.y + 8).min(desktop.height - 1);
                    dirty = true;
                }
                Key::Char(b'1') => {
                    desktop.show_window(0);
                    dirty = true;
                }
                Key::Char(b'2') => {
                    desktop.show_window(1);
                    dirty = true;
                }
                Key::Char(b'3') => {
                    desktop.show_window(2);
                    dirty = true;
                }
                _ => {}
            }
        }

        while let Some(event) = mouse::poll_event() {
            if let Some(exit) = desktop.handle_mouse(event) {
                return exit;
            }
            dirty = true;
        }

        let tick = timer::ticks();
        let second = tick / timer::HZ;
        if second != desktop.last_second {
            desktop.last_second = second;
            dirty = true;
        }

        if dirty && tick != last_present_tick {
            desktop.draw(&mut display);
            display.present();
            last_present_tick = tick;
            dirty = false;
        }

        x86_64::instructions::hlt();
    }
}

impl Desktop {
    fn new(width: i32, height: i32) -> Self {
        let usable_height = (height - TOP_BAR - TASKBAR_HEIGHT).max(240);
        let welcome_width = (width * 55 / 100).clamp(420, 680);
        let welcome_height = (usable_height * 58 / 100).clamp(280, 430);
        let monitor_width = (width * 42 / 100).clamp(360, 540);
        let monitor_height = (usable_height * 50 / 100).clamp(260, 390);
        let files_width = (width * 46 / 100).clamp(390, 620);
        let files_height = (usable_height * 54 / 100).clamp(280, 420);

        let windows = [
            Window {
                kind: WindowKind::Welcome,
                title: "Welcome to Generic",
                rect: Rect::new(
                    (width - welcome_width) / 2,
                    TOP_BAR + 48,
                    welcome_width,
                    welcome_height,
                ),
                visible: true,
                minimized: false,
            },
            Window {
                kind: WindowKind::Kernel,
                title: "Kernel Monitor",
                rect: Rect::new(
                    (width - monitor_width - 44).max(40),
                    TOP_BAR + 86,
                    monitor_width,
                    monitor_height,
                ),
                visible: true,
                minimized: false,
            },
            Window {
                kind: WindowKind::Files,
                title: "Files",
                rect: Rect::new(54, TOP_BAR + 112, files_width, files_height),
                visible: true,
                minimized: true,
            },
        ];

        Self {
            windows,
            z_order: [2, 0, 1],
            cursor: Point {
                x: width / 2,
                y: height / 2,
            },
            buttons: 0,
            dragging: None,
            start_menu: false,
            files: vfs::read_dir("/").unwrap_or_default(),
            width,
            height,
            last_second: timer::ticks() / timer::HZ,
        }
    }

    fn draw(&self, display: &mut Display<'_>) {
        display.clear(DESKTOP_BG);

        self.draw_wallpaper(display);
        self.draw_top_bar(display);

        for index in self.z_order {
            let window = self.windows[index];
            if window.visible && !window.minimized {
                self.draw_window(display, window);
            }
        }

        self.draw_taskbar(display);
        if self.start_menu {
            self.draw_start_menu(display);
        }
        self.draw_cursor(display);
    }

    fn draw_wallpaper(&self, display: &mut Display<'_>) {
        let center_x = self.width / 2;
        let center_y = (self.height - TASKBAR_HEIGHT + TOP_BAR) / 2;
        for radius in [210, 160, 110] {
            let shade = match radius {
                210 => Color::rgb(21, 38, 66),
                160 => Color::rgb(24, 48, 80),
                _ => Color::rgb(28, 58, 94),
            };
            display.fill_rect(
                Rect::new(center_x - radius, center_y - radius / 2, radius * 2, radius),
                shade,
            );
        }
        display.text(
            36,
            self.height - TASKBAR_HEIGHT - 34,
            "Generic OS  |  kernel desktop prototype",
            TEXT_MUTED,
            TextStyle::Regular16,
        );
    }

    fn draw_top_bar(&self, display: &mut Display<'_>) {
        display.fill_rect(Rect::new(0, 0, self.width, TOP_BAR), PANEL);
        display.text(14, 7, "Generic", TEXT, TextStyle::Bold16);
        let seconds = timer::millis() / 1000;
        let text_x = (self.width - 240).max(180);
        display.text(
            text_x,
            7,
            "F12: text console",
            TEXT_MUTED,
            TextStyle::Regular16,
        );
        draw_number(display, self.width - 70, 7, seconds, TEXT);
        display.text(self.width - 30, 7, "s", TEXT_MUTED, TextStyle::Regular16);
    }

    fn draw_taskbar(&self, display: &mut Display<'_>) {
        let y = self.height - TASKBAR_HEIGHT;
        display.fill_rect(Rect::new(0, y, self.width, TASKBAR_HEIGHT), PANEL);
        display.fill_rect(self.start_button(), DESKTOP_ACCENT);
        display.text(20, y + 13, "Generic", Color::WHITE, TextStyle::Bold16);

        display.fill_rect(self.console_button(), PANEL_LIGHT);
        display.text(112, y + 13, "Console", TEXT, TextStyle::Regular16);

        for (slot, index) in self.z_order.iter().copied().enumerate() {
            let window = self.windows[index];
            let rect = self.task_button(slot);
            let color = if window.visible && !window.minimized {
                Color::rgb(48, 69, 100)
            } else {
                PANEL_LIGHT
            };
            display.fill_rect(rect, color);
            display.text(
                rect.x + 10,
                rect.y + 10,
                window.title,
                TEXT,
                TextStyle::Regular16,
            );
        }
    }

    fn draw_start_menu(&self, display: &mut Display<'_>) {
        let menu = self.start_menu_rect();
        display.fill_rect(menu, WINDOW_BG);
        display.stroke_rect(menu, WINDOW_BORDER, 2);
        display.text(
            menu.x + 16,
            menu.y + 14,
            "Generic",
            Color::WHITE,
            TextStyle::Bold20,
        );
        display.text(
            menu.x + 16,
            menu.y + 42,
            "Kernel desktop",
            TEXT_MUTED,
            TextStyle::Regular16,
        );

        let labels = ["Welcome", "Kernel Monitor", "Files", "Text Console"];
        for (index, label) in labels.iter().enumerate() {
            let item = self.start_item(index);
            display.fill_rect(item, if index == 3 { PANEL_LIGHT } else { PANEL });
            display.text(item.x + 12, item.y + 9, label, TEXT, TextStyle::Regular16);
        }
    }

    fn draw_window(&self, display: &mut Display<'_>, window: Window) {
        let shadow = Rect::new(
            window.rect.x + 6,
            window.rect.y + 7,
            window.rect.width,
            window.rect.height,
        );
        display.fill_rect(shadow, Color::rgb(10, 15, 25));
        display.fill_rect(window.rect, WINDOW_BG);
        display.stroke_rect(window.rect, WINDOW_BORDER, 2);

        let title = self.title_rect(window.rect);
        display.fill_rect(title, Color::rgb(39, 58, 88));
        display.text(
            title.x + 12,
            title.y + 7,
            window.title,
            TEXT,
            TextStyle::Bold16,
        );

        let close = self.close_button(window.rect);
        display.fill_rect(close, CLOSE);
        display.text(
            close.x + 8,
            close.y + 5,
            "x",
            Color::WHITE,
            TextStyle::Bold16,
        );

        let minimize = self.minimize_button(window.rect);
        display.fill_rect(minimize, PANEL_LIGHT);
        display.text(
            minimize.x + 8,
            minimize.y + 3,
            "-",
            Color::WHITE,
            TextStyle::Bold16,
        );

        match window.kind {
            WindowKind::Welcome => self.draw_welcome(display, window.rect),
            WindowKind::Kernel => self.draw_kernel_monitor(display, window.rect),
            WindowKind::Files => self.draw_files(display, window.rect),
        }
    }

    fn draw_welcome(&self, display: &mut Display<'_>, rect: Rect) {
        let x = rect.x + 24;
        let mut y = rect.y + TITLE_HEIGHT + 24;
        display.text(
            x,
            y,
            "Generic OS",
            Color::rgb(100, 188, 255),
            TextStyle::Bold20,
        );
        y += 34;
        for line in [
            "The first Generic graphical desktop is running.",
            "Rendering uses a kernel-owned double buffer.",
            "Windows can be focused, dragged, minimized and closed.",
            "Mouse and keyboard input arrive through IRQ event queues.",
            "",
            "Shortcuts:",
            "  F12  switch to the text console",
            "  1/2/3 restore Welcome / Kernel / Files",
        ] {
            display.text(x, y, line, TEXT, TextStyle::Regular16);
            y += 22;
        }
    }

    fn draw_kernel_monitor(&self, display: &mut Display<'_>, rect: Rect) {
        let stats = crate::mm::stats();
        let apic = apic::diagnostics();
        let mounts = vfs::mounts();
        let x = rect.x + 20;
        let mut y = rect.y + TITLE_HEIGHT + 20;

        display.text(x, y, "Live kernel state", TEXT, TextStyle::Bold16);
        y += 28;
        display.text(x, y, "Memory free:", TEXT_MUTED, TextStyle::Regular16);
        draw_number(
            display,
            x + 160,
            y,
            stats.physical_free / (1024 * 1024),
            TEXT,
        );
        display.text(x + 202, y, "MiB", TEXT_MUTED, TextStyle::Regular16);
        y += 22;
        display.text(x, y, "Heap free:", TEXT_MUTED, TextStyle::Regular16);
        draw_number(display, x + 160, y, stats.heap_free as u64 / 1024, TEXT);
        display.text(x + 202, y, "KiB", TEXT_MUTED, TextStyle::Regular16);
        y += 22;
        display.text(x, y, "Timer ticks:", TEXT_MUTED, TextStyle::Regular16);
        draw_number(display, x + 160, y, timer::ticks(), TEXT);
        y += 22;
        display.text(x, y, "Local APIC:", TEXT_MUTED, TextStyle::Regular16);
        draw_number(display, x + 160, y, apic.local_apic_id as u64, TEXT);
        y += 22;
        display.text(x, y, "IOAPICs:", TEXT_MUTED, TextStyle::Regular16);
        draw_number(display, x + 160, y, apic.io_apic_count as u64, TEXT);
        y += 22;
        display.text(x, y, "Mounts:", TEXT_MUTED, TextStyle::Regular16);
        draw_number(display, x + 160, y, mounts.len() as u64, TEXT);
        y += 22;
        display.text(x, y, "Recontrol ABI:", TEXT_MUTED, TextStyle::Regular16);
        draw_number(display, x + 160, y, crate::recontrol::probe() as u64, TEXT);
    }

    fn draw_files(&self, display: &mut Display<'_>, rect: Rect) {
        let x = rect.x + 20;
        let mut y = rect.y + TITLE_HEIGHT + 18;
        display.text(x, y, "Root filesystem /", TEXT, TextStyle::Bold16);
        y += 30;

        for entry in self.files.iter().take(12) {
            let marker = match entry.metadata.kind {
                kernel_core::vfs::NodeKind::Directory => "[DIR]",
                kernel_core::vfs::NodeKind::File => "[FILE]",
            };
            display.text(x, y, marker, DESKTOP_ACCENT, TextStyle::Regular16);
            display.text(x + 62, y, &entry.name, TEXT, TextStyle::Regular16);
            y += 22;
            if y > rect.y + rect.height - 24 {
                break;
            }
        }
    }

    fn draw_cursor(&self, display: &mut Display<'_>) {
        let x = self.cursor.x;
        let y = self.cursor.y;
        for row in 0..16 {
            let width = (row / 2 + 2).min(9);
            display.fill_rect(
                Rect::new(x, y + row, width, 1),
                if row == 0 || row == 15 {
                    Color::BLACK
                } else {
                    Color::WHITE
                },
            );
        }
        display.line(
            Point { x, y },
            Point {
                x: x + 8,
                y: y + 15,
            },
            Color::BLACK,
        );
        display.line(Point { x, y }, Point { x, y: y + 15 }, Color::BLACK);
    }

    fn handle_mouse(&mut self, event: mouse::MouseEvent) -> Option<DesktopExit> {
        self.cursor.x = (self.cursor.x + event.dx as i32).clamp(0, self.width - 1);
        self.cursor.y = (self.cursor.y + event.dy as i32).clamp(0, self.height - 1);

        let left_before = self.buttons & 1 != 0;
        let left_now = event.buttons & 1 != 0;
        self.buttons = event.buttons;

        if left_now {
            if let Some((window_index, offset_x, offset_y)) = self.dragging {
                let mut rect = self.windows[window_index].rect;
                rect.x = (self.cursor.x - offset_x).clamp(-rect.width + 80, self.width - 80);
                rect.y = (self.cursor.y - offset_y)
                    .clamp(TOP_BAR, self.height - TASKBAR_HEIGHT - TITLE_HEIGHT);
                self.windows[window_index].rect = rect;
            }
        }

        if left_now && !left_before {
            return self.mouse_down();
        }
        if !left_now && left_before {
            self.dragging = None;
        }
        None
    }

    fn mouse_down(&mut self) -> Option<DesktopExit> {
        let point = self.cursor;

        if self.start_menu {
            for index in 0..4 {
                if self.start_item(index).contains(point) {
                    self.start_menu = false;
                    if index == 3 {
                        return Some(DesktopExit::Console);
                    }
                    self.show_window(index);
                    return None;
                }
            }
            if !self.start_menu_rect().contains(point) && !self.start_button().contains(point) {
                self.start_menu = false;
            }
        }

        if self.start_button().contains(point) {
            self.start_menu = !self.start_menu;
            return None;
        }
        if self.console_button().contains(point) {
            return Some(DesktopExit::Console);
        }

        for slot in 0..self.z_order.len() {
            if self.task_button(slot).contains(point) {
                let index = self.z_order[slot];
                self.windows[index].visible = true;
                self.windows[index].minimized = false;
                self.bring_front(index);
                return None;
            }
        }

        for index in self.z_order.into_iter().rev() {
            let window = self.windows[index];
            if !window.visible || window.minimized || !window.rect.contains(point) {
                continue;
            }

            self.bring_front(index);
            let rect = self.windows[index].rect;
            if self.close_button(rect).contains(point) {
                self.windows[index].visible = false;
                self.dragging = None;
                return None;
            }
            if self.minimize_button(rect).contains(point) {
                self.windows[index].minimized = true;
                self.dragging = None;
                return None;
            }
            if self.title_rect(rect).contains(point) {
                self.dragging = Some((index, point.x - rect.x, point.y - rect.y));
            }
            return None;
        }

        None
    }

    fn show_window(&mut self, index: usize) {
        if index >= self.windows.len() {
            return;
        }
        self.windows[index].visible = true;
        self.windows[index].minimized = false;
        self.bring_front(index);
    }

    fn bring_front(&mut self, index: usize) {
        let Some(position) = self
            .z_order
            .iter()
            .position(|candidate| *candidate == index)
        else {
            return;
        };
        for slot in position..self.z_order.len() - 1 {
            self.z_order[slot] = self.z_order[slot + 1];
        }
        self.z_order[self.z_order.len() - 1] = index;
    }

    fn title_rect(&self, window: Rect) -> Rect {
        Rect::new(window.x + 2, window.y + 2, window.width - 4, TITLE_HEIGHT)
    }

    fn close_button(&self, window: Rect) -> Rect {
        Rect::new(window.x + window.width - 31, window.y + 5, 24, 22)
    }

    fn minimize_button(&self, window: Rect) -> Rect {
        Rect::new(window.x + window.width - 59, window.y + 5, 24, 22)
    }

    fn start_button(&self) -> Rect {
        Rect::new(8, self.height - TASKBAR_HEIGHT + 7, 86, 30)
    }

    fn console_button(&self) -> Rect {
        Rect::new(102, self.height - TASKBAR_HEIGHT + 7, 92, 30)
    }

    fn task_button(&self, slot: usize) -> Rect {
        let width = ((self.width - 212) / 3).clamp(100, 190);
        Rect::new(
            202 + slot as i32 * (width + 5),
            self.height - TASKBAR_HEIGHT + 7,
            width,
            30,
        )
    }

    fn start_menu_rect(&self) -> Rect {
        Rect::new(8, self.height - TASKBAR_HEIGHT - 226, 250, 218)
    }

    fn start_item(&self, index: usize) -> Rect {
        let menu = self.start_menu_rect();
        Rect::new(
            menu.x + 10,
            menu.y + 74 + index as i32 * 34,
            menu.width - 20,
            30,
        )
    }
}

fn draw_number(display: &mut Display<'_>, x: i32, y: i32, value: u64, color: Color) {
    let mut buffer = [0u8; 24];
    let text = decimal(value, &mut buffer);
    display.text(x, y, text, color, TextStyle::Regular16);
}

fn decimal(mut value: u64, buffer: &mut [u8; 24]) -> &str {
    if value == 0 {
        return "0";
    }
    let mut index = buffer.len();
    while value != 0 && index != 0 {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    core::str::from_utf8(&buffer[index..]).unwrap_or("?")
}
