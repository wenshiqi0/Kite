//! Disposable native allocation peer for the owned cgroup acceptance test.
use std::fs::File;
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let cache = std::env::args().nth(1).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "owned cache path is required")
    })?;
    let mut resident: Vec<Vec<u8>> = Vec::new();
    println!("ready");
    io::stdout().flush()?;
    for command in io::stdin().lock().lines() {
        let command = command?;
        if command.trim() == "cache" {
            let mut file = File::create(&cache)?;
            let block = vec![b'x'; 1024 * 1024];
            for _ in 0..160 {
                file.write_all(&block)?;
            }
            file.sync_all()?;
            println!("cached");
        } else {
            let size = if command.trim() == "resident" {
                32
            } else {
                256
            } * 1024
                * 1024;
            let mut bytes = vec![0; size];
            for offset in (0..size).step_by(4096) {
                bytes[offset] = 1;
            }
            resident.push(bytes);
            std::hint::black_box(&resident);
            println!("resident");
        }
        io::stdout().flush()?;
    }
    Ok(())
}
