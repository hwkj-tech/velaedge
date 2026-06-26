use std::sync::{Arc, Mutex};

use cloud_control::CloudControlStore;

#[derive(Clone, Default)]
pub struct AppState {
    pub store: Arc<Mutex<CloudControlStore>>,
}
