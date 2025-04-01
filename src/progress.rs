use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct CompressionProgress {
    pub multi_progress: MultiProgress,
    pub status_bar: ProgressBar,
    pub total_progress: ProgressBar,
    pub file_counter: Arc<Mutex<usize>>,
    pub progress_lock: Arc<Mutex<()>>,
    pub total_files: u64,
    pub last_update_time: Arc<Mutex<Instant>>,
    pub smoothed_eta_per_file: Arc<Mutex<Option<f64>>>,
}

impl CompressionProgress {
    pub fn new(total_files: u64) -> Self {
        let multi_progress = MultiProgress::new();
        let status_bar = multi_progress.add(ProgressBar::new_spinner());
        status_bar.enable_steady_tick(Duration::from_millis(100));
        status_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .expect("Failed to set spinner template"),
        );

        println!("\n\n");

        // Create the main progress bar
        let total_progress = multi_progress.add(ProgressBar::new(total_files));
        total_progress.set_style(ProgressStyle::default_bar()
            .template("Total: {spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} files ({percent}%) - {msg}")
            .expect("Failed to set progress bar template")
            .progress_chars("-\u{15E7}\u{00BA}"));

        total_progress.enable_steady_tick(Duration::from_millis(300));

        Self {
            multi_progress,
            status_bar,
            total_progress,
            file_counter: Arc::new(Mutex::new(0)),
            progress_lock: Arc::new(Mutex::new(())),
            total_files,
            last_update_time: Arc::new(Mutex::new(Instant::now())),
            smoothed_eta_per_file: Arc::new(Mutex::new(None)),
        }
    }

    pub fn increment_total_progress(&self) {
        // Serialize the updates
        let _lock = self.progress_lock.lock().unwrap();
        let mut counter = self.file_counter.lock().unwrap();
        *counter += 1;

        let now = Instant::now();
        let mut last_time = self.last_update_time.lock().unwrap();
        let duration = now.duration_since(*last_time);

        *last_time = now;

        // Ignore updates that are too fast or too slow
        if duration < Duration::from_millis(10) || duration > Duration::from_secs(30) {
            self.total_progress.set_position(*counter as u64);
            return;
        }

        let latest_secs = duration.as_secs_f64();
        let smoothing_scaler = 0.2;
        let mut smoothed = self.smoothed_eta_per_file.lock().unwrap();

        *smoothed = Some(match *smoothed {
            Some(prev_time) => {
                smoothing_scaler * latest_secs + (1.0 - smoothing_scaler) * prev_time
            }
            None => latest_secs,
        });

        self.total_progress.set_position(*counter as u64);
    }

    pub fn update_eta(self: &Arc<Self>) {
        let progress = self.clone();

        thread::spawn(move || {
            loop {
                {
                    let counter = progress.file_counter.lock().unwrap();
                    let smoothed_eta = progress.smoothed_eta_per_file.lock().unwrap();

                    let min_required = std::cmp::min(10, progress.total_files) as usize;
                    let eta_string = if *counter < min_required || smoothed_eta.is_none() {
                        format!("ETA: Calculating...")
                    } else {
                        let avg_time = smoothed_eta.unwrap_or(0.0).clamp(0.005, 60.0);
                        let remaining = progress.total_files.saturating_sub(*counter as u64) as f64;
                        let eta_secs = avg_time * remaining;
                        let eta_duration = Duration::from_secs(eta_secs as u64);
                        format_eta(eta_duration)
                    };

                    progress.total_progress.set_message(eta_string);
                }

                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    pub fn finish(&self, message: &str) {
        // Serialize the updates
        let _lock = self.progress_lock.lock().unwrap();

        self.total_progress.finish_with_message(message.to_string());
    }
}

pub struct ProgressReader<R: std::io::Read> {
    inner: R,
    progress_bar: ProgressBar,
    bytes_read: u64,
}

impl<R: std::io::Read> ProgressReader<R> {
    pub fn new(inner: R, progress_bar: ProgressBar) -> Self {
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

// Helper functions
fn format_eta(duration: Duration) -> String {
    let secs = duration.as_secs();

    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("ETA: {hours:02}:{minutes:02}:{seconds:02} hours")
    } else if minutes > 0 {
        format!("ETA: {minutes:02}:{seconds:02} minutes")
    } else {
        format!("ETA: {seconds:02} seconds")
    }
}
