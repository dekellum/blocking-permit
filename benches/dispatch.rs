#![cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#![warn(rust_2018_idioms)]

#![feature(test)]
extern crate test; // Still required, see rust-lang/rust#55133

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

#[cfg(feature="tangential")]
use futures_executor as futr_exec;

#[cfg(feature="tangential")]
use futures_util::task::SpawnExt;

use lazy_static::lazy_static;
use rand::seq::SliceRandom;
use test::Bencher;

#[cfg(feature="tokio-threaded")]
use futures_util::stream::{FuturesUnordered, StreamExt};

use blocking_permit::{
    dispatch_rx, DispatchPool,
    deregister_dispatch_pool, register_dispatch_pool,
    Semaphore,
    Semaphorish,
};

#[cfg(feature="tokio-threaded")]
use blocking_permit::blocking_permit_future;

const CORE_THREADS: usize      =   4;
const EXTRA_THREADS: usize     =   4;
const BATCH: usize             = 100;
const SLEEP_BATCH: usize       = 200;
const SLEEP_THREADS: usize     =  40;
const EXPECTED_RETURN: usize   = 100;

lazy_static! {
    static ref DEFAULT_SET: Semaphore = Semaphore::default_new(EXTRA_THREADS);
    static ref SLEEP_SET: Semaphore = Semaphore::default_new(SLEEP_THREADS);
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn noop_threaded_direct(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS+EXTRA_THREADS, None, None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let r = 41;
                assert_eq!(41, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn noop_threaded_dispatch_rx(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, None, Some(EXTRA_THREADS));

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let r = dispatch_rx(|| {
                    41
                }).unwrap() .await .unwrap();
                assert_eq!(41, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn noop_threaded_spawn_blocking(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, Some(EXTRA_THREADS), None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let r = tokio::task::spawn_blocking(|| {
                    41
                }) .await .unwrap();
                assert_eq!(41, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn noop_threaded_permit(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, Some(EXTRA_THREADS), None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let p = blocking_permit_future(&DEFAULT_SET)
                    .make_sync()
                    .await
                    .unwrap();
                let r = p.run(|| 41);
                assert_eq!(41, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[bench]
#[cfg(feature="tangential")]
fn noop_local_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(EXTRA_THREADS)
        .create();
    register_dispatch_pool(pool);
    b.iter(|| {
        let mut pool = futr_exec::LocalPool::new();
        let sp = pool.spawner();
        for _ in 0..BATCH {
            sp.spawn(async {
                let r = dispatch_rx(|| {
                    41
                }).unwrap() .await .unwrap();
                assert_eq!(41, r);
            }).unwrap();
        }
        pool.run();
    });
    deregister_dispatch_pool();
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn r_expensive_threaded_dispatch_rx(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, None, Some(EXTRA_THREADS));

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let r = dispatch_rx(|| {
                    // eprintln!("expensively on {}",
                    //           std::thread::current().name().unwrap());
                    expensive_comp()
                }).unwrap() .await .unwrap();
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn r_expensive_threaded_spawn_blocking(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, Some(EXTRA_THREADS), None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let r = tokio::task::spawn_blocking(|| {
                    expensive_comp()
                }) .await .unwrap();
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn r_expensive_threaded_permit(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, Some(EXTRA_THREADS), None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let p = blocking_permit_future(&DEFAULT_SET)
                    .make_sync()
                    .await
                    .unwrap();
                let r = p.run(|| expensive_comp());
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn r_expensive_threaded_direct(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS+EXTRA_THREADS, None, None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..BATCH).map(|_| {
            rt.spawn(async {
                let r = expensive_comp();
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[bench]
#[cfg(feature="tangential")]
fn r_expensive_local_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(EXTRA_THREADS)
        .create();
    register_dispatch_pool(pool);
    b.iter(|| {
        let mut pool = futr_exec::LocalPool::new();
        let sp = pool.spawner();
        for _ in 0..BATCH {
            sp.spawn(async {
                let r = dispatch_rx(|| {
                    expensive_comp()
                }).unwrap() .await .unwrap();
                assert_eq!(EXPECTED_RETURN, r);
            }).unwrap();
        }
        pool.run();
    });
    deregister_dispatch_pool();
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn sleep_threaded_direct(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS+SLEEP_THREADS, None, None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..SLEEP_BATCH).map(|_| {
            rt.spawn(async {
                let r = random_sleep();
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(SLEEP_BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn sleep_threaded_dispatch_rx(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, None, Some(SLEEP_THREADS));

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..SLEEP_BATCH).map(|_| {
            rt.spawn(async {
                let r = dispatch_rx(|| {
                    random_sleep()
                }).unwrap() .await .unwrap();
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(SLEEP_BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn sleep_threaded_spawn_blocking(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, Some(SLEEP_THREADS), None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..SLEEP_BATCH).map(|_| {
            rt.spawn(async {
                let r = tokio::task::spawn_blocking(|| {
                    random_sleep()
                }) .await .unwrap();
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(SLEEP_BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn sleep_threaded_permit(b: &mut Bencher) {
    let rt = rt_multi_thread(CORE_THREADS, Some(SLEEP_THREADS), None);

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..SLEEP_BATCH).map(|_| {
            rt.spawn(async {
                let p = blocking_permit_future(&SLEEP_SET)
                    .make_sync()
                    .await
                    .unwrap();
                let r = p.run(|| random_sleep());
                assert_eq!(EXPECTED_RETURN, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(SLEEP_BATCH, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[bench]
#[cfg(feature="tangential")]
fn sleep_local_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(SLEEP_THREADS)
        .create();
    register_dispatch_pool(pool);
    b.iter(|| {
        let mut pool = futr_exec::LocalPool::new();
        let sp = pool.spawner();
        for _ in 0..SLEEP_BATCH {
            sp.spawn(async {
                let r = dispatch_rx(|| {
                    random_sleep()
                }).unwrap() .await .unwrap();
                assert_eq!(EXPECTED_RETURN, r);
            }).unwrap();
        }
        pool.run();
    });
    deregister_dispatch_pool();
}

#[cfg(feature="tokio-threaded")]
fn rt_multi_thread(
    core: usize,
    blocking: Option<usize>,
    dispatch: Option<usize>)
    -> tokio::runtime::Runtime
{
    struct AbortOnPanic;

    impl Drop for AbortOnPanic {
        fn drop(&mut self) {
            std::process::abort();
        }
    }

    let mut bldr = tokio::runtime::Builder::new_multi_thread();
    bldr.worker_threads(core);

    let extra_threads = match blocking {
        Some(c) => c,
        None => 1
    };
    bldr.max_blocking_threads(extra_threads);

    if let Some(c) = dispatch {
        let pool = DispatchPool::builder().pool_size(c).create();
        bldr.on_thread_start(move || {
            register_dispatch_pool(pool.clone());
        });
        bldr.on_thread_stop(|| {
            deregister_dispatch_pool();
        });
    }
    let cntr = AtomicUsize::new(0);
    let mut max = core;
    if let Some(b) = blocking {
        max += b;
    }
    bldr.thread_name_fn(move || {
        let c = cntr.fetch_add(1, Ordering::SeqCst);
        if c >= max {
            let _aborter = AbortOnPanic;
            panic!("spawn_blocking/block_in_place must have been used!");
        } else {
            format!("worker-{}", c)
        }
    });
    bldr.build().unwrap()
}

fn expensive_comp() -> usize {
    let mut vals: Vec<usize> = (500..800).map(|v| (v % 101)).collect();
    vals.shuffle(&mut rand::thread_rng());
    vals.sort();
    vals[vals.len() - 1]
}

fn random_sleep() -> usize {
    const DELAYS: [u64; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 12, 49];
    thread::sleep(Duration::from_micros(
        *DELAYS.choose(&mut rand::thread_rng()).unwrap()
    ));
    EXPECTED_RETURN
}
