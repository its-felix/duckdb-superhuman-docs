use std::future::Future;
use std::time::Duration;

#[cfg(not(target_os = "emscripten"))]
use std::sync::OnceLock;

#[cfg(not(target_os = "emscripten"))]
fn runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("failed to create async runtime: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(not(target_os = "emscripten"))]
pub(crate) fn block_on<F: Future>(future: F) -> Result<F::Output, String> {
    Ok(runtime()?.block_on(future))
}

#[cfg(not(target_os = "emscripten"))]
pub(crate) async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(target_os = "emscripten")]
extern "C" {
    fn emscripten_sleep(milliseconds: u32);
}

#[cfg(target_os = "emscripten")]
pub(crate) fn block_on<F: Future>(future: F) -> Result<F::Output, String> {
    use std::task::{Context, Poll, Waker};

    let mut future = std::pin::pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return Ok(output),
            Poll::Pending => unsafe { emscripten_sleep(1) },
        }
    }
}

#[cfg(target_os = "emscripten")]
pub(crate) async fn sleep(duration: Duration) {
    let milliseconds = duration.as_millis().min(u32::MAX as u128) as u32;
    unsafe { emscripten_sleep(milliseconds) };
}

pub(crate) fn block_on_result<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    block_on(future)?
}
