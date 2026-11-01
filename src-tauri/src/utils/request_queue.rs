use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
};

use tokio::sync::{Mutex, Notify};

/// Relative priority of a queued Warframe Market API call. `High` is for actions the user is
/// directly waiting on (Estimate Price, Quick Sell lookups); `Normal` is for the live scraper's
/// own background polling/posting. A `High` job is always dispatched before any queued `Normal`
/// job, even if the `Normal` job was submitted first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RequestPriority {
    Normal,
    High,
}

type Job = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

struct QueueInner {
    high: VecDeque<Job>,
    normal: VecDeque<Job>,
}

/// A single-worker FIFO-per-tier queue that every Warframe Market API call is funneled through.
/// This doesn't replace the underlying client's own rate limiting (still in effect per call) -
/// it just controls *ordering*, so a user waiting on one result doesn't sit behind a backlog of
/// the live scraper's background requests.
#[derive(Clone)]
pub struct RequestQueue {
    inner: Arc<Mutex<QueueInner>>,
    notify: Arc<Notify>,
}

impl RequestQueue {
    fn new() -> Self {
        let queue = Self {
            inner: Arc::new(Mutex::new(QueueInner {
                high: VecDeque::new(),
                normal: VecDeque::new(),
            })),
            notify: Arc::new(Notify::new()),
        };
        queue.spawn_worker();
        queue
    }

    fn spawn_worker(&self) {
        let inner = self.inner.clone();
        let notify = self.notify.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let job = {
                    let mut guard = inner.lock().await;
                    guard.high.pop_front().or_else(|| guard.normal.pop_front())
                };
                match job {
                    Some(job) => job().await,
                    None => notify.notified().await,
                }
            }
        });
    }

    /// Submits a job at the given priority and awaits its result. The job itself (the closure
    /// passed in) is what should perform the actual network call.
    pub async fn submit<F, Fut, T>(&self, priority: RequestPriority, task: F) -> T
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move || {
            Box::pin(async move {
                let result = task().await;
                // Ignore send errors: only happens if the caller stopped awaiting, e.g. dropped.
                let _ = tx.send(result);
            })
        });

        {
            let mut guard = self.inner.lock().await;
            match priority {
                RequestPriority::High => guard.high.push_back(job),
                RequestPriority::Normal => guard.normal.push_back(job),
            }
        }
        self.notify.notify_one();

        rx.await
            .expect("request queue worker task ended unexpectedly")
    }
}

static WFM_REQUEST_QUEUE: OnceLock<RequestQueue> = OnceLock::new();

/// The shared Warframe Market request queue. Lazily created on first use, backed by a single
/// background worker for the lifetime of the app.
pub fn wfm_request_queue() -> RequestQueue {
    WFM_REQUEST_QUEUE.get_or_init(RequestQueue::new).clone()
}
