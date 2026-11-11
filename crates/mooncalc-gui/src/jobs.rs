//! Background jobs: run expensive computations (map grids, tables, calendar
//! scans) off the UI thread, with progress, cancellation and
//! replace-on-new-request.
//!
//! # Usage
//!
//! A feature keeps one [`JobSlot`] per kind of result. When its inputs
//! change it calls [`JobSlot::submit`], which cancels the previous job of
//! that slot and starts the new one. Every frame it calls
//! [`JobSlot::poll`] to pick up a finished result.
//!
//! ```
//! use mooncalc_gui::jobs::{JobContext, JobError, JobSlot};
//!
//! let mut slot: JobSlot<u64> = JobSlot::new();
//! slot.submit("Summing", || { /* e.g. egui ctx.request_repaint() */ }, |job: &JobContext| {
//!     let mut sum = 0u64;
//!     for i in 0..1000u64 {
//!         job.checkpoint()?; // returns Err(JobError::Cancelled) once cancelled
//!         job.set_progress(i as f32 / 1000.0);
//!         sum += i;
//!     }
//!     Ok::<_, JobError>(sum)
//! });
//! let result = loop {
//!     if let Some(r) = slot.poll() { break r; }
//!     std::thread::yield_now();
//! };
//! assert_eq!(result, Ok(499_500));
//! ```
//!
//! Inside a job, [`parallel_rows`] spreads row-wise work (one grid row per
//! call) over all CPU cores while keeping progress and cancellation.
//!
//! Cancellation is cooperative: the work closure must call
//! [`JobContext::checkpoint`] (or [`JobContext::is_cancelled`]) regularly.
//! A cancelled job's thread runs until its next checkpoint and its result is
//! discarded. A panic inside a job is caught and reported as
//! [`JobError::Failed`].

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Why a job did not produce a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobError {
    /// The job was cancelled (explicitly, or replaced by a newer request).
    Cancelled,
    /// The job returned an error or panicked.
    Failed(String),
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::Cancelled => f.write_str("cancelled"),
            JobError::Failed(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for JobError {}

#[derive(Debug, Default)]
struct Shared {
    cancelled: AtomicBool,
    finished: AtomicBool,
    /// Progress fraction in `[0, 1]`, stored as `f32` bits.
    progress: AtomicU32,
}

/// Handle given to the work closure: check for cancellation and report
/// progress. Cheap to clone and `Send + Sync`, so it can be shared with
/// helper threads.
#[derive(Clone, Debug)]
pub struct JobContext {
    shared: Arc<Shared>,
}

impl JobContext {
    /// A context that is never cancelled, for running job code synchronously
    /// (tests, command-line tools).
    pub fn detached() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
        }
    }

    /// Whether the job has been asked to stop.
    pub fn is_cancelled(&self) -> bool {
        self.shared.cancelled.load(Ordering::Relaxed)
    }

    /// `Err(JobError::Cancelled)` once the job has been asked to stop; use
    /// with `?` inside loops.
    pub fn checkpoint(&self) -> Result<(), JobError> {
        if self.is_cancelled() {
            Err(JobError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Report progress as a fraction in `[0, 1]`.
    pub fn set_progress(&self, fraction: f32) {
        let f = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.shared.progress.store(f.to_bits(), Ordering::Relaxed);
    }

    /// Last reported progress.
    pub fn progress(&self) -> f32 {
        f32::from_bits(self.shared.progress.load(Ordering::Relaxed))
    }
}

/// One running (or finished) background job producing a `T`.
pub struct Job<T> {
    label: String,
    ctx: JobContext,
    rx: mpsc::Receiver<Result<T, JobError>>,
    started: Instant,
}

impl<T: Send + 'static> Job<T> {
    /// Start `work` on a new thread. `on_finish` runs on that thread after
    /// the result is sent (successful, failed or cancelled); pass a closure
    /// that requests a repaint, e.g. `move || ctx.request_repaint()`.
    pub fn spawn<F, N>(label: impl Into<String>, on_finish: N, work: F) -> Self
    where
        F: FnOnce(&JobContext) -> Result<T, JobError> + Send + 'static,
        N: FnOnce() + Send + 'static,
    {
        let label = label.into();
        let ctx = JobContext::detached();
        let (tx, rx) = mpsc::channel();
        let thread_ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("job: {label}"))
            .spawn(move || {
                let result = match catch_unwind(AssertUnwindSafe(|| work(&thread_ctx))) {
                    Ok(Ok(_)) if thread_ctx.is_cancelled() => Err(JobError::Cancelled),
                    Ok(r) => r,
                    Err(panic) => Err(JobError::Failed(panic_message(&panic))),
                };
                if result.is_ok() {
                    thread_ctx.set_progress(1.0);
                }
                thread_ctx.shared.finished.store(true, Ordering::Release);
                // The receiver is gone if the job was replaced; that's fine.
                let _ = tx.send(result);
                on_finish();
            });
        if let Err(e) = spawned {
            ctx.shared.finished.store(true, Ordering::Release);
            // `rx` will report the failure through a disconnected channel.
            log_spawn_error(&label, &e);
        }
        Self {
            label,
            ctx,
            rx,
            started: Instant::now(),
        }
    }
}

impl<T> Job<T> {
    /// Ask the job to stop at its next checkpoint.
    pub fn cancel(&self) {
        self.ctx.shared.cancelled.store(true, Ordering::Relaxed);
    }

    /// Whether the work closure has returned.
    pub fn is_finished(&self) -> bool {
        self.ctx.shared.finished.load(Ordering::Acquire)
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn progress(&self) -> f32 {
        self.ctx.progress()
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// The result, if the job has finished; never blocks.
    /// Returns `Some` at most once.
    pub fn try_take(&mut self) -> Option<Result<T, JobError>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(JobError::Failed(
                "job thread ended without a result".into(),
            ))),
        }
    }

    /// Block until the job finishes.
    pub fn wait(self) -> Result<T, JobError> {
        self.rx
            .recv()
            .unwrap_or_else(|_| Err(JobError::Failed("job thread ended without a result".into())))
    }
}

