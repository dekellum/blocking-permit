use std::future::Future;
use std::panic::UnwindSafe;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use futures::executor as futr_exec;
use futures::future::FutureExt;
use futures::task::SpawnExt;

use lazy_static::lazy_static;
use log::{debug, info};

#[cfg(feature="tokio_pool")]
use tokio_executor::threadpool as tokio_pool;

use crate::*;

lazy_static! {
    static ref TEST_SET: Semaphore = Semaphore::new(true, 1);
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
    DispatchPool::register_thread_local(pool);
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
fn dispatch_panic_returns_canceled() {
    log_init();
    register_dispatch_pool();

    let task = dispatch_rx(|| {
        debug!("about to panic");
        panic!("you asked for it");
    });
    let res = futr_exec::block_on(task);
    match res {
        Err(e @ Canceled) => debug!("return from panic dispatch: {}", e),
        Ok(_) => panic!("should not succeed")
    }
    deregister_dispatch_pool();
}

fn spawn_good_and_bad_tasks() {
    let mut lp = futr_exec::LocalPool::new();
    let spawner = lp.spawner();
    spawner.spawn(
        dispatch_rx(|| {
            debug!("about to panic");
            panic!("you asked for it");
        }).map(|r| {
            if let Err(e) = r {
                debug!("panicky dispatch_rx err: {}", e)
            } else {
                panic!("should not succeed");
            }
        })
    ).expect("spawn 1");

    spawner.spawn(
        dispatch_rx(|| {
            debug!("anyone here to run me?");
        })
            .map(|r| {
                if let Err(e) = r {
                    panic!("normal dispatch_rx err: {}", e)
                }
            })
    ).expect("spawn 2");

    lp.run();
}

#[test]
fn survives_dispatch_panic_catch_unwind() {
    log_init();
    let pool = DispatchPool::builder()
        .pool_size(1)
        .catch_unwind(true)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    DispatchPool::register_thread_local(pool);

    spawn_good_and_bad_tasks();

    deregister_dispatch_pool();
}

#[test]
fn survives_dispatch_panic_ql0() {
    log_init();
    let pool = DispatchPool::builder()
        .pool_size(1)
        .queue_length(0)
        .catch_unwind(false)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    DispatchPool::register_thread_local(pool);

    spawn_good_and_bad_tasks();

    deregister_dispatch_pool();
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        // Safety: orginal self Pin means our self address is stable. We pin
        // project below only to poll ourselves or a `Delegate`, without any
        // other moves.
        let this = unsafe { self.get_unchecked_mut() };
        match this.delegate {
            Delegate::None => {
                match blocking_permit_future(&TEST_SET) {
                    Err(IsReactorThread) => {
                        let s = "dispatched S".to_owned();
                        this.delegate = Delegate::Dispatch(
                            dispatch_rx(move || -> usize {
                                info!("do some blocking stuff ({})", s);
                                thread::sleep(Duration::from_millis(100));
                                42
                            })
                        );
                    },
                    Ok(f) => {
                        this.delegate = Delegate::Permit(f);
                    }
                }
                let s = unsafe { Pin::new_unchecked(this) };
                s.poll(cx) // recurse once, with delegate in place
                           // (needed for correct waking)
            }
            Delegate::Dispatch(ref mut db) => {
                info!("delegate poll to DispatchBlocking");
                let db = unsafe { Pin::new_unchecked(db) };
                db.poll(cx)
            }
            Delegate::Permit(ref mut pf) => {
                info!("delegate poll to BlockingPermitFuture");
                let pf = unsafe { Pin::new_unchecked(pf) };
                match pf.poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Ok(p)) => {
                        p.enter();
                        info!("do some blocking stuff (permitted)");
                        Poll::Ready(Ok(41))
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e))
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
        match blocking_permit_future(&TEST_SET) {
            Err(IsReactorThread) => {
                let mut _i = 0;
                dispatch_rx(move || -> usize {
                    info!("do some blocking stuff (dispatched)");
                    _i = 1;
                    41
                })
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
fn async_block_with_macro_semaphore() {
    log_init();
    let task = async {
        permit_or_dispatch!(
            &TEST_SET,
            || -> Result::<usize, Canceled> {
                info!("do some blocking stuff, here or there");
                Ok(41)
            }
        )
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

    static FINISHED: AtomicUsize = AtomicUsize::new(0);

    let mut pool = futr_exec::LocalPool::new();
    let sp = pool.spawner();
    for _ in 0..1000 {
        sp.spawn(async {
            permit_or_dispatch!(&TEST_SET, || -> Result<usize, Canceled> {
                info!("do some blocking stuff, here or there");
                Ok(41)
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
            permit_run_or_dispatch!(&TEST_SET, || -> Result<usize, Canceled> {
                info!("do some blocking stuff, here or there");
                Ok(41)
            })
        }.map(|r| {
            assert_eq!(41, r.expect("permit_run_or_dispatch future"));
            FINISHED.fetch_add(1, Ordering::SeqCst);
            ()
        }));
    }
    rt.shutdown_on_idle().wait();
    assert_eq!(1000, FINISHED.load(Ordering::SeqCst));
}
