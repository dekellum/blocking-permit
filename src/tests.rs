use std::future::Future;
use std::panic::UnwindSafe;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use futures::executor as futr_exec;
use futures::future::{FutureExt, TryFutureExt};
use futures::task::SpawnExt;

use lazy_static::lazy_static;
use log::{debug, info};

use tokio_sync::semaphore::Semaphore;

#[cfg(feature="tokio_pool")]
use tokio_executor::threadpool as tokio_pool;

use crate::*;

// TODO: Pretend for now that this is part of tokio-fs and also somehow
// configurable.
pub mod tokio_fs {
    use lazy_static::lazy_static;
    use tokio_sync::semaphore::Semaphore;

    lazy_static! {
        pub static ref BLOCKING_SET: Semaphore = Semaphore::new(1);
    }
}

fn is_send<T: Send>() -> bool { true }

#[allow(dead_code)]
fn is_unwind_safe<T: UnwindSafe>() -> bool { true }

#[test]
fn test_blocking_permit_traits() {
    assert!(is_send::<BlockingPermit<'_>>());

    // TODO: its not UnwindSafe because `semaphore::Permit` is not.  Does
    // it need to be?
    // assert!(is_unwind_safe::<BlockingPermit<'_>>());
}

fn log_init() {
    env_logger::builder().is_test(true).try_init().ok();
}

fn register_dispatch_pool() {
    let pool = DispatchPool::builder()
        .pool_size(2)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    DispatchPool::register_thread_local(&pool);
}

fn deregister_dispatch_pool() {
    DispatchPool::deregister();
}

fn maybe_register_dispatch_pool() {
    #[cfg(feature="current_thread")] {
        register_dispatch_pool();
    }
}

#[test]
fn unlimited_current_thread() {
    log_init();
    register_dispatch_pool();
    match blocking_permit() {
        Ok(_) => panic!("should have errored"),
        Err(IsReactorThread) => {}
    }
    deregister_dispatch_pool();
}

#[test]
fn unlimited_not_current_thread() {
    log_init();
    match blocking_permit() {
        Ok(p) => {
            p.enter();
            info!("do some blocking stuff");
        },
        Err(e) => panic!("errored: {}", e)
    }
}

/// Test of a manually-constructed future which uses
/// `blocking_permit_future` and `dispatch_blocking`.
struct TestFuture<'a> {
    delegate: Delegate<'a>,
}

enum Delegate<'a> {
    Dispatch(DispatchBlocking<usize>),
    Permit(BlockingPermitFuture<'a>),
    None
}

impl<'a> TestFuture<'a> {
    fn new() -> Self {
        TestFuture { delegate: Delegate::None }
    }
}

impl<'a> Future for TestFuture<'a> {
    type Output = Result<usize, Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        match self.delegate {
            Delegate::None => {
                match blocking_permit_future(&tokio_fs::BLOCKING_SET) {
                    Err(IsReactorThread) => {
                        let s = "dispatched S".to_owned();
                        self.delegate = Delegate::Dispatch(
                            dispatch_rx(move || -> usize {
                                info!("do some blocking stuff ({})", s);
                                thread::sleep(Duration::from_millis(100));
                                42
                            })
                        );
                    },
                    Ok(f) => {
                        self.delegate = Delegate::Permit(f);
                    }
                }
                self.poll(cx) // recurse once, with delegate in place
                              // (needed for correct waking)
            }
            Delegate::Dispatch(ref mut db) => {
                info!("delegate poll to DispatchBlocking");
                Pin::new(&mut *db).poll(cx).map_err(|_| Canceled)
            }
            Delegate::Permit(ref mut pf) => {
                info!("delegate poll to BlockingPermitFuture");
                match Pin::new(&mut *pf).poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Ok(p)) => {
                        p.enter();
                        info!("do some blocking stuff (permitted)");
                        Poll::Ready(Ok(41))
                    }
                    Poll::Ready(Err(_)) => Poll::Ready(Err(Canceled))
                }
            }
        }
    }
}

#[test]
fn manual_future() {
    log_init();
    maybe_register_dispatch_pool();
    let val = futr_exec::block_on(TestFuture::new()).expect("success");
    assert!(val == 41 || val == 42);
    deregister_dispatch_pool();
}

