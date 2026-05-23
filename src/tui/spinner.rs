pub const SPINNER_FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];

#[derive(Debug, Clone)]
pub struct Spinner {
    pub frame_idx: usize,
}

impl Default for Spinner {
    fn default() -> Self {
        Self { frame_idx: 0 }
    }
}

impl Spinner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self) -> &str {
        let frame = SPINNER_FRAMES[self.frame_idx];
        self.frame_idx = (self.frame_idx + 1) % SPINNER_FRAMES.len();
        frame
    }

    pub fn current(&self) -> &str {
        SPINNER_FRAMES[self.frame_idx]
    }
}
