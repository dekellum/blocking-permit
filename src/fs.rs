// TODO: This is currently just a subset of potential usage from tokio_fs

use std::fs;
use std::io;
use std::path::Path;

use lazy_static::lazy_static;

use crate::{dispatch_or_permit, Semaphore};

#[cfg(feature = "tokio-semaphore")]
lazy_static! {
    static ref BLOCKING_SET: Semaphore = Semaphore::new(1);
}

#[cfg(not(feature = "tokio-semaphore"))]
#[cfg(feature = "futures-intrusive")]
lazy_static! {
    static ref BLOCKING_SET: Semaphore = Semaphore::new(true, 1);
}

/// Creates a new, empty directory at the provided path
///
/// This is an async version of [`std::fs::create_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir.html
async fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let path = path.as_ref().to_path_buf();
    dispatch_or_permit!(&BLOCKING_SET, move || fs::create_dir(&path))
}

#[cfg(test)]
mod tests {
    use futures_executor as futr_exec;
    use tempfile::tempdir;

    use crate::{deregister_dispatch_pool, DispatchPool};

    use super::*;

    fn register_dispatch_pool() {
        let pool = DispatchPool::builder()
            .pool_size(2)
            .create();
        crate::register_dispatch_pool(pool);
    }

    fn log_init() {
        env_logger::builder().is_test(true).try_init().ok();
    }

    #[test]
    fn dir_create_dispatch() {
        log_init();
        register_dispatch_pool();
        let base_dir = tempdir().unwrap();
        let new_dir = base_dir.path().join("test");
        let new_dir_2 = new_dir.clone();

        futr_exec::block_on(async move {
            create_dir(new_dir)
                .await
                .unwrap();
        });

        assert!(new_dir_2.is_dir());
        deregister_dispatch_pool();
    }

    #[cfg(feature="tokio-threaded")]
    #[test]
    fn dir_create_permit() {
        log_init();

        let base_dir = tempdir().unwrap();
        let new_dir = base_dir.path().join("test");
        let new_dir_2 = new_dir.clone();

        {
            let mut rt = tokio::runtime::Builder::new()
                .core_threads(2)
                .max_threads(2)
                .threaded_scheduler()
                .build()
                .unwrap();

            let join = rt.spawn(async move {
                create_dir(new_dir)
                    .await
                    .unwrap();
            });
            rt.block_on(join).expect("join");
        }

        assert!(new_dir_2.is_dir());
    }
}
