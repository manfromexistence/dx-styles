use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use libc::{cpu_set_t, sched_setaffinity, CPU_SET};
use memmap2::MmapMut;
use rayon::prelude::*;

const NUM_FILES: usize = 10000;
// Solution: Define a BATCH_SIZE to process files in manageable chunks.
// This directly addresses all three potential problems:
// 1. I/O Bound: Prevents overwhelming the disk by limiting concurrent I/O.
// 2. OS Limits: Keeps the number of simultaneously open files well below system limits.
// 3. Memory Pressure: Ensures memory usage stays low by only loading/mapping one batch at a time.
const BATCH_SIZE: usize = 1024;
const CONTENT: &[u8] = b"initial content padded to simulate dx-check workload....................100 bytes..";
const UPDATE_CONTENT: &[u8] = b"updated content padded to simulate dx-check workload....................100 bytes..";

fn get_dir() -> PathBuf {
    let mut path = env::temp_dir();
    path.push("modules");
    path
}

fn pin_thread(core_id: usize) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let mut cpu_set: cpu_set_t = std::mem::zeroed();
            CPU_SET(core_id, &mut cpu_set);
            let result = sched_setaffinity(0, std::mem::size_of::<cpu_set_t>(), &cpu_set);
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

fn run_in_pinned_pool<F>(benchmark_fn: F) -> io::Result<()>
where
    F: FnOnce() -> io::Result<()> + Send,
{
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(rayon::current_num_threads())
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    pool.install(|| {
        (0..rayon::current_num_threads())
            .into_par_iter()
            .for_each(|id| {
                let _ = pin_thread(id);
            });
        benchmark_fn()
    })
}

// The core logic of this function is UNCHANGED.
// It still processes a slice of paths in parallel.
fn create_files(paths: &[PathBuf]) -> io::Result<()> {
    paths.par_iter().try_for_each(|path| {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(CONTENT)?;
        writer.flush()?;
        Ok::<(), io::Error>(())
    })
}

// The core logic of this function is UNCHANGED.
fn read_files(paths: &[PathBuf]) -> io::Result<()> {
    paths.par_iter().try_for_each(|path| {
        let mut file = File::open(path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok::<(), io::Error>(())
    })
}

// The core logic of this function is UNCHANGED.
fn update_files_smartly(paths: &[PathBuf]) -> io::Result<()> {
    paths.par_iter().try_for_each(|path| {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut mmap = unsafe { MmapMut::map_mut(&file)? };
        if mmap.len() < UPDATE_CONTENT.len() {
            file.set_len(UPDATE_CONTENT.len() as u64)?;
            mmap = unsafe { MmapMut::map_mut(&file)? };
        }
        mmap[..UPDATE_CONTENT.len()].copy_from_slice(UPDATE_CONTENT);
        Ok::<(), io::Error>(())
    })
}

// The core logic of this function is UNCHANGED.
fn delete_files(paths: &[PathBuf]) -> io::Result<()> {
    paths.par_iter().try_for_each(fs::remove_file)
}

// MODIFIED: This function now orchestrates the I/O operations in batches.
fn dx_io() -> io::Result<()> {
    let dir_path = get_dir();
    let file_paths: Vec<_> = (0..NUM_FILES)
        .map(|i| dir_path.join(format!("file_{}.txt", i)))
        .collect();

    // --- Create Operation in Batches ---
    let mut total_create_time = Duration::new(0, 0);
    for batch in file_paths.chunks(BATCH_SIZE) {
        let start = Instant::now();
        create_files(batch)?;
        total_create_time += start.elapsed();
    }
    let create_time = total_create_time.as_millis();

    // --- Read Operation in Batches ---
    let mut total_read_time = Duration::new(0, 0);
    for batch in file_paths.chunks(BATCH_SIZE) {
        let start = Instant::now();
        read_files(batch)?;
        total_read_time += start.elapsed();
    }
    let read_time = total_read_time.as_millis();

    // --- Update Operation in Batches ---
    let mut total_update_time = Duration::new(0, 0);
    for batch in file_paths.chunks(BATCH_SIZE) {
        let start = Instant::now();
        update_files_smartly(batch)?;
        total_update_time += start.elapsed();
    }
    let update_time = total_update_time.as_millis();

    // --- Delete Operation in Batches ---
    let mut total_delete_time = Duration::new(0, 0);
    for batch in file_paths.chunks(BATCH_SIZE) {
        let start = Instant::now();
        delete_files(batch)?;
        total_delete_time += start.elapsed();
    }
    let delete_time = total_delete_time.as_millis();

    println!(
        "I/O operation times (ms): Create: {}, Read: {}, Update: {}, Delete: {}",
        create_time, read_time, update_time, delete_time
    );
    println!(
        "Total: {} ms",
        create_time + read_time + update_time + delete_time
    );
    Ok(())
}

fn main() -> io::Result<()> {
    let dir_path = get_dir();
    fs::create_dir_all(&dir_path)?;

    println!("\nRunning dx_io...");
    run_in_pinned_pool(dx_io)?;

    fs::remove_dir_all(&dir_path)?;
    Ok(())
}
