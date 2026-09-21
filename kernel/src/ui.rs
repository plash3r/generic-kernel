use bootloader_api::info::FrameBuffer;

#[derive(Clone, Copy)]
enum Mode {
    Desktop,
    Console,
}

pub fn run(framebuffer: &mut FrameBuffer) -> ! {
    let mut mode = Mode::Desktop;

    loop {
        mode = match mode {
            Mode::Desktop => {
                let display = crate::arch::graphics::Display::new(framebuffer);
                match crate::desktop::run(display) {
                    crate::desktop::DesktopExit::Console => Mode::Console,
                }
            }
            Mode::Console => {
                let console = crate::arch::framebuffer::Console::new(framebuffer);
                match crate::shell::run(console) {
                    crate::shell::ShellExit::Desktop => Mode::Desktop,
                }
            }
        };
    }
}
