use crate::arch::{
    framebuffer::Console,
    keyboard::{Key, Keyboard},
};
use alloc::string::String;
use core::fmt::Write;
use kernel_core::vfs::NodeKind;

const MAX_LINE: usize = 256;

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
            }
        }

        let input = core::str::from_utf8(&line[..len]).unwrap_or("");
        execute(&mut console, &mut cwd, input);
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
    let _ = writeln!(console, "  CLEAR                clear the screen");
    let _ = writeln!(console, "  ECHO TEXT            print text");
    let _ = writeln!(console, "  UNAME / VERSION      kernel information");
    let _ = writeln!(console, "  MEM / VIDEO          memory and framebuffer");
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
    let _ = writeln!(console, "  MOUNTS               mounted filesystems");
    let _ = writeln!(console, "  RECONTROL            call Recontrol code");
    let _ = writeln!(console, "  REBOOT / HALT        reset or stop the VM");
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
