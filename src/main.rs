pub(crate) mod encode;
pub(crate) mod processing;
pub(crate) mod progress;
pub(crate) mod utilities;

use clap::Parser;
use encode::{Cli, encode_tar_bz, encode_tar_gz, encode_tar_xz, encode_tar_zstd, encode_zip};

fn main() {
    let cli = Cli::parse();

    match cli.format.as_str() {
        "tar-gz" => {
            encode_tar_gz(cli.src, format!("{}.tar.gz", cli.output)).unwrap_or_else(|err| {
                eprintln!("Error compressing files: {}", err);
                std::process::exit(1);
            })
        }
        "tar-bz2" => {
            encode_tar_bz(cli.src, format!("{}.tar.bz", cli.output)).unwrap_or_else(|err| {
                eprintln!("Error compressing files: {}", err);
                std::process::exit(1);
            })
        }
        "tar-xz" => {
            encode_tar_xz(cli.src, format!("{}.tar.xz", cli.output)).unwrap_or_else(|err| {
                eprintln!("Error compressing files: {}", err);
                std::process::exit(1);
            })
        }
        "tar-zstd" => {
            encode_tar_zstd(cli.src, format!("{}.tar.zst", cli.output)).unwrap_or_else(|err| {
                eprintln!("Error compressing files: {}", err);
                std::process::exit(1);
            })
        }
        "zip" => encode_zip(cli.src, format!("{}.zip", cli.output)).unwrap_or_else(|err| {
            eprintln!("Error compressing files: {}", err);
            std::process::exit(1);
        }),
        _ => {
            eprintln!(
                "Invalid format specified: {}. Please use one of the following: tar-gz, tar-bz2, tar-xz, tar-zstd, zip",
                cli.format
            );
            std::process::exit(1);
        }
    }
}