impl<T> Drop for Job<T> {
    /// Dropping a job handle cancels it: nobody can receive its result.
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Progress snapshot of a running job, for the status bar.
#[derive(Clone, Debug, PartialEq)]
pub struct JobProgress {
    pub label: String,
    pub fraction: f32,
    pub elapsed: Duration,
}

/// Holds at most one job; submitting a new one cancels the old one
/// (replace-on-new-request).
pub struct JobSlot<T> {
    current: Option<Job<T>>,
    generation: u64,
}

impl<T> Default for JobSlot<T> {
    fn default() -> Self {
        Self {
            current: None,
            generation: 0,
        }
    }
}

impl<T: Send + 'static> JobSlot<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel any running job and start `work`. Returns the new job's
    /// generation number (increments on every submit).
    pub fn submit<F, N>(&mut self, label: impl Into<String>, on_finish: N, work: F) -> u64
    where
        F: FnOnce(&JobContext) -> Result<T, JobError> + Send + 'static,
        N: FnOnce() + Send + 'static,
    {
        self.cancel();
        self.generation += 1;
        self.current = Some(Job::spawn(label, on_finish, work));
        self.generation
    }

    /// The current job's result once it has finished. Cancelled results are
    /// swallowed (the caller asked for that); failures are returned as
    /// `Err(message)`. Returns `Some` once per job.
    pub fn poll(&mut self) -> Option<Result<T, String>> {
        let result = self.current.as_mut()?.try_take()?;
        self.current = None;
        match result {
            Ok(v) => Some(Ok(v)),
            Err(JobError::Cancelled) => None,
            Err(JobError::Failed(msg)) => Some(Err(msg)),
        }
    }
}

impl<T> JobSlot<T> {
    /// Cancel the running job, if any.
    pub fn cancel(&mut self) {
        if let Some(job) = self.current.take() {
            job.cancel();
        }
    }

    /// Whether a job has been submitted and its result not yet polled.
    pub fn is_running(&self) -> bool {
        self.current.is_some()
    }

    /// Generation number of the most recent submit (0 before the first).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Progress of the current job, if one is running.
    pub fn progress(&self) -> Option<JobProgress> {
        self.current.as_ref().map(|job| JobProgress {
            label: job.label.clone(),
            fraction: job.progress(),
            elapsed: job.elapsed(),
        })
    }
}

/// Evaluate `f(row)` for every row in `0..rows` on all available cores,
/// returning the results in row order.
///
/// Rows are handed out one at a time from a shared counter, so uneven rows
/// balance automatically. Progress is reported as the fraction of rows done
/// (`ctx.set_progress`); a cancelled `ctx` stops all threads at their next
/// row and returns `Err(JobError::Cancelled)`.
pub fn parallel_rows<T, F>(ctx: &JobContext, rows: usize, f: F) -> Result<Vec<T>, JobError>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(rows)
        .max(1);
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let worker = || {
        let mut out = Vec::new();
        loop {
            if ctx.is_cancelled() {
                break;
            }
            let row = next.fetch_add(1, Ordering::Relaxed);
            if row >= rows {
                break;
            }
            out.push((row, f(row)));
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            ctx.set_progress(d as f32 / rows as f32);
        }
        out
    };
    let parts: Vec<Vec<(usize, T)>> = std::thread::scope(|s| {
        let handles: Vec<_> = (1..threads).map(|_| s.spawn(worker)).collect();
        let mut parts = vec![worker()];
        for h in handles {
            match h.join() {
                Ok(p) => parts.push(p),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        parts
    });
    ctx.checkpoint()?;
    let mut slots: Vec<Option<T>> = std::iter::repeat_with(|| None).take(rows).collect();
    for (row, value) in parts.into_iter().flatten() {
        slots[row] = Some(value);
    }
    slots
        .into_iter()
        .map(|v| v.ok_or_else(|| JobError::Failed("row was not computed".into())))
        .collect()
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        format!("job panicked: {s}")
    } else if let Some(s) = panic.downcast_ref::<String>() {
        format!("job panicked: {s}")
    } else {
        "job panicked".into()
    }
}

