#![warn(rust_2018_idioms)]

#![feature(test)]
extern crate test; // Still required, see rust-lang/rust#55133

use lazy_static::lazy_static;
use test::Bencher;
use futures::executor as futr_exec;
use futures::task::SpawnExt;
use rand::seq::SliceRandom;

use blocking_permit::{
    blocking_permit_future,
    dispatch_rx, DispatchPool,
    deregister_dispatch_pool, register_dispatch_pool,
    Semaphore
};

lazy_static! {
    static ref TEST_SET: Semaphore = Semaphore::new(true, 20);
}

#[cfg(feature="tokio_threaded")]
#[bench]
fn noop_threaded_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(4)
        .create();

    let mut rt = tokio::runtime::Builder::new()
        .num_threads(5)
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
        let futures: Vec<_> = (0..100).map(|_| {
            let job = async {
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                p.enter();
                let r = dispatch_rx(|| {
                    41
                }).unwrap() .await .unwrap();
                assert_eq!(41, r);
                drop(p);
            };
            rt.spawn(job)
        }).collect();
        rt.block_on(futures::future::join_all(futures));
    });
}

#[cfg(feature="tokio_threaded")]
#[bench]
fn noop_threaded_spawn_blocking(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .num_threads(5)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: Vec<_> = (0..100).map(|_| {
            let job = async {
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                p.enter();
                let r = tokio::task::spawn_blocking(|| {
                    41
                }) .await .unwrap();
                assert_eq!(41, r);
                drop(p);
            };
            rt.spawn(job)
        }).collect();
        rt.block_on(futures::future::join_all(futures));
    });
}

#[cfg(feature="tokio_threaded")]
#[bench]
fn noop_threaded_in_place(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .num_threads(4)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: Vec<_> = (0..100).map(|_| {
            let job = async {
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                let r = p.run(|| 41);
                assert_eq!(41, r);
            };
            rt.spawn(job)
        }).collect();
        rt.block_on(futures::future::join_all(futures));
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
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                p.enter();
                let r = dispatch_rx(|| {
                    41
                }).unwrap() .await .unwrap();
                assert_eq!(41, r);
                drop(p);
            }).unwrap();
        }
        pool.run();
    });
    deregister_dispatch_pool();
}

#[cfg(feature="tokio_threaded")]
#[bench]
fn r_expensive_threaded_dispatch_rx(b: &mut Bencher) {
    let pool = DispatchPool::builder()
        .pool_size(4)
        .create();

    let mut rt = tokio::runtime::Builder::new()
        .num_threads(5)
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
        let futures: Vec<_> = (0..100).map(|_| {
            let job = async {
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                p.enter();
                let r = dispatch_rx(|| {
                    expensive_comp()
                }).unwrap() .await .unwrap();
                assert_eq!(100, r);
            };
            rt.spawn(job)
        }).collect();
        rt.block_on(futures::future::join_all(futures));
    });
}

#[cfg(feature="tokio_threaded")]
#[bench]
fn r_expensive_threaded_spawn_blocking(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .num_threads(5)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: Vec<_> = (0..100).map(|_| {
            let job = async {
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                p.enter();
                let r = tokio::task::spawn_blocking(|| {
                    expensive_comp()
                }) .await .unwrap();
                assert_eq!(100, r);
            };
            rt.spawn(job)
        }).collect();
        rt.block_on(futures::future::join_all(futures));
    });
}

#[cfg(feature="tokio_threaded")]
#[bench]
fn r_expensive_threaded_in_place(b: &mut Bencher) {
    let mut rt = tokio::runtime::Builder::new()
        .num_threads(4)
        .threaded_scheduler()
        .build()
        .unwrap();

    b.iter(|| {
        let futures: Vec<_> = (0..100).map(|_| {
            let job = async {
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                let r = p.run(|| expensive_comp());
                assert_eq!(100, r);
            };
            rt.spawn(job)
        }).collect();
        rt.block_on(futures::future::join_all(futures));
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
                let p = blocking_permit_future(&TEST_SET)
                    .await
                    .unwrap();
                p.enter();
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

fn expensive_comp() -> usize {
    let mut vals: Vec<usize> = (500..600).map(|v| (v % 101)).collect();
    vals.shuffle(&mut rand::thread_rng());
    vals.sort();
    vals[vals.len() - 1]
}
