#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll},
    thread,
    time::Duration
};

#[cfg(feature = "cleaver")]
use std::io;

use std::panic::UnwindSafe;

#[cfg(feature = "cleaver")]
use bytes::Bytes;

use futures_executor as futr_exec;
use futures_util::future::FutureExt;
use futures_util::task::SpawnExt;

#[cfg(any(feature = "cleaver", feature = "yield-stream"))]
use futures_util::{stream, stream::StreamExt};

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
use lazy_static::lazy_static;

#[cfg(feature="tokio-threaded")]
#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
use futures_util::stream::FuturesUnordered;

use tao_log::debug;

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
use tao_log::info;

use crate::*;

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
lazy_static! {
    static ref TEST_SET: Semaphore = Semaphore::default_new(1);
}

#[allow(dead_code)] fn is_send<T: Send>() -> bool { true }
#[allow(dead_code)] fn is_sync<T: Sync>() -> bool { true }

#[allow(dead_code)] fn is_unwind_safe<T: UnwindSafe>() -> bool { true }

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[test]
fn test_blocking_permit_traits() {
    assert!(is_send::<BlockingPermit<'_>>());

    // TODO: its not UnwindSafe because internals are not.  Does it need to be?
    // assert!(is_unwind_safe::<BlockingPermit<'_>>());

    assert!(is_send::<BlockingPermitFuture<'_>>());

    #[cfg(features = "tokio-semaphore")] {
        assert!(is_sync::<BlockingPermitFuture<'_>>());
    }

    assert!(is_send::<SyncBlockingPermitFuture<'_>>());
    assert!(is_sync::<SyncBlockingPermitFuture<'_>>());
}

fn log_init() {
    piccolog::test_logger();
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
fn register_dispatch_pool() {
    let pool = DispatchPool::builder()
        .pool_size(2)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    debug!("default pool: {:?}", pool);
    crate::register_dispatch_pool(pool);
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
fn maybe_register_dispatch_pool() {
    #[cfg(feature="current-thread")] {
        register_dispatch_pool();
    }
}

#[test]
fn dispatch_panic_returns_canceled() {
    log_init();
    let pool = DispatchPool::builder()
        .pool_size(1)
        .ignore_panics(true)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    debug!("pool: {:?}", pool);
    crate::register_dispatch_pool(pool);

    let task = dispatch_rx(|| {
        debug!("about to panic");
        panic!("you asked for it");
    }).unwrap();
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
        })
            .unwrap()
            .map(|r| {
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
            .unwrap()
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
        .ignore_panics(true)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    debug!("pool: {:?}", pool);
    crate::register_dispatch_pool(pool);

    spawn_good_and_bad_tasks();

    deregister_dispatch_pool();
}

#[test]
fn survives_dispatch_panic_catch_unwind_on_caller() {
    log_init();
    let pool = DispatchPool::builder()
        .queue_length(0)
        .ignore_panics(true)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    debug!("pool: {:?}", pool);
    crate::register_dispatch_pool(pool);

    spawn_good_and_bad_tasks();

    deregister_dispatch_pool();
}

#[cfg(feature="impossible_abort_test")]
#[test]
fn aborts_dispatch_panic() {
    log_init();
    let pool = DispatchPool::builder()
        .pool_size(1)
        .queue_length(0)
        .ignore_panics(false) // will abort
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    debug!("pool: {:?}", pool);
    crate::register_dispatch_pool(pool);

    spawn_good_and_bad_tasks();

    deregister_dispatch_pool();
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[test]
fn runs_oldest_on_caller() {
    log_init();
    let pool = DispatchPool::builder()
        .pool_size(1)
        .queue_length(1)
        .after_start(|i| debug!("starting DispatchPool thread {}", i))
        .before_stop(|i| debug!("dropping DispatchPool thread {}", i))
        .create();
    debug!("pool: {:?}", pool);
    crate::register_dispatch_pool(pool);

    let mut lp = futr_exec::LocalPool::new();
    let spawner = lp.spawner();
    // First goes through queue to occupy thread
    spawner.spawn(
        dispatch_rx(|| {
            assert!(
                thread::current().name()
                    .unwrap()
                    .contains(&"dpx-")
            );
            thread::sleep(Duration::from_millis(100));
        })
            .unwrap()
            .map(|_r| ())
    ).expect("spawn 1");

    thread::sleep(Duration::from_millis(10));

    // Second fills queue, momentarily
    spawner.spawn(
        dispatch_rx(|| {
            assert!(
                thread::current().name()
                    .unwrap()
                    .contains(&"runs_oldest_on_caller")
            );
            thread::sleep(Duration::from_millis(100));
        })
            .unwrap()
            .map(|_r| ())
    ).expect("spawn 2");

    // Third replaces second in queue, second run by this call.
    spawner.spawn(
        dispatch_rx(|| {
            assert!(
                thread::current().name()
                    .unwrap()
                    .contains(&"dpx-")
            );
        })
            .unwrap()
            .map(|_r| ())
    ).expect("spawn 3");

    lp.run();

    deregister_dispatch_pool();
}

/// Test of a manually-constructed future which uses `blocking_permit_future`
/// and `dispatch_rx`.
#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
struct TestFuture<'a> {
    delegate: Delegate<'a>,
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
enum Delegate<'a> {
    Dispatch(Dispatched<usize>),
    Permit(BlockingPermitFuture<'a>),
    None
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
impl<'a> TestFuture<'a> {
    fn new() -> Self {
        TestFuture { delegate: Delegate::None }
    }
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
impl<'a> Future for TestFuture<'a> {
    type Output = Result<usize, Canceled>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Self::Output>
    {
        // Safety: above self Pin means our self address is stable. We pin
        // project below only to poll ourselves or a `Delegate`, without any
        // other moves.
        let this = unsafe { self.get_unchecked_mut() };
        match this.delegate {
            Delegate::None => {
                let s = "dispatched S".to_owned();
                let drx = dispatch_rx(move || -> usize {
                    info!("do some blocking stuff ({})", s);
                    thread::sleep(Duration::from_millis(100));
                    42
                });
                match drx {
                    DispatchRx::Dispatch(disp) => {
                        this.delegate = Delegate::Dispatch(disp);
                    }
                    DispatchRx::NotRegistered(_) => {
                        let f = blocking_permit_future(&TEST_SET);
                        this.delegate = Delegate::Permit(f);
                    }
                }
                let s = unsafe { Pin::new_unchecked(this) };
                s.poll(cx) // recurse once, with delegate in place
                           // (needed for correct waking)
            }
            Delegate::Dispatch(ref mut disp) => {
                info!("delegate poll to Dispatched");
                let disp = unsafe { Pin::new_unchecked(disp) };
                disp.poll(cx)
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

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[test]
fn manual_future() {
    log_init();
    maybe_register_dispatch_pool();
    let val = futr_exec::block_on(TestFuture::new()).expect("success");
    assert!(val == 41 || val == 42);
    deregister_dispatch_pool();
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[test]
fn async_block_await_semaphore() {
    log_init();
    // Note how async/await makes this a lot nicer than the above
    // `TestFuture` manual way.
    let task = async {
        let mut _i = 0;
        let drx = dispatch_rx(move || -> usize {
            info!("do some blocking stuff (dispatched)");
            _i = 1;
            41
        });
        match drx {
            DispatchRx::Dispatch(disp) => {
                disp .await
            }
            DispatchRx::NotRegistered(_) => {
                let permit = blocking_permit_future(&TEST_SET) .await?;
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

#[cfg(feature = "futures-intrusive")]
#[test]
fn async_block_with_macro_semaphore() {
    log_init();
    register_dispatch_pool();
    let task = async {
        dispatch_or_permit!(
            &TEST_SET,
            || -> Result::<usize, Canceled> {
                info!("do some blocking stuff, here or there");
                Ok(41)
            }
        )
    };
    let val = futr_exec::block_on(task).expect("task success");
    assert_eq!(val, 41);
    deregister_dispatch_pool();
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[test]
fn test_futr_local_pool() {
    log_init();
    register_dispatch_pool();

    static FINISHED: AtomicUsize = AtomicUsize::new(0);

    let mut pool = futr_exec::LocalPool::new();
    let sp = pool.spawner();
    for _ in 0..1000 {
        sp.spawn(async {
            dispatch_or_permit!(&TEST_SET, || -> Result<usize, Canceled> {
                info!("do some blocking stuff, here or there");
                Ok(41)
            })
        }.map(|r| {
            assert_eq!(41, r.expect("dispatch_or_permit future"));
            FINISHED.fetch_add(1, Ordering::SeqCst);
            ()
        })).unwrap();
    }
    pool.run();
    assert_eq!(1000, FINISHED.load(Ordering::SeqCst));
    deregister_dispatch_pool();
}

#[cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#[cfg(feature="tokio-threaded")]
#[test]
fn test_tokio_threadpool() {
    use futures_util::stream::StreamExt;

    log_init();

    lazy_static! {
        static ref TEST_SET: Semaphore = Semaphore::default_new(3);
    }

    static FINISHED: AtomicUsize = AtomicUsize::new(0);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(3)
        .max_blocking_threads(1)
        .build()
        .unwrap();

    let futures: FuturesUnordered<_> = (0..1000).map(|_| {
        let j = async {
            dispatch_or_permit!(&TEST_SET, || -> Result<usize, Canceled> {
                info!("do some blocking stuff - {:?}",
                      thread::current().id());
                thread::sleep(Duration::from_millis(1));
                Ok(41)
            })
        }.map(|r| {
            assert_eq!(41, r.expect("dispatch_or_permit future"));
            FINISHED.fetch_add(1, Ordering::SeqCst);
            ()
        });
        rt.spawn(j) // -> JoinHandle (future)
    }).collect();
    let join = rt.spawn(async {
        let c = futures.collect::<Vec<_>>() .await;
        assert_eq!(1000, c.iter().filter(|r| r.is_ok()).count());
    });
    rt.block_on(join).unwrap();
    assert_eq!(1000, FINISHED.load(Ordering::SeqCst));
}

#[cfg(feature="cleaver")]
#[test]
fn test_cleaver_empty() {
    let task = async {
        let bstream = stream::empty();
        let cleaver = super::Cleaver::new(bstream, 1);
        cleaver.collect::<Vec<Result<Bytes,io::Error>>>() .await
    };
    let collected = futr_exec::block_on(task);
    assert_eq!(collected.len(), 0);
}

#[cfg(feature="cleaver")]
#[test]
fn test_cleaver_empty_bytes() {
    let task = async {
        let bstream = stream::once(async { Ok(Bytes::new()) });
        let cleaver = super::Cleaver::new(bstream, 1);
        cleaver.collect::<Vec<Result<Bytes,io::Error>>>() .await
    };
    let collected = futr_exec::block_on(task);
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].as_ref().unwrap().len(), 0);
}

#[cfg(feature="cleaver")]
#[test]
fn test_cleaver_through() {
    let task = async {
        let bstream = stream::once(async { Ok(Bytes::from("foobar")) });
        let cleaver = super::Cleaver::new(bstream, 9);
        cleaver.collect::<Vec<Result<Bytes,io::Error>>>() .await
    };
    let collected = futr_exec::block_on(task);
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].as_ref().unwrap().as_ref(), b"foobar");
}

#[cfg(feature="cleaver")]
#[test]
fn test_cleaver_more() {
    let task = async {
        let bstream = stream::once(async { Ok(Bytes::from("foobar")) });
        let cleaver = super::Cleaver::new(bstream, 4);
        cleaver.collect::<Vec<Result<Bytes,io::Error>>>() .await
    };
    let collected = futr_exec::block_on(task);
    assert_eq!(collected.len(), 2);
    assert_eq!(collected[0].as_ref().unwrap().as_ref(), b"foob");
    assert_eq!(collected[1].as_ref().unwrap().as_ref(), b"ar");
}

#[cfg(feature="yield-stream")]
#[test]
fn test_yield_stream_empty() {
    let task = async {
        let bstream = stream::empty::<usize>();
        let ystream = super::YieldStream::new(bstream);
        ystream.collect::<Vec<usize>>() .await
    };
    let collected = futr_exec::block_on(task);
    assert_eq!(collected.len(), 0);
}

#[cfg(feature="yield-stream")]
#[test]
fn test_yield_stream_multiple() {
    let task = async {
        let bstream = stream::iter(vec![1, 2, 3]);
        let ystream = super::YieldStream::new(bstream);
        ystream.collect::<Vec<usize>>() .await
    };
    let collected = futr_exec::block_on(task);
    assert_eq!(vec![1, 2, 3], collected);
}

#[cfg(feature="yield-stream")]
#[test]
fn test_yield_stream_stepping() {
    use std::pin::Pin;
    use futures_core::stream::Stream;
    use futures_core::task::{Context, Poll::*};
    use futures_util::task::noop_waker;

    let bstream = stream::iter(vec![1, 2, 3]);
    let mut ystream = super::YieldStream::new(bstream);
    let waker = noop_waker();
    let mut ctx = Context::from_waker(&waker);

    for i in 1..=3 {
        assert_eq!(Pin::new(&mut ystream).poll_next(&mut ctx), Ready(Some(i)));
        assert_eq!(Pin::new(&mut ystream).poll_next(&mut ctx), Pending);
    }
    assert_eq!(Pin::new(&mut ystream).poll_next(&mut ctx), Ready(None));
    assert_eq!(Pin::new(&mut ystream).poll_next(&mut ctx), Ready(None));
}
