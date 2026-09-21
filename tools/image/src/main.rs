use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let first = args
        .next()
        .ok_or("usage: generic-image [uefi|bios] KERNEL OUTPUT")?;

    let first_text = first.to_string_lossy();
    let (mode, kernel) = if first_text == "uefi" || first_text == "bios" {
        let kernel = PathBuf::from(args.next().ok_or("missing kernel path")?);
        (first_text.into_owned(), kernel)
    } else {
        ("uefi".to_string(), PathBuf::from(first))
    };

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

    match mode.as_str() {
        "uefi" => bootloader::UefiBoot::new(&kernel).create_disk_image(&output)?,
        "bios" => bootloader::BiosBoot::new(&kernel).create_disk_image(&output)?,
        _ => unreachable!(),
    }

    println!("{}", output.display());
    Ok(())
}
