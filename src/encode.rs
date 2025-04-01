use crate::processing::process_tar_entries;
use crate::utilities::*;
use bzip2::{Compression as BzCompression, write::BzEncoder};
use clap::Parser;
use flate2::{Compression as GzCompression, write::GzEncoder};
use rayon::prelude::*;
use std::fs::File;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use xz2::write::XzEncoder;
use zip::CompressionMethod;
use zip::write::FileOptions;
use zip::{ZipWriter, write::SimpleFileOptions};
use zstd::Encoder as ZstdEncoder;

#[derive(Parser)]
#[command(version, about, long_about = None, author = "Bryan Hyland <bryan.hyland32@gmail.com")]
pub struct Cli {
    #[
        arg(long, short, help = "Encoding format to use.", 
        value_parser = clap::builder::PossibleValuesParser::new(
            ["tar-gz", "tar-bz2", "tar-xz", "tar-zstd", "zip"]
        ), default_value = "tar-gz")
    ]
    pub format: String,
    #[arg(long, short, help = "Path to the directory to be compressed.")]
    pub src: String,
    #[arg(long, short, help = "Path to the output file.")]
    pub output: String,
}

pub fn encode_tar_gz(src: String, output: String) -> Result<(), std::io::Error> {
    let output_file = File::create(output)?;
    let output_writer = BufWriter::new(output_file);
    let encoder = GzEncoder::new(output_writer, GzCompression::default());
    let tar_file = tar::Builder::new(encoder);

    let total_files = num_files(&src);

    let (progress, working_status) = setup_progress(total_files);

    let tar_file = process_tar_entries(&src, tar_file, progress.clone(), working_status.clone())?;

    let encoder = tar_file.into_inner()?;

    encoder.finish()?;

    // Clear the screen and reset the cursor before the final message
    finalize_progress(&progress);

    Ok(())
}

pub fn encode_tar_bz(src: String, output: String) -> Result<(), std::io::Error> {
    // create the .tar.bz destination file
    let output_file = File::create(output)?;
    let output_writer = BufWriter::new(output_file);

    // create the encoder for the destination file
    let encoder = BzEncoder::new(output_writer, BzCompression::default());

    // create a tar builder with the encoder
    let tar_file = tar::Builder::new(encoder);

    // Walk through the source directory and count all of the files for the progress bar
    let total_files = num_files(&src);

    // Create the progress bar and working status
    let (progress, working_status) = setup_progress(total_files);

    // Process the tar entries
    let tar_file = process_tar_entries(&src, tar_file, progress.clone(), working_status.clone())?;

    let encoder = tar_file.into_inner()?;

    encoder.finish()?;

    // Add a small delay to let all progress bars finish their drawing
    std::thread::sleep(Duration::from_millis(100));

    // Clear the screen and reset the cursor before the final message
    finalize_progress(&progress);

    Ok(())
}

pub fn encode_tar_xz(src: String, output: String) -> Result<(), std::io::Error> {
    let output_file = File::create(output)?;
    let output_writer = BufWriter::new(output_file);
    let encoder = XzEncoder::new(output_writer, 6);
    let tar_file = tar::Builder::new(encoder);

    let total_files = num_files(&src);

    let (progress, working_status) = setup_progress(total_files);

    let tar_file = process_tar_entries(&src, tar_file, progress.clone(), working_status)?;

    let encoder = tar_file.into_inner()?;

    encoder.finish()?;

    // Clear the screen and reset the cursor before the final message
    finalize_progress(&progress);

    Ok(())
}

pub fn encode_tar_zstd(src: String, output: String) -> Result<(), std::io::Error> {
    let output_file = File::create(output)?;
    let output_writer = BufWriter::new(output_file);
    let encoder = ZstdEncoder::new(output_writer, 3)?;
    let tar_file = tar::Builder::new(encoder);

    let total_files = num_files(&src);

    let (progress, working_status) = setup_progress(total_files);

    let tar_file = process_tar_entries(&src, tar_file, progress.clone(), working_status)?;

    let encoder = tar_file.into_inner()?;

    encoder.finish()?;

    finalize_progress(&progress);

    Ok(())
}

pub fn encode_zip(src: String, output: String) -> Result<(), std::io::Error> {
    let output_file = File::create(output)?;
    let output_writer = BufWriter::new(output_file);
    let zip_writer = Arc::new(Mutex::new(ZipWriter::new(output_writer)));

    let total_files = num_files(&src);

    let (progress, working_status) = setup_progress(total_files);

    let entries = entries(&src);
    entries.into_par_iter().for_each(|entry| {
        let path = entry.path();
        let rel_path = match path.strip_prefix(&src) {
            Ok(path) => path.to_path_buf(),
            Err(_) => return,
        };

        let file_name = rel_path.clone();

        {
            if let Ok(mut status) = working_status.lock() {
                *status = file_name.to_string_lossy().to_string().clone();
            }

            progress.status_bar.tick();
        }

        if path.is_file() {
            let mut file = match File::open(path) {
                Ok(file) => file,
                Err(e) => {
                    eprintln!("Error: {e}");
                    return;
                }
            };

            let mut zip_file = zip_writer.lock().unwrap();
            let options =
                FileOptions::<()>::default().compression_method(CompressionMethod::Deflated);
            let _ = zip_file
                .start_file(file_name.to_string_lossy().to_string(), options)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, format!("{err}")));

            std::io::copy(&mut file, &mut *zip_file).unwrap_or_default();
        } else if path.is_dir() {
            let mut zip_file = zip_writer.lock().unwrap();
            let dir_name = format!("{}/", file_name.to_string_lossy());
            zip_file
                .add_directory(dir_name, SimpleFileOptions::default())
                .unwrap_or_default();
        };

        progress.increment_total_progress();
    });

    {
        let mut status = working_status.lock().unwrap();
        status.clear();
    }

    // Finalize
    let zip = Arc::try_unwrap(zip_writer)
        .expect("Multiple writers in use")
        .into_inner()
        .expect("Mutex poisoned");
    zip.finish()?;

    finalize_progress(&progress);

    Ok(())
}
