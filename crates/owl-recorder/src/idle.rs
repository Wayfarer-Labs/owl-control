pub(crate) struct IdlenessTracker {
    idle_duration: std::time::Duration,
    last_active: std::time::Instant,
}

impl IdlenessTracker {
    pub(crate) fn new(idle_duration: std::time::Duration) -> Self {
        Self {
            idle_duration,
            last_active: std::time::Instant::now(),
        }
    }

    pub(crate) fn update_activity(&mut self) {
        self.last_active = std::time::Instant::now();
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.last_active.elapsed() >= self.idle_duration
    }
}
