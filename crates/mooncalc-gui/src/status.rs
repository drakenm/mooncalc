//! Information shown in the bottom status bar.
//!
//! Tabs write into [`StatusInfo`] through [`crate::tabs::TabContext`]:
//! report running jobs with [`StatusInfo::report_job`], the map cursor with
//! [`StatusInfo::set_hover`], and one-off messages with
//! [`StatusInfo::notify`].

use std::time::{Duration, Instant};

use crate::jobs::JobProgress;

/// How long a [`StatusInfo::notify`] message stays visible.
pub const MESSAGE_DURATION: Duration = Duration::from_secs(6);

/// A transient status message.
#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
    pub shown_at: Instant,
}

/// Per-frame and transient status-bar contents.
#[derive(Clone, Debug, Default)]
pub struct StatusInfo {
    /// Cursor position over a map (lat, lon in degrees), this frame.
    pending_hover: Option<(f64, f64)>,
    /// Cursor position reported in the previous frame (what the bar shows,
    /// because the bar is laid out before the central panel).
    hover: Option<(f64, f64)>,
    /// Jobs reported this frame.
    jobs: Vec<JobProgress>,
    message: Option<StatusMessage>,
}

impl StatusInfo {
    /// Start a new frame: clear per-frame items. Returns whether the hover
    /// position changed since the last frame (the caller then requests a
    /// repaint so the bar catches up).
    pub fn begin_frame(&mut self) -> bool {
        let changed = self.pending_hover != self.hover;
        self.hover = self.pending_hover.take();
        self.jobs.clear();
        if self
            .message
            .as_ref()
            .is_some_and(|m| m.shown_at.elapsed() > MESSAGE_DURATION)
        {
            self.message = None;
        }
        changed
    }

    /// Report the cursor position over a map this frame.
    pub fn set_hover(&mut self, lat_lon_deg: Option<(f64, f64)>) {
        if lat_lon_deg.is_some() {
            self.pending_hover = lat_lon_deg;
        }
    }

    pub fn hover(&self) -> Option<(f64, f64)> {
        self.hover
    }

    /// Report a running job this frame.
    pub fn report_job(&mut self, progress: JobProgress) {
        self.jobs.push(progress);
    }

    pub fn jobs(&self) -> &[JobProgress] {
        &self.jobs
    }

    /// Show an informational message for a few seconds.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.set_message(text.into(), false);
    }

    /// Show an error message for a few seconds.
    pub fn error(&mut self, text: impl Into<String>) {
        self.set_message(text.into(), true);
    }

    fn set_message(&mut self, text: String, is_error: bool) {
        self.message = Some(StatusMessage {
            text,
            is_error,
            shown_at: Instant::now(),
        });
    }

    pub fn message(&self) -> Option<&StatusMessage> {
        self.message.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_is_shown_one_frame_later_and_then_cleared() {
        let mut s = StatusInfo::default();
        s.set_hover(Some((10.0, 20.0)));
        assert!(s.begin_frame());
        assert_eq!(s.hover(), Some((10.0, 20.0)));
        assert!(s.begin_frame());
        assert_eq!(s.hover(), None);
        assert!(!s.begin_frame());
    }

    #[test]
    fn jobs_are_per_frame() {
        let mut s = StatusInfo::default();
        s.report_job(JobProgress {
            label: "x".into(),
            fraction: 0.5,
            elapsed: Duration::ZERO,
        });
        assert_eq!(s.jobs().len(), 1);
        s.begin_frame();
        assert!(s.jobs().is_empty());
    }
}