#[test]
fn async_block_await_semaphore() {
    log_init();
    // Note how async/await makes this a lot nicer than the above
    // `TestFuture` manual way.
    let task = async {
        match blocking_permit_future(&tokio_fs::BLOCKING_SET) {
            Err(IsReactorThread) => {
                let mut _i = 0;
                dispatch_rx(move || -> usize {
                    info!("do some blocking stuff (dispatched)");
                    _i = 1;
                    41
                })
                    .map_err(|_| Canceled)
                    .await
            }
            Ok(f) => {
                let permit = f .await?;
                permit.enter();
                info!("do some blocking stuff (permitted)");
                Ok(42)
            }
        }
    };
    maybe_register_dispatch_pool();
    let val = futr_exec::block_on(task).expect("task success");
    assert!(val == 41 || val == 42);
    deregister_dispatch_pool();
}

#[test]
fn async_block_await_unlimited() {
    log_init();
    let task = async {
        match blocking_permit() {
            Ok(permit) => {
                permit.enter();
                info!("do some blocking stuff (permitted)");
                Ok(42)
            }
            Err(IsReactorThread) => {
                dispatch_rx(|| -> usize {
                    info!("do some blocking stuff (dispatched)");
                    41
                })
                    .map_err(|_| Canceled)
                    .await
            }
        }
    };
    maybe_register_dispatch_pool();
    let val = futr_exec::block_on(task).expect("task success");
    assert!(val == 41 || val == 42);
    deregister_dispatch_pool();
}

#[test]
fn async_block_with_macro_semaphore() {
    log_init();
    let task = async {
        permit_or_dispatch!(&tokio_fs::BLOCKING_SET, || {
            info!("do some blocking stuff, here or there");
            41
        })
    };
    maybe_register_dispatch_pool();
    let val = futr_exec::block_on(task).expect("task success");
    assert_eq!(val, 41);
    deregister_dispatch_pool();
}

#[test]
fn async_block_with_macro_unlimited() {
    log_init();
    let task = async {
        permit_or_dispatch!(|| {
            info!("do some blocking stuff, here or there");
            41
        })
    };
    maybe_register_dispatch_pool();
    let val = futr_exec::block_on(task).expect("task success");
    assert_eq!(val, 41);
    deregister_dispatch_pool();
}

#[test]
fn test_futr_local_pool() {
    log_init();
    register_dispatch_pool();
    lazy_static! {
        static ref TEST_SET: Semaphore = Semaphore::new(1);
    }
    static FINISHED: AtomicUsize = AtomicUsize::new(0);

    let mut pool = futr_exec::LocalPool::new();
    let mut sp = pool.spawner();
    for _ in 0..1000 {
        sp.spawn(async {
            permit_or_dispatch!(&TEST_SET, || {
                info!("do some blocking stuff, here or there");
                41
            })
        }.map(|r| {
            assert_eq!(41, r.expect("permit_or_dispatch future"));
            FINISHED.fetch_add(1, Ordering::SeqCst);
            ()
        })).unwrap();
    }
    pool.run();
    assert_eq!(1000, FINISHED.load(Ordering::SeqCst));
    deregister_dispatch_pool();
}

#[cfg(feature="tokio_pool")]
#[test]
fn test_tokio_threadpool() {
    log_init();
    lazy_static! {
        static ref TEST_SET: Semaphore = Semaphore::new(3);
    }
    static FINISHED: AtomicUsize = AtomicUsize::new(0);

    let rt = tokio_pool::Builder::new()
        .pool_size(7)
        .max_blocking(32768-7)
        .build();

    for _ in 0..1000 {
        rt.spawn(async {
            permit_run_or_dispatch!(&TEST_SET, || {
                info!("do some blocking stuff, here or there");
                41
            })
        }.map(|r| {
            assert_eq!(41, r.expect("permit_or_dispatch future"));
            FINISHED.fetch_add(1, Ordering::SeqCst);
            ()
        }));
    }
    rt.shutdown_on_idle().wait();
    assert_eq!(1000, FINISHED.load(Ordering::SeqCst));
}
