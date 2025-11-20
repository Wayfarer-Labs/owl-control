use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// A thread-safe queue for managing pending recording uploads.
#[derive(Clone)]
pub struct UploadQueue {
    inner: Arc<Mutex<UploadQueueInner>>,
}

struct UploadQueueInner {
    /// Queue of pending recording folder paths
    pending: VecDeque<PathBuf>,
    /// Currently uploading recording path (if any)
    current: Option<PathBuf>,
}

impl UploadQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(UploadQueueInner {
                pending: VecDeque::new(),
                current: None,
            })),
        }
    }

    /// Add a recording to the upload queue
    pub fn enqueue(&self, path: PathBuf) {
        let mut inner = self.inner.lock().unwrap();

        // Don't add duplicates
        if inner.pending.contains(&path) || inner.current.as_ref() == Some(&path) {
            tracing::debug!("Recording {} already in queue, skipping", path.display());
            return;
        }

        tracing::info!("Enqueuing recording for upload: {}", path.display());
        inner.pending.push_back(path);
    }

    /// Get the next recording to upload, marking it as current
    pub fn dequeue(&self) -> Option<PathBuf> {
        let mut inner = self.inner.lock().unwrap();
        let path = inner.pending.pop_front()?;
        inner.current = Some(path.clone());
        tracing::info!("Dequeued recording for upload: {}", path.display());
        Some(path)
    }

    /// Mark the current upload as complete
    pub fn complete_current(&self) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(path) = inner.current.take() {
            tracing::info!("Completed upload of: {}", path.display());
        }
    }

    /// Clear all pending uploads (but not the current one)
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        let count = inner.pending.len();
        inner.pending.clear();
        if count > 0 {
            tracing::info!("Cleared {} pending uploads from queue", count);
        }
    }

    /// Clear all pending uploads AND mark current as cancelled
    pub fn clear_all(&self) {
        let mut inner = self.inner.lock().unwrap();
        let pending_count = inner.pending.len();
        inner.pending.clear();
        inner.current = None;
        tracing::info!("Cleared all uploads from queue ({} pending)", pending_count);
    }

    /// Check if queue is empty (no pending and no current)
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.pending.is_empty() && inner.current.is_none()
    }

    /// Get the number of pending uploads (not including current)
    pub fn pending_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.pending.len()
    }

    /// Get the current upload path (if any)
    pub fn current(&self) -> Option<PathBuf> {
        let inner = self.inner.lock().unwrap();
        inner.current.clone()
    }

    /// Get queue statistics
    pub fn stats(&self) -> QueueStats {
        let inner = self.inner.lock().unwrap();
        QueueStats {
            pending_count: inner.pending.len(),
            has_current: inner.current.is_some(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueueStats {
    pub pending_count: usize,
    pub has_current: bool,
}
