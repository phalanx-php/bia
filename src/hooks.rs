use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ripht_php_sapi::{ExecutionHooks, ExecutionMessage, OutputAction};

pub struct DoryHooks {
    shutdown: Arc<AtomicBool>,
}

impl DoryHooks {
    pub fn new(shutdown: Arc<AtomicBool>) -> Self {
        Self { shutdown }
    }
}

impl ExecutionHooks for DoryHooks {
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        use std::io::Write;
        let _ = std::io::stdout().write_all(data);
        OutputAction::Done
    }

    fn on_php_message(&mut self, message: &ExecutionMessage) {
        eprintln!("{}", message.message);
    }

    fn on_script_executing(&mut self, script_path: &Path) {
        let _ = script_path;
    }

    fn is_connection_alive(&self) -> bool {
        !self.shutdown.load(Ordering::Relaxed)
    }
}
