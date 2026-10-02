//! The shared tokio runtime that owns every socket and HTTP request.
//!
//! GPUI runs its own executor, so networking lives on a small dedicated tokio runtime. Work is
//! handed over with [`spawn`], whose returned future can be awaited from any executor.

use std::{future::Future, sync::OnceLock};

/// Returns the process-wide networking runtime, creating it on first use.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("t3-net")
            .enable_all()
            .build()
            .expect("failed to start the networking runtime")
    })
}

/// Runs `future` on the networking runtime and returns a handle future that resolves with its
/// output. Panics inside the task are propagated to the awaiting side.
pub fn spawn<F>(future: F) -> impl Future<Output = F::Output> + Send + 'static
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let handle = runtime().spawn(future);
    async move {
        match handle.await {
            Ok(output) => output,
            Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Err(error) => panic!("networking task was cancelled: {error}"),
        }
    }
}
