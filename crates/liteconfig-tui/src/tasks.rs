//! Background task runner for long-running operations (sync, clone, push).
//!
//! The TUI runs on a single thread; a `Task` moves the slow work (git clone,
//! copying 1000+ skills, git push) onto a worker thread so the UI stays
//! responsive. Results flow back over an `mpsc` channel that the event loop
//! drains each tick and translates into `TaskLogEntry` updates.
//!
//! Scope is intentionally small — one worker thread, FIFO queue, no
//! cancellation. Pulling in `tokio` for this one case would be overkill.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

/// Auto-incrementing id for tasks; used by the log to correlate submission
/// with completion.
pub type TaskId = u64;

#[derive(Debug, Clone)]
pub enum TaskStatus {
    Running,
    Ok(String),
    Err(String),
}

#[derive(Debug, Clone)]
pub struct TaskLogEntry {
    pub id: TaskId,
    pub name: String,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
    pub status: TaskStatus,
}

/// Message sent from the worker thread back to the main thread.
pub struct TaskResult {
    pub id: TaskId,
    pub status: TaskStatus,
}

type Work = Box<dyn FnOnce() -> Result<String, String> + Send + 'static>;

struct Job {
    id: TaskId,
    work: Work,
}

/// Owns a single worker thread and a channel for completion messages.
pub struct TaskRunner {
    next_id: TaskId,
    submit_tx: Sender<Job>,
    result_rx: Receiver<TaskResult>,
    log: VecDeque<TaskLogEntry>,
    /// Kept for visibility only — the worker thread exits when `submit_tx`
    /// is dropped.
    _worker: thread::JoinHandle<()>,
}

impl TaskRunner {
    pub fn new() -> Self {
        let (submit_tx, submit_rx) = mpsc::channel::<Job>();
        let (result_tx, result_rx) = mpsc::channel::<TaskResult>();

        let worker = thread::spawn(move || {
            // Single-consumer; `recv` returns Err when the sender is dropped,
            // which is how we shut down.
            while let Ok(Job { id, work }) = submit_rx.recv() {
                let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
                    Ok(Ok(msg)) => TaskStatus::Ok(msg),
                    Ok(Err(e)) => TaskStatus::Err(e),
                    Err(_) => TaskStatus::Err("task panicked".to_string()),
                };
                // If the UI thread is gone we just stop; no error handling
                // makes sense at that point.
                if result_tx
                    .send(TaskResult {
                        id,
                        status: outcome,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            next_id: 1,
            submit_tx,
            result_rx,
            log: VecDeque::with_capacity(64),
            _worker: worker,
        }
    }

    /// Queue a new background job. Returns the `TaskId` that will appear in
    /// the log. The result is delivered the next time `drain_completed` runs.
    pub fn submit<F>(&mut self, name: impl Into<String>, work: F) -> TaskId
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;
        let name = name.into();
        self.log.push_front(TaskLogEntry {
            id,
            name,
            started_at: Instant::now(),
            finished_at: None,
            status: TaskStatus::Running,
        });
        while self.log.len() > 50 {
            self.log.pop_back();
        }
        // We shouldn't fail to send unless the worker is gone; swallow the
        // error to keep the caller simple.
        let _ = self.submit_tx.send(Job {
            id,
            work: Box::new(work),
        });
        id
    }

    /// Pull any newly-completed results off the channel and apply them to the
    /// log. Returns the entries that just transitioned out of `Running`.
    pub fn drain_completed(&mut self) -> Vec<TaskLogEntry> {
        let mut done = Vec::new();
        while let Ok(TaskResult { id, status }) = self.result_rx.try_recv() {
            if let Some(entry) = self.log.iter_mut().find(|e| e.id == id) {
                entry.status = status;
                entry.finished_at = Some(Instant::now());
                done.push(entry.clone());
            }
        }
        done
    }

    pub fn log(&self) -> &VecDeque<TaskLogEntry> {
        &self.log
    }

    pub fn running_count(&self) -> usize {
        self.log
            .iter()
            .filter(|e| matches!(e.status, TaskStatus::Running))
            .count()
    }
}

impl Default for TaskRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared handle useful when a task needs to read live settings / DB paths
/// without owning them. Most tasks don't need this — they capture owned data.
pub type SharedRunner = Arc<Mutex<TaskRunner>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn wait_for<F: FnMut() -> bool>(mut cond: F) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if cond() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("condition never became true");
    }

    #[test]
    fn submit_and_collect_ok_result() {
        let mut runner = TaskRunner::new();
        let id = runner.submit("hello", || Ok("world".to_string()));
        assert_eq!(runner.running_count(), 1);
        wait_for(|| {
            let done = runner.drain_completed();
            done.iter().any(|e| e.id == id)
        });
        assert_eq!(runner.running_count(), 0);
        let entry = runner.log().iter().find(|e| e.id == id).unwrap();
        assert!(matches!(entry.status, TaskStatus::Ok(ref s) if s == "world"));
    }

    #[test]
    fn submit_and_collect_err_result() {
        let mut runner = TaskRunner::new();
        let id = runner.submit("boom", || Err("bang".to_string()));
        wait_for(|| !runner.drain_completed().is_empty() || runner.running_count() == 0);
        let entry = runner.log().iter().find(|e| e.id == id).unwrap();
        assert!(matches!(entry.status, TaskStatus::Err(ref s) if s == "bang"));
    }

    #[test]
    fn panic_in_task_is_captured_as_err() {
        let mut runner = TaskRunner::new();
        let id = runner.submit("panicky", || -> Result<String, String> { panic!("nope") });
        wait_for(|| !runner.drain_completed().is_empty() || runner.running_count() == 0);
        let entry = runner.log().iter().find(|e| e.id == id).unwrap();
        assert!(matches!(entry.status, TaskStatus::Err(_)));
    }
}
