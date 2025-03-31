pub(crate) mod encode;

use clap::Parser;
use crate::encode::{Cli, encode_tar_bz};

fn main() {
    let cli = Cli::parse();

    match cli.format.as_str() {
        "tar-gz" => { todo!() },
        "tar-bz2" => encode_tar_bz(cli.src, format!("{}.tar.bz", cli.output))
            .unwrap_or_else(|err| {
                eprintln!("Error compressing files: {}", err);
                std::process::exit(1);
            }),
        "tar-xz" => { todo!() },
        "tar-zstd" => { todo!() },
        "zip" => { todo!() },
        _ => {
            eprintln!("Invalid format specified: {}. Please use one of the following: tar-gz, tar-bz2, tar-xz, tar-zstd, zip", cli.format);
            std::process::exit(1);
        }
    }
}
