use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::{
    fs::File,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{progress::CompressionProgress, utilities::entries};

pub fn process_tar_file(
    path: &Path,
    rel_path: &Path,
    tar_file: &Arc<Mutex<tar::Builder<impl std::io::Write>>>,
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

    progress.status_bar.tick();

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
            .as_secs(),
    );

    // Set the path in the header
    header.set_path(rel_path).unwrap_or_else(|err| {
        eprintln!("Error setting path for {}: {}", file_display_name, err);
    });

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
pub fn process_tar_directory(
    path: &Path,
    rel_path: &Path,
    tar_file: &mut tar::Builder<impl std::io::Write>,
    progress: &CompressionProgress,
    working_status: &Arc<Mutex<String>>,
) -> Result<(), std::io::Error> {
    // Update working status
    if let Ok(mut status) = working_status.lock() {
        *status = format!("Processing directory: {}", rel_path.display());
    }

    // Append directory to tar
    tar_file.append_dir(rel_path, path)?;

    // For empty directories, update progress
    if path.read_dir()?.next().is_none() {
        let _dir_name = rel_path.to_string_lossy().to_string();
        progress.increment_total_progress();
    }

    Ok(())
}

pub fn process_tar_entries<W: Write + Send + 'static>(
    src: &str,
    tar_file: Arc<Mutex<tar::Builder<W>>>,
    progress: Arc<CompressionProgress>,
    working_status: Arc<Mutex<String>>,
) -> Result<(), std::io::Error> {
    let entries = entries(src);
    entries.into_par_iter().for_each(|entry| {
        let path = entry.path();
        let rel_path = match path.strip_prefix(src) {
            Ok(stripped) => PathBuf::from(stripped),
            Err(_) => {
                return;
            }
        };

        let result = if path.is_file() {
            process_tar_file(&path, &rel_path, &tar_file, &progress, &working_status)
        } else if path.is_dir() {
            process_tar_directory(
                &path,
                &rel_path,
                &mut tar_file.lock().unwrap(),
                &progress,
                &working_status,
            )
        } else {
            Ok(())
        };

        if let Err(e) = result {
            eprintln!("Error processing file: {}", e);
        }
    });

    Ok(())
}
