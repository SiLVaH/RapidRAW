//! Offline-first conversion queue. Recipes edit without a camera; renders
//! drain when a camera in USB RAW CONV. mode is connected.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;

use crate::fuji_raw_conv::cache::RenderStatus;
use crate::fuji_raw_conv::preset::FujiRecipe;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueJob {
    pub id: String,
    pub source_path: String,
    pub recipe: FujiRecipe,
    pub raf_hash: String,
    pub cache_key: String,
    pub status: RenderStatus,
    pub error: Option<String>,
    pub created_unix_ms: u64,
}

#[derive(Debug, Default)]
pub struct ConvertQueue {
    inner: Mutex<VecDeque<QueueJob>>,
}

impl ConvertQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&self, job: QueueJob) {
        let mut q = self.inner.lock().unwrap();
        // Replace existing job for same source+recipe.
        q.retain(|j| j.cache_key != job.cache_key);
        q.push_back(job);
    }

    pub fn list(&self) -> Vec<QueueJob> {
        self.inner.lock().unwrap().iter().cloned().collect()
    }

    pub fn pop_next(&self) -> Option<QueueJob> {
        let mut q = self.inner.lock().unwrap();
        q.pop_front()
    }

    pub fn peek(&self) -> Option<QueueJob> {
        self.inner.lock().unwrap().front().cloned()
    }

    pub fn update_status(&self, id: &str, status: RenderStatus, error: Option<String>) {
        let mut q = self.inner.lock().unwrap();
        if let Some(job) = q.iter_mut().find(|j| j.id == id) {
            job.status = status;
            job.error = error;
        }
    }

    pub fn remove(&self, id: &str) {
        let mut q = self.inner.lock().unwrap();
        q.retain(|j| j.id != id);
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

unsafe impl Send for ConvertQueue {}
unsafe impl Sync for ConvertQueue {}