fn log_spawn_error(label: &str, e: &std::io::Error) {
    eprintln!("mooncalc: could not start job thread for {label:?}: {e}");
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(10);

    fn poll_until<T: Send + 'static>(slot: &mut JobSlot<T>) -> Option<Result<T, String>> {
        let start = Instant::now();
        while start.elapsed() < TIMEOUT {
            if let Some(r) = slot.poll() {
                return Some(r);
            }
            if !slot.is_running() {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("job did not finish within {TIMEOUT:?}");
    }

    /// A job that spins until cancelled, signalling once it has started.
    fn spin_until_cancelled(
        started: mpsc::Sender<()>,
    ) -> impl FnOnce(&JobContext) -> Result<u32, JobError> {
        move |job: &JobContext| {
            started.send(()).unwrap();
            let start = Instant::now();
            loop {
                job.checkpoint()?;
                if start.elapsed() > TIMEOUT {
                    return Ok(0); // never cancelled: let the test fail on the value
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn job_delivers_its_result_and_calls_on_finish() {
        let (done_tx, done_rx) = mpsc::channel();
        let job = Job::spawn(
            "answer",
            move || done_tx.send(()).unwrap(),
            |job| {
                job.set_progress(0.5);
                Ok(42)
            },
        );
        assert_eq!(job.wait(), Ok(42));
        done_rx.recv_timeout(TIMEOUT).unwrap();
    }

    #[test]
    fn cancelled_job_stops_and_reports_cancelled() {
        let (started_tx, started_rx) = mpsc::channel();
        let job = Job::spawn("spin", || {}, spin_until_cancelled(started_tx));
        started_rx.recv_timeout(TIMEOUT).unwrap();
        job.cancel();
        assert_eq!(job.wait(), Err(JobError::Cancelled));
    }

    #[test]
    fn submit_replaces_and_cancels_the_previous_job() {
        let mut slot = JobSlot::new();
        let (started_tx, started_rx) = mpsc::channel();
        let (old_done_tx, old_done_rx) = mpsc::channel();
        slot.submit(
            "old",
            move || old_done_tx.send(()).unwrap(),
            spin_until_cancelled(started_tx),
        );
        started_rx.recv_timeout(TIMEOUT).unwrap();
        let generation = slot.submit("new", || {}, |_| Ok(7));
        assert_eq!(generation, 2);
        // The old job notices the cancellation and finishes promptly.
        old_done_rx.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(poll_until(&mut slot), Some(Ok(7)));
        assert!(!slot.is_running());
        assert_eq!(slot.poll(), None);
    }

    #[test]
    fn explicit_cancel_swallows_the_result() {
        let mut slot: JobSlot<u32> = JobSlot::new();
        let (started_tx, started_rx) = mpsc::channel();
        slot.submit("spin", || {}, spin_until_cancelled(started_tx));
        started_rx.recv_timeout(TIMEOUT).unwrap();
        assert!(slot.progress().is_some());
        slot.cancel();
        assert!(!slot.is_running());
        assert!(slot.progress().is_none());
        assert_eq!(slot.poll(), None);
    }

    #[test]
    fn errors_and_panics_are_reported_as_failures() {
        let mut slot: JobSlot<u32> = JobSlot::new();
        slot.submit("err", || {}, |_| Err(JobError::Failed("no data".into())));
        assert_eq!(poll_until(&mut slot), Some(Err("no data".into())));
        slot.submit("panic", || {}, |_| panic!("boom"));
        let r = poll_until(&mut slot).unwrap();
        assert!(r.unwrap_err().contains("boom"));
    }

    #[test]
    fn parallel_rows_preserves_order_and_reports_progress() {
        let ctx = JobContext::detached();
        let rows = parallel_rows(&ctx, 1000, |r| r * r).unwrap();
        assert_eq!(rows.len(), 1000);
        assert!(rows.iter().enumerate().all(|(i, &v)| v == i * i));
        assert_eq!(ctx.progress(), 1.0);
        assert_eq!(parallel_rows(&ctx, 0, |r| r).unwrap(), Vec::<usize>::new());
    }

    #[test]
    fn parallel_rows_stops_when_cancelled() {
        let (started_tx, started_rx) = mpsc::channel();
        let job = Job::spawn(
            "rows",
            || {},
            move |job| {
                let started_tx = std::sync::Mutex::new(started_tx);
                parallel_rows(job, 1_000_000, |_| {
                    let _ = started_tx.lock().unwrap().send(());
                    std::thread::sleep(Duration::from_millis(1));
                })
            },
        );
        started_rx.recv_timeout(TIMEOUT).unwrap();
        job.cancel();
        assert_eq!(job.wait(), Err(JobError::Cancelled));
    }
}
