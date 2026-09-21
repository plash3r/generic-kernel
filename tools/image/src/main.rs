use std::{env, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let kernel = PathBuf::from(args.next().ok_or("usage: generic-image KERNEL OUTPUT")?);
    let output = PathBuf::from(args.next().ok_or("missing output path")?);
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    if !kernel.is_file() {
        return Err("kernel ELF not found".into());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    bootloader::UefiBoot::new(&kernel).create_disk_image(&output)?;
    println!("{}", output.display());
    Ok(())
}
