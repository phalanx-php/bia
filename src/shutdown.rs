use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

static FLAG: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

pub fn flag() -> Arc<AtomicBool> {
    Arc::clone(&FLAG)
}

pub fn requested() -> bool {
    FLAG.load(Ordering::Relaxed)
}
