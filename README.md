# Packit

`Packit` is a versatile tool for compressing directories and files with support for various encoding formats. It also provides a progress bar to keep track of the compression process.

## Features

- Supports multiple compression formats:
  - tar-gz
  - tar-bz2
  - tar-xz
  - tar-zstd
  - zip
- Displays a progress bar during the compression process
- Handles directories and files efficiently
- Provides detailed status messages during compression
- Built with compression of large directories in mind

## Installation

To install `packit`, clone the repository and build the project using cargo.

```bash
git clone https://github.com/bhh32/packit.git
cd packit

cargo build --release
```