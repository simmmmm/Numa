use alloc::{string::String, vec::Vec};

use parking_lot::Mutex;

pub static VALIDATION_CANARY: ValidationCanary = ValidationCanary {
    inner: Mutex::new(Vec::new()),
};

#[derive(Debug)]
pub struct ValidationCanary {
    inner: Mutex<Vec<String>>,
}

impl ValidationCanary {
    #[allow(dead_code, reason = "in some configurations this function is dead")]
    pub(crate) fn add(&self, msg: String) {
        self.inner.lock().push(msg);
    }

    pub fn get_and_reset(&self) -> Vec<String> {
        self.inner.lock().drain(..).collect()
    }
}
