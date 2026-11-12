//! Native parallel evaluation of map row blocks.
//!
//! `mooncalc_app::map::MapJob` is single-threaded and portable; here its rows
//! are split into chunks ([`mooncalc_app::map::row_chunks`]) that scoped
//! worker threads pull from a shared counter, so faster threads take more
//! chunks. The blocks are merged with `MapJob::assemble`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use mooncalc_app::dto::ApiError;
use mooncalc_app::map::{DEFAULT_ROW_CHUNK, MapJob, MapOutput, row_chunks};

/// Worker threads to use: the available parallelism, at least one.
pub fn default_threads() -> usize {
    thread::available_parallelism().map_or(1, |n| n.get())
}

/// Evaluate every row of `job` on up to `threads` threads. `keep_going` is
/// polled between chunks; when it returns `false` the evaluation stops with
/// a `cancelled` error. Returns the map and the number of threads used.
pub fn evaluate_parallel(
    job: &MapJob,
    threads: usize,
    keep_going: impl Fn() -> bool + Sync,
) -> Result<(MapOutput, usize), ApiError> {
    let chunks = row_chunks(job.rows(), DEFAULT_ROW_CHUNK);
    let threads = threads.clamp(1, chunks.len().max(1));
    let next = AtomicUsize::new(0);
    let worker = || {
        let mut blocks = Vec::new();
        loop {
            if !keep_going() {
                return Err(ApiError::cancelled());
            }
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(rows) = chunks.get(i) else {
                return Ok(blocks);
            };
            blocks.push(job.evaluate_rows(rows.clone()));
        }
    };
    let parts: Vec<Result<_, ApiError>> = if threads == 1 {
        vec![worker()]
    } else {
        thread::scope(|s| {
            let handles: Vec<_> = (0..threads).map(|_| s.spawn(worker)).collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(ApiError::internal("a map worker panicked")))
                })
                .collect()
        })
    };
    let mut blocks = Vec::with_capacity(chunks.len());
    for p in parts {
        blocks.extend(p?);
    }
    Ok((job.assemble(blocks), threads))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncalc_app::dto::{CivilDate, Crescent, MapParams};
    use mooncalc_app::ephemeris::meeus;

    fn job() -> MapJob {
        MapJob::prepare(
            &*meeus(false),
            &MapParams {
                date: CivilDate {
                    year: 2024,
                    month: 3,
                    day: 10,
                },
                criterion_id: "yallop".into(),
                crescent: Crescent::Evening,
                resolution_deg: 5.0,
                bounds: None,
                ephemeris_id: None,
                include_values: true,
            },
        )
        .unwrap()
    }

    #[test]
    fn parallel_matches_serial() {
        let job = job();
        let serial = job.evaluate();
        let (par, used) = evaluate_parallel(&job, 4, || true).unwrap();
        assert!((1..=4).contains(&used));
        assert_eq!(par.grid, serial.grid);
        assert_eq!(par.field.status, serial.field.status);
        assert!(
            par.field
                .values
                .iter()
                .zip(&serial.field.values)
                .all(|(a, b)| a == b || (a.is_nan() && b.is_nan()))
        );
    }

    #[test]
    fn cancellation() {
        let err = evaluate_parallel(&job(), 3, || false).unwrap_err();
        assert_eq!(err.code, "cancelled");
    }
}
