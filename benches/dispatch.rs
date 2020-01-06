#![cfg(any(feature = "tokio-semaphore", feature = "futures-intrusive"))]
#![warn(rust_2018_idioms)]

#![feature(test)]
extern crate test; // Still required, see rust-lang/rust#55133

use std::thread;
use std::time::Duration;

use futures_executor as futr_exec;
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

lazy_static! {
    static ref DEFAULT_SET: Semaphore = Semaphore::default_new(4);
    static ref SLEEP_SET: Semaphore = Semaphore::default_new(40);
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn noop_threaded_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(4)
        .create();

    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4)
        .max_threads(4)
        .threaded_scheduler()
        .on_thread_start(move || {
            register_dispatch_pool(pool.clone());
        })
        .on_thread_stop(|| {
            deregister_dispatch_pool();
        })
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let r = dispatch_rx(|| {
                    41
                }).unwrap() .await .unwrap();
                assert_eq!(41, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn noop_threaded_spawn_blocking(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4)
        .max_threads(4+4)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let r = tokio::task::spawn_blocking(|| {
                    41
                }) .await .unwrap();
                assert_eq!(41, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn noop_threaded_permit(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4+4)
        .max_threads(4+4)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
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
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[bench]
fn noop_local_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(4)
        .create();
    register_dispatch_pool(pool);
    b.iter(|| {
        let mut pool = futr_exec::LocalPool::new();
        let sp = pool.spawner();
        for _ in 0..100 {
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
    let pool = DispatchPool::builder()
        .pool_size(4)
        .create();

    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4)
        .max_threads(4)
        .threaded_scheduler()
        .on_thread_start(move || {
            register_dispatch_pool(pool.clone());
        })
        .on_thread_stop(|| {
            deregister_dispatch_pool();
        })
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let r = dispatch_rx(|| {
                    // eprintln!("expensively on {}",
                    //           std::thread::current().name().unwrap());
                    expensive_comp()
                }).unwrap() .await .unwrap();
                assert_eq!(100, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn r_expensive_threaded_spawn_blocking(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4)
        .max_threads(4+4)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let r = tokio::task::spawn_blocking(|| {
                    expensive_comp()
                }) .await .unwrap();
                assert_eq!(100, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn r_expensive_threaded_permit(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4+4)
        .max_threads(4+4)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let p = blocking_permit_future(&DEFAULT_SET)
                    .make_sync()
                    .await
                    .unwrap();
                let r = p.run(|| expensive_comp());
                assert_eq!(100, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[bench]
fn r_expensive_local_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(4)
        .create();
    register_dispatch_pool(pool);
    b.iter(|| {
        let mut pool = futr_exec::LocalPool::new();
        let sp = pool.spawner();
        for _ in 0..100 {
            sp.spawn(async {
                let r = dispatch_rx(|| {
                    expensive_comp()
                }).unwrap() .await .unwrap();
                assert_eq!(100, r);
            }).unwrap();
        }
        pool.run();
    });
    deregister_dispatch_pool();
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn sleep_threaded_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(40)
        .create();

    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4)
        .max_threads(4)
        .threaded_scheduler()
        .on_thread_start(move || {
            register_dispatch_pool(pool.clone());
        })
        .on_thread_stop(|| {
            deregister_dispatch_pool();
        })
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let r = dispatch_rx(|| {
                    random_sleep()
                }).unwrap() .await .unwrap();
                assert_eq!(100, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn sleep_threaded_spawn_blocking(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4)
        .max_threads(4+40)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let r = tokio::task::spawn_blocking(|| {
                    random_sleep()
                }) .await .unwrap();
                assert_eq!(100, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[cfg(feature="tokio-threaded")]
#[bench]
fn sleep_threaded_permit(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .core_threads(4+40)
        .max_threads(4+40)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: FuturesUnordered<_> = (0..100).map(|_| {
            rt.spawn(async {
                let p = blocking_permit_future(&SLEEP_SET)
                    .make_sync()
                    .await
                    .unwrap();
                let r = p.run(|| random_sleep());
                assert_eq!(100, r);
            })
        }).collect();
        let join = rt.spawn(async {
            let c = futures.collect::<Vec<_>>() .await;
            assert_eq!(100, c.iter().filter(|r| r.is_ok()).count());
        });
        rt.block_on(join).unwrap();
    });
}

#[bench]
fn sleep_local_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(40)
        .create();
    register_dispatch_pool(pool);
    b.iter(|| {
        let mut pool = futr_exec::LocalPool::new();
        let sp = pool.spawner();
        for _ in 0..100 {
            sp.spawn(async {
                let r = dispatch_rx(|| {
                    random_sleep()
                }).unwrap() .await .unwrap();
                assert_eq!(100, r);
            }).unwrap();
        }
        pool.run();
    });
    deregister_dispatch_pool();
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
    100
}
