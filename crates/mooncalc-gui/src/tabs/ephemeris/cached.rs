//! A background job keyed by its request: recompute when the request
//! changes, keep the last result, report progress to the status bar.

use crate::jobs::{JobContext, JobError, JobSlot};
use crate::status::StatusInfo;

/// The result of the most recent request `R`, computed on a job.
pub struct CachedJob<R, T> {
    slot: JobSlot<T>,
    requested: Option<R>,
    /// Last finished result and the request it belongs to.
    pub value: Option<(R, T)>,
    pub error: Option<String>,
    pending: Option<R>,
}

impl<R, T> Default for CachedJob<R, T> {
    fn default() -> Self {
        Self {
            slot: JobSlot::default(),
            requested: None,
            value: None,
            error: None,
            pending: None,
        }
    }
}

impl<R, T> CachedJob<R, T>
where
    R: Clone + PartialEq + Send + 'static,
    T: Send + 'static,
{
    /// Submit `work` unless `req` is already computed or running.
    pub fn request<F>(&mut self, ctx: &egui::Context, req: R, label: &str, work: F)
    where
        F: FnOnce(&R, &JobContext) -> Result<T, JobError> + Send + 'static,
    {
        if self.requested.as_ref() == Some(&req) {
            return;
        }
        self.requested = Some(req.clone());
        self.pending = Some(req.clone());
        let repaint = ctx.clone();
        self.slot.submit(
            label,
            move || repaint.request_repaint(),
            move |job| work(&req, job),
        );
    }

    /// Collect a finished result and report a running job's progress.
    pub fn poll(&mut self, status: &mut StatusInfo) {
        match self.slot.poll() {
            Some(Ok(v)) => {
                if let Some(req) = self.pending.take() {
                    self.value = Some((req, v));
                }
                self.error = None;
            }
            Some(Err(e)) => {
                self.pending = None;
                status.error(format!("Ephemeris: {e}"));
                self.error = Some(e);
            }
            None => {}
        }
        if let Some(p) = self.slot.progress() {
            status.report_job(p);
        }
    }

    pub fn is_running(&self) -> bool {
        self.slot.is_running()
    }

    /// The last result, whatever request it belongs to.
    pub fn get(&self) -> Option<&T> {
        self.value.as_ref().map(|(_, v)| v)
    }
}
