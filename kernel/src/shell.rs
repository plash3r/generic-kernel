use crate::arch::{
    framebuffer::{Console, FontPreset},
    keyboard::{Key, Keyboard},
};
use alloc::string::String;
use core::fmt::{self, Write};
use kernel_core::vfs::NodeKind;

const MAX_LINE: usize = 256;
const MAX_CUSTOM_FONT_BYTES: u64 = 512 * 1024;

pub fn run(mut console: Console<'_>) -> ! {
    let mut keyboard = Keyboard::new();
    keyboard.drain();
    let mut cwd = String::from("/");

    banner(&mut console);

    loop {
        console.set_accent_color();
        let _ = write!(console, "generic:{}# ", cwd);
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
                _ => {}
            }
        }

        let input = core::str::from_utf8(&line[..len]).unwrap_or("");
        execute(&mut console, &mut cwd, input);
        crate::task::checkpoint();
    }
}

fn banner(console: &mut Console<'_>) {
    console.clear();
    console.set_accent_color();
    let _ = writeln!(console, "GENERIC OS 0.1.0");
    console.set_default_color();
    let _ = writeln!(console, "Interactive framebuffer console + VFS");
    let _ = writeln!(console, "Type HELP to list commands.");
    let _ = writeln!(console);
}

fn execute(console: &mut Console<'_>, cwd: &mut String, input: &str) {
    let input = input.trim();
    if input.is_empty() {
        return;
    }

    let (command, args) = input
        .split_once(' ')
        .map(|(command, args)| (command, args.trim_start()))
        .unwrap_or((input, ""));

    if command.eq_ignore_ascii_case("help") {
        help(console);
    } else if command.eq_ignore_ascii_case("kernel") {
        kernel_command(console, cwd, args);
    } else if command.eq_ignore_ascii_case("clear") {
        console.clear();
    } else if command.eq_ignore_ascii_case("echo") {
        let _ = writeln!(console, "{args}");
    } else if command.eq_ignore_ascii_case("uname") {
        let _ = writeln!(console, "Generic 0.1.0 x86_64");
    } else if command.eq_ignore_ascii_case("version") {
        let _ = writeln!(console, "Generic OS kernel 0.1.0");
    } else if command.eq_ignore_ascii_case("mem") {
        memory(console);
    } else if command.eq_ignore_ascii_case("video") {
        video(console);
    } else if command.eq_ignore_ascii_case("font") {
        font(console, cwd, args);
    } else if command.eq_ignore_ascii_case("recontrol") {
        let value = crate::recontrol::probe();
        let _ = writeln!(console, "Recontrol ABI probe returned {value}");
    } else if command.eq_ignore_ascii_case("whoami") {
        let _ = writeln!(console, "root (kernel console)");
    } else if command.eq_ignore_ascii_case("pwd") {
        let _ = writeln!(console, "{cwd}");
    } else if command.eq_ignore_ascii_case("cd") {
        change_directory(console, cwd, args);
    } else if command.eq_ignore_ascii_case("ls") {
        list_directory(console, cwd, args);
    } else if command.eq_ignore_ascii_case("cat") {
        cat_file(console, cwd, args);
    } else if command.eq_ignore_ascii_case("touch") {
        touch(console, cwd, args);
    } else if command.eq_ignore_ascii_case("mkdir") {
        mkdir(console, cwd, args);
    } else if command.eq_ignore_ascii_case("rm") {
        remove(console, cwd, args);
    } else if command.eq_ignore_ascii_case("write") {
        write_file(console, cwd, args, false);
    } else if command.eq_ignore_ascii_case("append") {
        write_file(console, cwd, args, true);
    } else if command.eq_ignore_ascii_case("stat") {
        stat(console, cwd, args);
    } else if command.eq_ignore_ascii_case("mounts") {
        mounts(console);
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

fn help(console: &mut Console<'_>) {
    let _ = writeln!(console, "Commands:");
    let _ = writeln!(console, "  HELP                 show this list");
    let _ = writeln!(
        console,
        "  KERNEL [COMMAND]     kernel settings, status and control"
    );
    let _ = writeln!(console, "  CLEAR                clear the screen");
    let _ = writeln!(console, "  ECHO TEXT            print text");
    let _ = writeln!(console, "  UNAME                system information");
    let _ = writeln!(console, "  PWD / CD PATH        current directory");
    let _ = writeln!(console, "  LS [PATH]            list directory");
    let _ = writeln!(console, "  CAT PATH             read file");
    let _ = writeln!(console, "  TOUCH PATH           create empty file");
    let _ = writeln!(console, "  MKDIR PATH           create directory");
    let _ = writeln!(console, "  WRITE PATH TEXT      create/truncate and write");
    let _ = writeln!(console, "  APPEND PATH TEXT     append to file");
    let _ = writeln!(
        console,
        "  RM PATH              unlink file/empty directory"
    );
    let _ = writeln!(console, "  STAT PATH            inode/type/size");
    let _ = writeln!(console, "  RECONTROL            call Recontrol code");
    let _ = writeln!(console);
    let _ = writeln!(console, "Use KERNEL HELP for system commands.");
}

fn kernel_command(console: &mut Console<'_>, cwd: &str, args: &str) {
    let args = args.trim();
    if args.is_empty() || args.eq_ignore_ascii_case("help") {
        kernel_help(console);
        return;
    }

    let (subcommand, subargs) = args
        .split_once(' ')
        .map(|(command, rest)| (command, rest.trim_start()))
        .unwrap_or((args, ""));

    if subcommand.eq_ignore_ascii_case("status") {
        kernel_status(console);
    } else if subcommand.eq_ignore_ascii_case("diagnostics")
        || subcommand.eq_ignore_ascii_case("diag")
    {
        kernel_diagnostics(console);
    } else if subcommand.eq_ignore_ascii_case("version") {
        let _ = writeln!(console, "Generic OS kernel 0.1.0 x86_64");
    } else if subcommand.eq_ignore_ascii_case("memory") || subcommand.eq_ignore_ascii_case("mem") {
        memory(console);
    } else if subcommand.eq_ignore_ascii_case("tasks") {
        tasks(console);
    } else if subcommand.eq_ignore_ascii_case("processes") || subcommand.eq_ignore_ascii_case("ps")
    {
        processes(console);
    } else if subcommand.eq_ignore_ascii_case("video") {
        video(console);
    } else if subcommand.eq_ignore_ascii_case("font") {
        font(console, cwd, subargs);
    } else if subcommand.eq_ignore_ascii_case("mounts") {
        mounts(console);
    } else if subcommand.eq_ignore_ascii_case("reboot") {
        let _ = writeln!(console, "Rebooting...");
        crate::arch::reboot();
    } else if subcommand.eq_ignore_ascii_case("halt") || subcommand.eq_ignore_ascii_case("shutdown")
    {
        let _ = writeln!(console, "CPU halted.");
        crate::arch::halt();
    } else {
        let _ = writeln!(console, "kernel: unknown subcommand: {subcommand}");
        let _ = writeln!(console, "Use KERNEL HELP.");
    }
}

fn kernel_help(console: &mut Console<'_>) {
    let _ = writeln!(console, "KERNEL - Generic kernel settings and control");
    let _ = writeln!(console, "Usage: KERNEL COMMAND [ARGS]");
    let _ = writeln!(console);
    let _ = writeln!(console, "  HELP                 show kernel command help");
    let _ = writeln!(console, "  STATUS               combined kernel status");
    let _ = writeln!(console, "  DIAGNOSTICS          run kernel self-checks");
    let _ = writeln!(console, "  VERSION              kernel version");
    let _ = writeln!(console, "  MEMORY               physical memory and heap");
    let _ = writeln!(
        console,
        "  TASKS                kernel scheduler/task state"
    );
    let _ = writeln!(console, "  PROCESSES             userspace process table");
    let _ = writeln!(console, "  VIDEO                framebuffer information");
    let _ = writeln!(
        console,
        "  FONT ...             inspect/change/load framebuffer font"
    );
    let _ = writeln!(console, "  MOUNTS               mounted filesystems");
    let _ = writeln!(console, "  REBOOT               reboot the machine");
    let _ = writeln!(console, "  HALT                 halt the CPU");
    let _ = writeln!(console);
    let _ = writeln!(console, "Examples:");
    let _ = writeln!(console, "  KERNEL STATUS");
    let _ = writeln!(console, "  KERNEL DIAGNOSTICS");
    let _ = writeln!(console, "  KERNEL TASKS");
    let _ = writeln!(console, "  KERNEL PROCESSES");
    let _ = writeln!(console, "  KERNEL FONT LIST");
    let _ = writeln!(console, "  KERNEL FONT SET noto20");
    let _ = writeln!(console, "  KERNEL FONT LOAD /mnt/fonts/custom.psf");
}

fn kernel_status(console: &mut Console<'_>) {
    let stats = crate::mm::stats();
    let (font_width, font_height) = console.font_dimensions();
    let mounts = crate::vfs::mounts();

    let _ = writeln!(console, "Generic OS kernel 0.1.0 x86_64");
    let _ = writeln!(
        console,
        "Video: {}x{} px, terminal {}x{} cells",
        console.width(),
        console.height(),
        console.columns(),
        console.rows()
    );
    let _ = writeln!(
        console,
        "Font: {} ({}x{} glyph)",
        console.font_name(),
        font_width,
        font_height
    );
    let _ = writeln!(
        console,
        "Memory: {} MiB total, {} MiB free; heap {} KiB free",
        stats.physical_total / (1024 * 1024),
        stats.physical_free / (1024 * 1024),
        stats.heap_free / 1024
    );
    let scheduler = crate::task::stats();
    let _ = writeln!(
        console,
        "Tasks: {} total, {} running, {} sleeping, {} switches",
        scheduler.total, scheduler.running, scheduler.sleeping, scheduler.context_switches
    );
    let processes = crate::process::diagnostics();
    let _ = writeln!(
        console,
        "Processes: {} known, init-exit={}",
        processes.process_count, processes.last_exit
    );
    let _ = writeln!(console, "Mounts: {}", mounts.len());
    for mount in mounts {
        let _ = writeln!(console, "  {} on {}", mount.filesystem, mount.path);
    }
}

#[derive(Default)]
struct DiagnosticSummary {
    passed: usize,
    warnings: usize,
    failed: usize,
}

impl DiagnosticSummary {
    fn ok(&mut self, console: &mut Console<'_>, message: fmt::Arguments<'_>) {
        self.passed += 1;
        let _ = write!(console, "[ok] ");
        let _ = console.write_fmt(message);
        let _ = writeln!(console);
    }

    fn warn(&mut self, console: &mut Console<'_>, message: fmt::Arguments<'_>) {
        self.warnings += 1;
        let _ = write!(console, "[warn] ");
        let _ = console.write_fmt(message);
        let _ = writeln!(console);
    }

    fn fail(&mut self, console: &mut Console<'_>, message: fmt::Arguments<'_>) {
        self.failed += 1;
        let _ = write!(console, "[fail] ");
        let _ = console.write_fmt(message);
        let _ = writeln!(console);
    }
}

fn kernel_diagnostics(console: &mut Console<'_>) {
    let mut summary = DiagnosticSummary::default();
    let _ = writeln!(console, "Generic kernel diagnostics");
    let _ = writeln!(console, "Running non-destructive runtime checks...");
    let _ = writeln!(console);

    let stats = crate::mm::stats();
    if stats.physical_total > 0
        && stats.physical_free <= stats.physical_total
        && stats.managed_regions > 0
        && stats.heap_total > 0
        && stats.heap_free <= stats.heap_total
    {
        summary.ok(
            console,
            format_args!(
                "memory accounting: {} MiB free / {} MiB, heap {} KiB free / {} KiB",
                stats.physical_free / (1024 * 1024),
                stats.physical_total / (1024 * 1024),
                stats.heap_free / 1024,
                stats.heap_total / 1024
            ),
        );
    } else {
        summary.fail(console, format_args!("memory accounting is inconsistent"));
    }

    let apic = crate::arch::apic::diagnostics();
    if apic.initialized && apic.io_apic_count > 0 {
        summary.ok(
            console,
            format_args!(
                "interrupt controller: xAPIC id={} with {} IOAPIC(s)",
                apic.local_apic_id, apic.io_apic_count
            ),
        );
    } else {
        summary.fail(console, format_args!("APIC/IOAPIC is not initialized"));
    }

    if crate::arch::timer::initialized() && crate::arch::timer::ticks() > 0 {
        summary.ok(
            console,
            format_args!(
                "system timer: {} Hz, {} ticks",
                crate::arch::timer::HZ,
                crate::arch::timer::ticks()
            ),
        );
    } else {
        summary.fail(console, format_args!("system timer is not advancing"));
    }

    let scheduler = crate::task::stats();
    if scheduler.initialized && scheduler.total >= 2 && scheduler.context_switches > 0 {
        summary.ok(
            console,
            format_args!(
                "scheduler: {} task(s), {} context switches, kworker heartbeat={}",
                scheduler.total, scheduler.context_switches, scheduler.heartbeat
            ),
        );
    } else if scheduler.initialized {
        summary.warn(
            console,
            format_args!(
                "scheduler initialized but not yet exercised: {} task(s), {} switches",
                scheduler.total, scheduler.context_switches
            ),
        );
    } else {
        summary.fail(console, format_args!("kernel scheduler is not initialized"));
    }

    let user = crate::arch::user::diagnostics();
    let process = crate::process::diagnostics();
    if process.probe_ok && user.last_cpl == 3 && process.process_count > 0 {
        summary.ok(
            console,
            format_args!(
                "userspace ELF/syscall: {} process(es), CPL{}, init exit={}",
                process.process_count, user.last_cpl, process.last_exit
            ),
        );
    } else {
        summary.fail(
            console,
            format_args!(
                "userspace ELF/syscall invalid: ok={} processes={} cpl={}",
                process.probe_ok, process.process_count, user.last_cpl
            ),
        );
    }

    match crate::arch::ps2::diagnostics() {
        Some(input) if input.keyboard && input.mouse => {
            summary.ok(
                console,
                format_args!("PS/2 input: keyboard=true mouse=true wheel={}", input.wheel),
            );
        }
        Some(input) => {
            summary.warn(
                console,
                format_args!(
                    "PS/2 input partial: keyboard={} mouse={} wheel={}",
                    input.keyboard, input.mouse, input.wheel
                ),
            );
        }
        None => summary.warn(console, format_args!("PS/2 input status unavailable")),
    }

    match crate::mm::runtime_diagnostics() {
        Some(memory) => {
            if memory.cr3 != 0 && memory.write_protect {
                summary.ok(
                    console,
                    format_args!(
                        "MMU: CR3={:#x}, supervisor write-protect enabled",
                        memory.cr3
                    ),
                );
            } else {
                summary.fail(
                    console,
                    format_args!(
                        "MMU state invalid: CR3={:#x}, write-protect={}",
                        memory.cr3, memory.write_protect
                    ),
                );
            }

            if memory.heap_start_mapped
                && memory.heap_end_mapped
                && memory.lower_guard_unmapped
                && memory.upper_guard_unmapped
            {
                summary.ok(
                    console,
                    format_args!("heap mapping present with both guard pages unmapped"),
                );
            } else {
                summary.fail(
                    console,
                    format_args!(
                        "heap layout invalid: start={} end={} lower-guard={} upper-guard={}",
                        memory.heap_start_mapped,
                        memory.heap_end_mapped,
                        memory.lower_guard_unmapped,
                        memory.upper_guard_unmapped
                    ),
                );
            }
        }
        None => summary.fail(console, format_args!("MMU diagnostics unavailable")),
    }

    let (font_width, font_height) = console.font_dimensions();
    if console.width() > 0
        && console.height() > 0
        && console.columns() > 0
        && console.rows() > 0
        && font_width > 0
        && font_height > 0
    {
        summary.ok(
            console,
            format_args!(
                "framebuffer {}x{}, terminal {}x{}, font {} {}x{}",
                console.width(),
                console.height(),
                console.columns(),
                console.rows(),
                console.font_name(),
                font_width,
                font_height
            ),
        );
    } else {
        summary.fail(
            console,
            format_args!("framebuffer or terminal geometry is invalid"),
        );
    }

    match crate::vfs::metadata("/") {
        Ok(metadata) if metadata.kind == NodeKind::Directory => {
            summary.ok(console, format_args!("VFS root is mounted and accessible"));
        }
        Ok(_) => summary.fail(console, format_args!("VFS root is not a directory")),
        Err(error) => summary.fail(console, format_args!("VFS root lookup failed: {error}")),
    }

    match crate::vfs::read_file("/etc/issue") {
        Ok(data) if !data.is_empty() => {
            summary.ok(
                console,
                format_args!(
                    "initramfs content readable: /etc/issue ({} bytes)",
                    data.len()
                ),
            );
        }
        Ok(_) => summary.warn(console, format_args!("/etc/issue is empty")),
        Err(error) => summary.fail(
            console,
            format_args!("initramfs read failed for /etc/issue: {error}"),
        ),
    }

    let mounts = crate::vfs::mounts();
    if mounts
        .iter()
        .any(|mount| mount.path == "/" && mount.filesystem == "ramfs")
    {
        summary.ok(
            console,
            format_args!("root filesystem: ramfs ({} mount(s) total)", mounts.len()),
        );
    } else {
        summary.fail(
            console,
            format_args!("expected ramfs root mount is missing"),
        );
    }

    if mounts
        .iter()
        .any(|mount| mount.path == "/mnt" && mount.filesystem == "genericfs")
    {
        match crate::vfs::read_file("/mnt/.generic-persist") {
            Ok(marker) if marker == b"generic-persistent-v1" => {
                summary.ok(
                    console,
                    format_args!("GenericFS persistent volume marker verified"),
                );
            }
            Ok(_) => summary.fail(
                console,
                format_args!("GenericFS persistent marker is corrupted"),
            ),
            Err(error) => summary.fail(
                console,
                format_args!("GenericFS persistent marker read failed: {error}"),
            ),
        }
    } else {
        summary.warn(
            console,
            format_args!("persistent GenericFS is not mounted at /mnt"),
        );
    }

    let recontrol = crate::recontrol::probe();
    if recontrol == 128 {
        summary.ok(
            console,
            format_args!("Recontrol freestanding ABI probe returned {recontrol}"),
        );
    } else {
        summary.fail(
            console,
            format_args!("Recontrol ABI probe returned unexpected value {recontrol}"),
        );
    }

    let _ = writeln!(console);
    let _ = writeln!(
        console,
        "Diagnostics: {} passed, {} warning(s), {} failed",
        summary.passed, summary.warnings, summary.failed
    );
    if summary.failed == 0 {
        if summary.warnings == 0 {
            let _ = writeln!(console, "Result: all checked subsystems healthy");
        } else {
            let _ = writeln!(
                console,
                "Result: checked subsystems operational with warnings"
            );
        }
    } else {
        let _ = writeln!(console, "Result: one or more checked subsystems failed");
    }
}

fn video(console: &mut Console<'_>) {
    let _ = writeln!(
        console,
        "Framebuffer: {}x{} pixels; terminal grid {}x{}",
        console.width(),
        console.height(),
        console.columns(),
        console.rows()
    );
}

fn font(console: &mut Console<'_>, cwd: &str, args: &str) {
    let args = args.trim();
    if args.is_empty() || args.eq_ignore_ascii_case("current") {
        print_font(console);
        return;
    }

    if args.eq_ignore_ascii_case("list") {
        let _ = writeln!(console, "Built-in fonts:");
        let _ = writeln!(console, "  noto16   Noto Sans Mono Regular 16 px (default)");
        let _ = writeln!(console, "  noto20   Noto Sans Mono Regular 20 px");
        let _ = writeln!(console, "  noto24   Noto Sans Mono Regular 24 px");
        let _ = writeln!(console, "  bold16   Noto Sans Mono Bold 16 px");
        let _ = writeln!(console, "  bold20   Noto Sans Mono Bold 20 px");
        let _ = writeln!(console, "Custom: KERNEL FONT LOAD PATH.psf (PSF2)");
        return;
    }

    if args.eq_ignore_ascii_case("reset") {
        console.set_font_preset(FontPreset::Noto16);
        let _ = writeln!(console, "Font reset to noto16.");
        print_font(console);
        return;
    }

    let (action, value) = args
        .split_once(' ')
        .map(|(action, value)| (action, value.trim()))
        .unwrap_or(("set", args));

    if action.eq_ignore_ascii_case("set") {
        let Some(preset) = FontPreset::parse(value) else {
            let _ = writeln!(console, "font: unknown preset: {value}");
            let _ = writeln!(console, "Use KERNEL FONT LIST.");
            return;
        };
        console.set_font_preset(preset);
        let _ = writeln!(console, "Font changed to {}.", preset.name());
        print_font(console);
        return;
    }

    if action.eq_ignore_ascii_case("load") {
        if value.is_empty() {
            let _ = writeln!(console, "font: usage: KERNEL FONT LOAD PATH.psf");
            return;
        }

        let path = match crate::vfs::canonicalize(cwd, value) {
            Ok(path) => path,
            Err(error) => return vfs_error(console, "font", error),
        };
        let metadata = match crate::vfs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => return vfs_error(console, "font", error),
        };
        if metadata.kind != NodeKind::File {
            let _ = writeln!(console, "font: not a file: {path}");
            return;
        }
        if metadata.len > MAX_CUSTOM_FONT_BYTES {
            let _ = writeln!(
                console,
                "font: file too large ({} bytes, max {})",
                metadata.len, MAX_CUSTOM_FONT_BYTES
            );
            return;
        }

        let data = match crate::vfs::read_file(&path) {
            Ok(data) => data,
            Err(error) => return vfs_error(console, "font", error),
        };

        match console.load_psf2(data) {
            Ok(glyphs) => {
                let _ = writeln!(console, "Loaded PSF2 font: {path} ({glyphs} glyphs)");
                print_font(console);
            }
            Err(error) => {
                let _ = writeln!(console, "font: {error}");
            }
        }
        return;
    }

    let _ = writeln!(
        console,
        "font: expected LIST, SET, LOAD, CURRENT or RESET\nUse KERNEL HELP."
    );
}

fn print_font(console: &mut Console<'_>) {
    let (width, height) = console.font_dimensions();
    let _ = writeln!(
        console,
        "Font: {}  glyph={}x{}  grid={}x{}",
        console.font_name(),
        width,
        height,
        console.columns(),
        console.rows()
    );
}

fn processes(console: &mut Console<'_>) {
    let items = crate::process::processes();
    let _ = writeln!(console, "Userspace processes: {}", items.len());
    for process in items {
        let state = match process.state {
            crate::process::ProcessState::Running => "running",
            crate::process::ProcessState::Exited => "exited",
        };
        match process.exit_code {
            Some(code) => {
                let _ = writeln!(
                    console,
                    "  pid={:<3} {:<12} {:<7} entry={:#x} exit={}",
                    process.pid, process.name, state, process.entry, code
                );
            }
            None => {
                let _ = writeln!(
                    console,
                    "  pid={:<3} {:<12} {:<7} entry={:#x}",
                    process.pid, process.name, state, process.entry
                );
            }
        }
    }
}

fn tasks(console: &mut Console<'_>) {
    let stats = crate::task::stats();
    let _ = writeln!(
        console,
        "Scheduler: {} task(s), {} context switches, heartbeat={}",
        stats.total, stats.context_switches, stats.heartbeat
    );
    for task in crate::task::tasks() {
        let state = match task.state {
            crate::task::TaskState::Running => "running",
            crate::task::TaskState::Ready => "ready",
            crate::task::TaskState::Sleeping => "sleeping",
            crate::task::TaskState::Exited => "exited",
        };
        match task.wake_tick {
            Some(tick) => {
                let _ = writeln!(
                    console,
                    "  #{:<3} {:<12} {:<9} wake@{}",
                    task.id, task.name, state, tick
                );
            }
            None => {
                let _ = writeln!(console, "  #{:<3} {:<12} {}", task.id, task.name, state);
            }
        }
    }
}

fn memory(console: &mut Console<'_>) {
    let stats = crate::mm::stats();
    let _ = writeln!(
        console,
        "Physical: {} MiB total, {} MiB free, {} regions",
        stats.physical_total / (1024 * 1024),
        stats.physical_free / (1024 * 1024),
        stats.managed_regions
    );
    let _ = writeln!(
        console,
        "Heap: {} KiB total, {} KiB free, RW+NX + guards",
        stats.heap_total / 1024,
        stats.heap_free / 1024
    );
}

fn change_directory(console: &mut Console<'_>, cwd: &mut String, args: &str) {
    let target = if args.is_empty() { "/" } else { args };
    let path = match crate::vfs::canonicalize(cwd, target) {
        Ok(path) => path,
        Err(error) => return vfs_error(console, "cd", error),
    };

    match crate::vfs::metadata(&path) {
        Ok(metadata) if metadata.kind == NodeKind::Directory => *cwd = path,
        Ok(_) => {
            let _ = writeln!(console, "cd: not a directory");
        }
        Err(error) => vfs_error(console, "cd", error),
    }
}

fn list_directory(console: &mut Console<'_>, cwd: &str, args: &str) {
    let target = if args.is_empty() { cwd } else { args };
    let path = match crate::vfs::canonicalize(cwd, target) {
        Ok(path) => path,
        Err(error) => return vfs_error(console, "ls", error),
    };

    match crate::vfs::read_dir(&path) {
        Ok(entries) => {
            for entry in entries {
                match entry.metadata.kind {
                    NodeKind::Directory => {
                        let _ = writeln!(console, "{}/", entry.name);
                    }
                    NodeKind::File => {
                        let _ = writeln!(console, "{}  {} bytes", entry.name, entry.metadata.len);
                    }
                }
            }
        }
        Err(error) => vfs_error(console, "ls", error),
    }
}

fn cat_file(console: &mut Console<'_>, cwd: &str, args: &str) {
    let Some(path) = required_path(console, cwd, "cat", args) else {
        return;
    };

    match crate::vfs::read_file(&path) {
        Ok(data) => {
            let text = String::from_utf8_lossy(&data);
            let _ = write!(console, "{text}");
            if !text.ends_with('\n') {
                let _ = writeln!(console);
            }
        }
        Err(error) => vfs_error(console, "cat", error),
    }
}

fn touch(console: &mut Console<'_>, cwd: &str, args: &str) {
    let Some(path) = required_path(console, cwd, "touch", args) else {
        return;
    };
    if let Err(error) = crate::vfs::touch(&path) {
        vfs_error(console, "touch", error);
    }
}

fn mkdir(console: &mut Console<'_>, cwd: &str, args: &str) {
    let Some(path) = required_path(console, cwd, "mkdir", args) else {
        return;
    };
    if let Err(error) = crate::vfs::mkdir(&path) {
        vfs_error(console, "mkdir", error);
    }
}

fn remove(console: &mut Console<'_>, cwd: &str, args: &str) {
    let Some(path) = required_path(console, cwd, "rm", args) else {
        return;
    };
    if let Err(error) = crate::vfs::remove(&path) {
        vfs_error(console, "rm", error);
    }
}

fn write_file(console: &mut Console<'_>, cwd: &str, args: &str, append: bool) {
    if args.is_empty() {
        let _ = writeln!(
            console,
            "{}: usage: {} PATH TEXT",
            if append { "append" } else { "write" },
            if append { "APPEND" } else { "WRITE" }
        );
        return;
    }

    let (raw_path, text) = args
        .split_once(' ')
        .map(|(path, text)| (path, text))
        .unwrap_or((args, ""));

    let path = match crate::vfs::canonicalize(cwd, raw_path) {
        Ok(path) => path,
        Err(error) => return vfs_error(console, if append { "append" } else { "write" }, error),
    };

    if let Err(error) = crate::vfs::write_file(&path, text.as_bytes(), append) {
        vfs_error(console, if append { "append" } else { "write" }, error);
    }
}

fn stat(console: &mut Console<'_>, cwd: &str, args: &str) {
    let Some(path) = required_path(console, cwd, "stat", args) else {
        return;
    };

    match crate::vfs::metadata(&path) {
        Ok(metadata) => {
            let kind = match metadata.kind {
                NodeKind::File => "file",
                NodeKind::Directory => "directory",
            };
            let _ = writeln!(
                console,
                "Path: {path}\nInode: {}\nType: {kind}\nSize: {} bytes",
                metadata.inode, metadata.len
            );
        }
        Err(error) => vfs_error(console, "stat", error),
    }
}

fn mounts(console: &mut Console<'_>) {
    for mount in crate::vfs::mounts() {
        let _ = writeln!(console, "{} on {}", mount.filesystem, mount.path);
    }
}

fn required_path(
    console: &mut Console<'_>,
    cwd: &str,
    command: &str,
    args: &str,
) -> Option<String> {
    let raw = args.trim();
    if raw.is_empty() {
        let _ = writeln!(console, "{command}: path required");
        return None;
    }

    match crate::vfs::canonicalize(cwd, raw) {
        Ok(path) => Some(path),
        Err(error) => {
            vfs_error(console, command, error);
            None
        }
    }
}

fn vfs_error(console: &mut Console<'_>, command: &str, error: kernel_core::vfs::VfsError) {
    let _ = writeln!(console, "{command}: {error}");
}
