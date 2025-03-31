use clap::Parser;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::thread;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use bzip2::{Compression, write::BzEncoder};
//use zstd::Encoder as ZstdEncoder;
//use xz2::write::XzEncoder;
use walkdir::WalkDir;
use std::sync::{Arc, Mutex};
use rayon::prelude::*;

#[derive(Parser)]
#[command(name = "encode", version, about, long_about = None, author = "Bryan Hyland <bryan.hyland32@gmail.com")]
pub struct Cli {
    #[
        arg(long, short, help = "Encoding format to use.", 
        value_parser = clap::builder::PossibleValuesParser::new(
            ["tar-gz", "tar-bz2", "tar-xz", "tar-zstd", "zip"]
        ), 
        default_value = "tar-gz")
    ]
    pub format: String,
    #[arg(long, short, help = "Path to the directory to be compressed.")]
    pub src: String,
    #[arg(long, short, help = "Path to the output file.")]
    pub output: String,
}

#[derive(Clone)]
struct CompressionProgress {
    multi_progress: MultiProgress,
    status_bar: ProgressBar,
    total_progress: ProgressBar,
    file_counter: Arc<Mutex<usize>>,
    progress_lock: Arc<Mutex<()>>,
}

impl CompressionProgress {
    fn new(total_files: u64) -> Self {
        let multi_progress = MultiProgress::new();
        let status_bar = multi_progress.add(ProgressBar::new_spinner());
        status_bar.enable_steady_tick(Duration::from_millis(100));
        status_bar.set_style(ProgressStyle::default_spinner()
            .template("{spinner:.blue} {msg}")
            .expect("Failed to set spinner template")
        );

        println!("\n\n");

        // Create the main progress bar
        let total_progress = multi_progress.add(ProgressBar::new(total_files));
        total_progress.set_style(ProgressStyle::default_bar()
            .template("Total: {spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} files ({percent}%) - {eta}")
            .expect("Failed to set progress bar template")
            .progress_chars("-\u{15E7}\u{00BA}"));

        total_progress.enable_steady_tick(Duration::from_millis(300));

        print!("\x1B[s"); // Save the cursor position for later use

        Self {
            multi_progress,
            status_bar,
            total_progress,
            file_counter: Arc::new(Mutex::new(0)),
            progress_lock: Arc::new(Mutex::new(())),
        }
    }

    fn increment_total_progress(&self) {
        // Serialize the updates
        let _lock = self.progress_lock.lock().unwrap();


        let mut counter = self.file_counter.lock().unwrap();
        *counter += 1;
        
        // Redraw the total progress bar at the bottom
        print!("\x1B[u"); // Move the cursor up to the saved position
        print!("\x1B[J"); // Clear the cursor from the end of the screen

        self.total_progress.set_position(*counter as u64);
        std::io::stderr().flush().unwrap(); // Ensure the output is flushed to stderr

        self.total_progress.set_message(format!("Processing files... ({}/{})", *counter, self.total_progress.length().unwrap_or(0)));
    }

    fn finish(&self, message: &str) {
        // Serialize the updates
        let _lock = self.progress_lock.lock().unwrap();

        self.total_progress.finish_with_message(message.to_string());
    }
}

struct ProgressReader<R: std::io::Read> {
    inner: R,
    progress_bar: ProgressBar,
    bytes_read: u64,
}

impl<R: std::io::Read> ProgressReader<R> {
    fn new(inner: R, progress_bar: ProgressBar) -> Self {
        Self {
            inner,
            progress_bar,
            bytes_read: 0,
        }
    }
}

impl<R: std::io::Read> std::io::Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let result = self.inner.read(buf);
        if let Ok(num_bytes) = result {
            self.bytes_read += num_bytes as u64;
            self.progress_bar.set_position(self.bytes_read);
        }

        result
    }
}

fn process_file(
    path: &Path,
    rel_path: &PathBuf,
    tar_file: &Arc::<Mutex<tar::Builder<impl std::io::Write>>>,
    progress: &CompressionProgress,
    working_status: &Arc<Mutex<String>>,
) -> Result<(), std::io::Error> {
    // Get file metadata
    let metadata = path.metadata()?;
    let file_size = metadata.len();
    let file_display_name = rel_path.to_string_lossy().to_string();

    // Update file for working status
    if let Ok(mut status) = working_status.lock() {
        *status = file_display_name.clone();
    }

    // Open the file
    let mut file = File::open(path)?;

    // Get a buffer for parallel compression
    let mut buffer = Vec::with_capacity(file_size as usize);
    std::io::copy(&mut file, &mut buffer)?;

    // Create a header for the file
    let mut header = tar::Header::new_gnu();
    header.set_size(file_size);
    header.set_mode(metadata.permissions().mode());
    header.set_mtime(
        metadata
            .modified()
            .unwrap_or_else(|_| std::time::SystemTime::now())
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs()
    );

    // Set the path in the header
    if let Err(_e) = header.set_path(rel_path.as_path()) {
        let stripped_path = rel_path.strip_prefix(&path).unwrap();
        eprintln!("Warning: Failed to set long path for file {}", stripped_path.display())
    };

    header.set_cksum();

    // Append the file with our custom header
    {
        let mut tar = tar_file.lock().unwrap();
        tar.append(&header, &*buffer)?;
    }

    // Update the total progress
    progress.increment_total_progress();

    // Clear the working status message
    if let Ok(mut status) = working_status.lock() {
        *status = String::new();
    }

    Ok(())
}

