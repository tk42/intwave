//! Progress reporting and cooperative cancellation.
//!
//! The engine is synchronous and caller-driven: callers pass a [`ProgressSink`]
//! and a [`CancelToken`]. The CLI drives them on the main thread; the GUI drives
//! them from a `spawn_blocking` task, forwarding progress to Tauri events and
//! flipping the token on a cancel button. Progress is an **integer permille**
//! (0..=1000) so the engine source stays float-free.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{EngineError, EngineResult, ErrorCode};

/// Receives coarse progress updates, in permille (0..=1000).
pub trait ProgressSink {
    fn set_permille(&self, permille: u32);
}

/// A sink that discards progress (for the CLI's quiet path and tests).
pub struct NoProgress;

impl ProgressSink for NoProgress {
    fn set_permille(&self, _permille: u32) {}
}

/// Adapts any `Fn(u32)` into a [`ProgressSink`].
pub struct FnProgress<F>(pub F);

impl<F: Fn(u32)> ProgressSink for FnProgress<F> {
    fn set_permille(&self, permille: u32) {
        (self.0)(permille)
    }
}

/// A cheap, cloneable cancellation flag checked between processing stages.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Return `Err(CANCELLED)` if cancellation was requested.
    pub fn check(&self) -> EngineResult<()> {
        if self.is_cancelled() {
            Err(EngineError::new(
                ErrorCode::Cancelled,
                "operation cancelled",
            ))
        } else {
            Ok(())
        }
    }
}
