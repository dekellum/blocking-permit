// TODO: This is currently just a subset of potential usage from tokio_fs

use std::fs;
use std::io;
use std::path::Path;

use lazy_static::lazy_static;
use tokio_sync::semaphore::Semaphore;

use crate::{permit_or_dispatch, Canceled};

impl From<Canceled> for io::Error {
    fn from(me: Canceled) -> io::Error {
        io::Error::new(io::ErrorKind::Other, me)
    }
}

lazy_static! {
    pub static ref BLOCKING_SET: Semaphore = Semaphore::new(1);
}

/// Creates a new, empty directory at the provided path
///
/// This is an async version of [`std::fs::create_dir`][std]
///
/// [std]: https://doc.rust-lang.org/std/fs/fn.create_dir.html
async fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let path = path.as_ref().to_path_buf();
    permit_or_dispatch!(&BLOCKING_SET, move || fs::create_dir(&path))
}

#[cfg(test)]
mod tests {
    use futures::executor as futr_exec;
    use tempfile::tempdir;

    #[cfg(feature="tokio_pool")]
    use tokio_executor::threadpool as tokio_pool;

    use crate::DispatchPool;

    use super::*;

    fn register_dispatch_pool() {
        let pool = DispatchPool::builder()
            .pool_size(2)
            .create();
        DispatchPool::register_thread_local(pool);
    }

    fn deregister_dispatch_pool() {
        DispatchPool::deregister();
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
            create_dir(new_dir).await.unwrap();
        });

        assert!(new_dir_2.is_dir());
        deregister_dispatch_pool();
    }

    #[cfg(feature="tokio_pool")]
    #[test]
    fn dir_create_permit() {
        log_init();
        let rt = tokio_pool::Builder::new()
            .pool_size(1)
            .max_blocking(100)
            .build();

        let base_dir = tempdir().unwrap();
        let new_dir = base_dir.path().join("test");
        let new_dir_2 = new_dir.clone();

        rt.spawn(async move {
            create_dir(new_dir).await.unwrap();
        });

        rt.shutdown_on_idle().wait();
        assert!(new_dir_2.is_dir());
    }
}