// Function to process a directory
fn process_directory(
    path: &Path,
    rel_path: &PathBuf,
    tar_file: &mut tar::Builder<impl std::io::Write>,
    progress: &CompressionProgress,
    working_status: &Arc<Mutex<String>>,
) -> Result<(), std::io::Error> {
    // Update working status
    if let Ok(mut status) = working_status.lock() {
        *status = format!("Processing directory: {}", rel_path.display());
    }

    // Append directory to tar
    tar_file.append_dir(rel_path.clone(), path)?;

    // For empty directories, update progress
    if path.read_dir()?.next().is_none() {
        let _dir_name = rel_path.to_string_lossy().to_string();
        progress.increment_total_progress();
    }

    Ok(())
}

pub fn encode_tar_bz(src: String, output: String) -> Result<(), std::io::Error> {
    // create the .tar.bz destination file
    let output_file = File::create(output)?;
    let output_writer = BufWriter::new(output_file);

    // create the encoder for the destination file
    let encoder = BzEncoder::new(output_writer, Compression::default());

    // create a tar builder with the encoder
    let tar_file = tar::Builder::new(encoder);

    // Walk through the source directory and count all of the files for the progress bar
    let total_files = WalkDir::new(&src)
    .into_iter()
    .filter_map(Result::ok)
    .filter(|entry| {
        // Count files and non-empty directories
        entry.path().is_file() ||
        (entry.path().is_dir() && 
         entry.path().read_dir()
            .map(|mut dir| dir.next().is_some()).unwrap_or(false))
    })
    .count() as u64;

    // create a progress tracker
    let progress = CompressionProgress::new(total_files);
    let progress_clone = progress.clone();

    // Working indicator thread
    let working_status = Arc::new(Mutex::new(String::new()));
    let working_status_clone = working_status.clone();

    let _status_thread = thread::spawn(move || {

        loop {
            let status = {
                working_status_clone.lock().unwrap().clone()
            };

            let message = if status.is_empty() {
                format!("Working...")
            } else {
                format!("{status}")
            };

            progress_clone.status_bar.set_message(message);

            thread::sleep(Duration::from_millis(100));
        }
    });

    // Process all files and directories
    let tar_file = Arc::new(Mutex::new(tar_file));

    let entries: Vec<_> = WalkDir::new(&src)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().is_file() ||
            (entry.path().is_dir() && 
             entry.path().read_dir()
                .map(|mut dir| dir.next().is_some()).unwrap_or(false))
        })
        .collect();

    entries.into_par_iter()
        .for_each(|entry| {
            let path = entry.path();
            let rel_path = match path.strip_prefix(&src) {
                Ok(path) => path.to_path_buf(),
                Err(_) => return
            };

            let result = if path.is_file() {
                process_file(
                    path,
                    &rel_path,
                    &tar_file,
                    &progress,
                    &working_status
                )
            } else if path.is_dir() {
                process_directory(
                    path,
                    &rel_path,
                    &mut *tar_file.lock().unwrap(),
                    &progress,
                    &working_status
                )
            } else {
                Ok(())
            };

            if let Err(e) = result {
                eprintln!("Error: {e}");
            }
        });

    tar_file.lock().unwrap().finish()?;
    let mut encoder = match Arc::try_unwrap(tar_file) {
        Ok(encoder) => encoder.into_inner().expect("Mutex poisoned"),
        Err(_) => {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Multiple tar writers still in use!"));
        }
    };

    encoder.finish()?;

    // Add a small delay to let all progress bars finish their drawing
    std::thread::sleep(Duration::from_millis(100));

    // Clear the screen and reset the cursor before the final message
    print!("\x1B[2J"); // Clear the screen
    print!("\x1B[H"); // Move the cursor to the home position

    progress.finish("Compression complete! Your archive is ready!");

    Ok(())
}