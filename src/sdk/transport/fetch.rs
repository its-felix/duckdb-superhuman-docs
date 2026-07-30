use std::ffi::{c_char, c_void, CStr, CString};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use superhuman_docs_async::{Error, Request, Response};

type FetchCallback = unsafe extern "C" fn(
    userdata: *mut c_void,
    status: u16,
    body: *const u8,
    body_len: usize,
    error: *const c_char,
);

extern "C" {
    fn rust_ext_emscripten_fetch_start(
        method: *const c_char,
        url: *const c_char,
        headers: *const *const c_char,
        header_count: usize,
        body: *const u8,
        body_len: usize,
        body_present: bool,
        timeout_ms: u32,
        callback: FetchCallback,
        userdata: *mut c_void,
    ) -> usize;
    fn rust_ext_emscripten_fetch_cancel(handle: usize);
}

struct Shared {
    result: Mutex<Option<Result<Response, Error>>>,
    waker: Mutex<Option<Waker>>,
}

struct FetchFuture {
    shared: Arc<Shared>,
    handle: usize,
    completed: bool,
}

impl Future for FetchFuture {
    type Output = Result<Response, Error>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let result = { self.shared.result.lock().expect("fetch result lock").take() };
        if let Some(result) = result {
            self.completed = true;
            return Poll::Ready(result);
        }
        *self.shared.waker.lock().expect("fetch waker lock") = Some(context.waker().clone());
        let result = { self.shared.result.lock().expect("fetch result lock").take() };
        if let Some(result) = result {
            self.completed = true;
            Poll::Ready(result)
        } else {
            Poll::Pending
        }
    }
}

impl Drop for FetchFuture {
    fn drop(&mut self) {
        if !self.completed && self.handle != 0 {
            unsafe { rust_ext_emscripten_fetch_cancel(self.handle) };
        }
    }
}

unsafe extern "C" fn complete(
    userdata: *mut c_void,
    status: u16,
    body: *const u8,
    body_len: usize,
    error: *const c_char,
) {
    let shared = *Box::from_raw(userdata.cast::<Arc<Shared>>());
    let result = if status == 0 {
        let message = if error.is_null() {
            "Emscripten Fetch failed".to_string()
        } else {
            CStr::from_ptr(error).to_string_lossy().into_owned()
        };
        Err(Error::transport(message))
    } else {
        let bytes = if body.is_null() || body_len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(body, body_len).to_vec()
        };
        Ok(Response {
            status,
            body: bytes,
        })
    };
    *shared.result.lock().expect("fetch result lock") = Some(result);
    let waker = shared.waker.lock().expect("fetch waker lock").take();
    if let Some(waker) = waker {
        waker.wake();
    }
}

pub(super) async fn send(
    request: Request,
    credential: &str,
    timeout: Duration,
) -> Result<Response, Error> {
    let method = CString::new(request.method.as_str()).map_err(Error::transport)?;
    let url = CString::new(request.url).map_err(Error::transport)?;
    let authorization = CString::new(format!("Bearer {credential}")).map_err(Error::transport)?;
    let header_names = [
        CString::new("Authorization").expect("static header"),
        authorization,
        CString::new("Content-Type").expect("static header"),
        CString::new("application/json").expect("static header"),
    ];
    let body_present = request.body.is_some();
    let body = request.body.unwrap_or_default();
    let shared = Arc::new(Shared {
        result: Mutex::new(None),
        waker: Mutex::new(None),
    });
    let callback_shared = Box::into_raw(Box::new(Arc::clone(&shared))).cast();
    let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
    let handle = {
        let header_ptrs = header_names
            .iter()
            .map(|header| header.as_ptr())
            .collect::<Vec<_>>();
        unsafe {
            rust_ext_emscripten_fetch_start(
                method.as_ptr(),
                url.as_ptr(),
                header_ptrs.as_ptr(),
                header_ptrs.len(),
                body.as_ptr(),
                body.len(),
                body_present,
                timeout_ms,
                complete,
                callback_shared,
            )
        }
    };
    drop(header_names);
    drop(url);
    drop(method);
    if handle == 0 {
        unsafe { drop(Box::from_raw(callback_shared.cast::<Arc<Shared>>())) };
        return Err(Error::transport("failed to start Emscripten Fetch request"));
    }
    FetchFuture {
        shared,
        handle,
        completed: false,
    }
    .await
}
