use crate::arch::{
    framebuffer::Console,
    keyboard::{Key, Keyboard},
};
use core::fmt::Write;

const MAX_LINE: usize = 128;

#[derive(Clone, Copy)]
pub struct SystemStats {
    pub usable_regions: usize,
    pub usable_bytes: u64,
}

pub fn run(mut console: Console<'_>, stats: SystemStats) -> ! {
    let mut keyboard = Keyboard::new();
    keyboard.drain();

    banner(&mut console);

    loop {
        console.set_accent_color();
        let _ = write!(console, "generic> ");
        console.set_default_color();

        let mut line = [0u8; MAX_LINE];
        let mut len = 0usize;

        loop {
            match keyboard.read_key_blocking() {
                Key::Char(byte) if len < line.len() => {
                    line[len] = byte;
                    len += 1;
                    console.write_byte(byte);
                }
                Key::Char(_) => {}
                Key::Backspace if len > 0 => {
                    len -= 1;
                    console.backspace();
                }
                Key::Backspace => {}
                Key::Enter => {
                    console.write_byte(b'\n');
                    break;
                }
            }
        }

        let input = core::str::from_utf8(&line[..len]).unwrap_or("");
        execute(&mut console, input, stats);
    }
}

fn banner(console: &mut Console<'_>) {
    console.clear();
    console.set_accent_color();
    let _ = writeln!(console, "GENERIC OS 0.1.0");
    console.set_default_color();
    let _ = writeln!(console, "Interactive framebuffer console");
    let _ = writeln!(console, "Type HELP to list commands.");
    let _ = writeln!(console);
}

fn execute(console: &mut Console<'_>, input: &str, stats: SystemStats) {
    let input = input.trim();
    if input.is_empty() {
        return;
    }

    let (command, args) = input
        .split_once(' ')
        .map(|(command, args)| (command, args.trim_start()))
        .unwrap_or((input, ""));

    if command.eq_ignore_ascii_case("help") {
        let _ = writeln!(console, "Commands:");
        let _ = writeln!(console, "  HELP       show this list");
        let _ = writeln!(console, "  CLEAR      clear the screen");
        let _ = writeln!(console, "  ECHO TEXT  print text");
        let _ = writeln!(console, "  UNAME      kernel and architecture");
        let _ = writeln!(console, "  VERSION    Generic version");
        let _ = writeln!(console, "  MEM        usable boot memory");
        let _ = writeln!(console, "  VIDEO      framebuffer information");
        let _ = writeln!(console, "  RECONTROL  call Recontrol code");
        let _ = writeln!(console, "  WHOAMI     current execution context");
        let _ = writeln!(console, "  PWD        current namespace path");
        let _ = writeln!(console, "  LS         filesystem status");
        let _ = writeln!(console, "  REBOOT     reset the virtual machine");
        let _ = writeln!(console, "  HALT       stop the CPU");
    } else if command.eq_ignore_ascii_case("clear") {
        console.clear();
    } else if command.eq_ignore_ascii_case("echo") {
        let _ = writeln!(console, "{args}");
    } else if command.eq_ignore_ascii_case("uname") {
        let _ = writeln!(console, "Generic 0.1.0 x86_64");
    } else if command.eq_ignore_ascii_case("version") {
        let _ = writeln!(console, "Generic OS kernel 0.1.0");
    } else if command.eq_ignore_ascii_case("mem") {
        let mib = stats.usable_bytes / (1024 * 1024);
        let _ = writeln!(
            console,
            "Usable memory: {} MiB in {} regions",
            mib, stats.usable_regions
        );
    } else if command.eq_ignore_ascii_case("video") {
        let _ = writeln!(
            console,
            "Framebuffer: {}x{} pixels",
            console.width(),
            console.height()
        );
    } else if command.eq_ignore_ascii_case("recontrol") {
        let value = crate::recontrol::probe();
        let _ = writeln!(console, "Recontrol ABI probe returned {value}");
    } else if command.eq_ignore_ascii_case("whoami") {
        let _ = writeln!(console, "root (kernel console)");
    } else if command.eq_ignore_ascii_case("pwd") {
        let _ = writeln!(console, "/");
    } else if command.eq_ignore_ascii_case("ls") {
        let _ = writeln!(console, "VFS is not implemented yet.");
    } else if command.eq_ignore_ascii_case("reboot") {
        let _ = writeln!(console, "Rebooting...");
        crate::arch::reboot();
    } else if command.eq_ignore_ascii_case("halt") || command.eq_ignore_ascii_case("shutdown") {
        let _ = writeln!(console, "CPU halted.");
        crate::arch::halt();
    } else {
        let _ = writeln!(console, "generic: unknown command: {command}");
        let _ = writeln!(console, "Type HELP for available commands.");
    }
}
