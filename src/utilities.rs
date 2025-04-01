use crate::progress::CompressionProgress;
use std::{
    io::Write,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use walkdir::{DirEntry, WalkDir};

pub fn num_files(src: &str) -> u64 {
    // Walk through the source directory and count all of the files
    WalkDir::new(src)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            // Count files and non-empty directories
            entry.path().is_file()
                || (entry.path().is_dir()
                    && entry
                        .path()
                        .read_dir()
                        .map(|mut dir| dir.next().is_some())
                        .unwrap_or(false))
        })
        .count() as u64
}

pub fn entries(src: &str) -> Vec<DirEntry> {
    WalkDir::new(src)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            // Count files and non-empty directories
            entry.path().is_file()
                || (entry.path().is_dir()
                    && entry
                        .path()
                        .read_dir()
                        .map(|mut dir| dir.next().is_some())
                        .unwrap_or(false))
        })
        .collect()
}

pub fn update_status(progress: Arc<CompressionProgress>, working_status: Arc<Mutex<String>>) {
    thread::spawn(move || {
        let mut prev_message = String::new();
        loop {
            let status = { working_status.lock().unwrap().clone() };

            let message = if status.is_empty() {
                format!("Switching directories...")
            } else {
                format!("Compressing: {status}")
            };

            let is_new_file = !status.is_empty() && message != prev_message;

            if is_new_file {
                // Clear the current terminal in stderr to clear any artifacting
                eprint!("\x1B[2J\x1B[H");
                std::io::stderr().flush().unwrap();
            }

            progress.status_bar.set_message(format!("-> {}", message));
            progress.status_bar.tick();
            prev_message = message.clone();

            thread::sleep(Duration::from_millis(300));
        }
    });
}

pub fn setup_progress(total_files: u64) -> (Arc<CompressionProgress>, Arc<Mutex<String>>) {
    let progress = Arc::new(CompressionProgress::new(total_files));
    progress.update_eta();

    let working_status = Arc::new(Mutex::new(String::new()));
    update_status(progress.clone(), working_status.clone());

    (progress, working_status)
}

pub fn finalize_progress(progress: &CompressionProgress) {
    thread::sleep(Duration::from_millis(100));

    print!("\x1B[2j\x1B[H");
    progress.finish("Compression complete! Your archive is read!");
}
