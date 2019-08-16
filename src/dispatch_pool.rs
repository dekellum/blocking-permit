use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use crossbeam_channel as cbch;
use num_cpus;

/// A specialized thread pool for dispatch_blocking tasks.
///
/// This simple pool is _not_ an executor and doesn't need any "waking" or
/// "parking" facilities. It uses an unbounded MPMC channel with the assumption
/// that resource/capacity is externally constrained. Once constructed, threads
/// are spawned and the instance acts as a handle to the pool. This may be
/// inexpensively cloned for additional handles to the same pool.
#[derive(Clone)]
pub struct DispatchPool(Arc<Sender>);

struct Sender {
    tx: cbch::Sender<Work>,
    pool_size: usize,
}

type AroundFn = Arc<dyn Fn(usize) + Send + Sync>;

/// A builder for [`DispatchPool`] supporting various configuration.
pub struct DispatchPoolBuilder {
    pool_size: Option<usize>,
    stack_size: Option<usize>,
    name_prefix: Option<String>,
    after_start: Option<AroundFn>,
    before_stop: Option<AroundFn>,
}

enum Work {
    Unit(Box<dyn FnOnce() + Send>),
    Terminate,
}

thread_local!(static POOL: RefCell<Option<DispatchPool>> = RefCell::new(None));

impl DispatchPool {
    /// Create new pool using defaults.
    pub fn new() -> DispatchPool {
        DispatchPoolBuilder::default().create()
    }

    /// Create a new builder for configuring a new pool.
    pub fn builder() -> DispatchPoolBuilder {
        DispatchPoolBuilder::new()
    }

    /// Enqueue a new blocking operation closure, returning immediately.
    pub fn spawn(&self, f: Box<dyn FnOnce() + Send>) {
        self.0.tx.try_send(Work::Unit(f)).expect("transmit success");
        // TODO: Maybe log and otherwise ignore any errors?
    }

    /// Register a clone of self as a thread local pool instance. Any prior
    /// instance is returned.
    pub fn register_thread_local(&self) -> Option<DispatchPool> {
        POOL.with(|p| p.replace(Some(self.clone())))
    }

    /// Deregister and return any thread local pool instance.
    pub fn deregister() -> Option<DispatchPool> {
        POOL.with(|p| p.replace(None))
    }

    /// Return true if a DispatchPool is registered to the current thread.
    pub fn is_thread_registered() -> bool {
        POOL.with(|p| p.borrow().is_some())
    }

    /// Enqueue a new blocking operation closure on the registered thread local
    /// pool, returning immediately.
    ///
    /// ## Panics
    ///
    /// Panics if no thread pool is registered, e.g. if
    /// [`DispatchPool::is_thread_registered`] would return false.
    pub fn spawn_local(f: Box<dyn FnOnce() + Send>) {
        POOL.with(|p| {
            p.borrow().as_ref()
                .expect("no thread local BlockingPool was registered")
                .spawn(f)
        });
    }
}

fn work(
    index: usize,
    after_start: Option<AroundFn>,
    before_stop: Option<AroundFn>,
    rx: cbch::Receiver<Work>)
{
    if let Some(ref asfn) = after_start {
        asfn(index);
    }
    drop(after_start);

    loop {
        match rx.recv() {
            Ok(Work::Unit(bfn)) => bfn(),
            Ok(Work::Terminate) | Err(_) => break,
        }
    }

    if let Some(bsfn) = before_stop {
        bsfn(index);
    }
}

impl Default for DispatchPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        for _ in 0..self.pool_size {
            if self.tx.try_send(Work::Terminate).is_err() {
                break;
            }
        }
    }
}

impl fmt::Debug for DispatchPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchPool")
            .field("threads", &self.0.pool_size)
            .finish()
    }
}

impl DispatchPoolBuilder {
    /// Create new dispatch pool builder, for configuration.
    pub fn new() -> DispatchPoolBuilder {
        DispatchPoolBuilder {
            pool_size: None,
            stack_size: None,
            name_prefix: None,
            after_start: None,
            before_stop: None,
        }
    }

    /// Set the fixed number of threads in the pool.
    ///
    /// This must at least be one (1) thread, asserted.
    ///
    /// Default: the number of logical CPU's, minus one, if more than
    /// one.
    pub fn pool_size(&mut self, size: usize) -> &mut Self {
        assert!(size > 0);
        self.pool_size = Some(size);
        self
    }

    /// Set the stack size in bytes for each thread in the pool.
    ///
    /// Default: the default thread stack size.
    pub fn stack_size(&mut self, stack_size: usize) -> &mut Self {
        self.stack_size = Some(stack_size);
        self
    }

    /// Set name prefix for threads in the pool.
    ///
    /// Default: "dpx-pool-N-" where N is a 1-based static pool counter
    pub fn name_prefix<S: Into<String>>(&mut self, name_prefix: S) -> &mut Self {
        self.name_prefix = Some(name_prefix.into());
        self
    }

    /// Set a closure to be called immediately after each thread is started.
    ///
    /// The closure is passed an 0-based index of the thread.
    ///
    /// Default: None
    pub fn after_start<F>(&mut self, f: F) -> &mut Self
        where F: Fn(usize) + Send + Sync + 'static
    {
        self.after_start = Some(Arc::new(f));
        self
    }

    /// Set a closure to be called immediately before a pool thread exits.
    ///
    /// The closure is passed a 0-based index of the thread.
    ///
    /// Default: None
    pub fn before_stop<F>(&mut self, f: F) -> &mut Self
        where F: Fn(usize) + Send + Sync + 'static
    {
        self.before_stop = Some(Arc::new(f));
        self
    }

    /// Create a new [`DispatchPool`](DispatchPool) with the provided
    /// configuration.
    pub fn create(&mut self) -> DispatchPool {
        let (tx, rx) = cbch::unbounded();

        let pool_size = if let Some(size) = self.pool_size {
            size
        } else {
            let mut size = num_cpus::get();
            if size > 1 {
                size -= 1;
            }
            size
        };

        static POOL_CNT: AtomicUsize = AtomicUsize::new(1);
        let name_prefix = if let Some(ref prefix) = self.name_prefix {
            prefix.to_owned()
        } else {
            format!(
                "dpx-pool-{}-",
                POOL_CNT.fetch_add(1, Ordering::SeqCst))
        };

        let pool = DispatchPool(Arc::new(Sender { tx, pool_size }));

        for i in 0..pool_size {
            let after_start = self.after_start.clone();
            let before_stop = self.before_stop.clone();
            let rx = rx.clone();

            let mut builder = thread::Builder::new();
            builder = builder.name(format!("{}{}", name_prefix, i));
            if let Some(size) = self.stack_size {
                builder = builder.stack_size(size);
            }
            builder
                .spawn(move || work(i, after_start, before_stop, rx))
                .expect("thread spawned");
        }

        pool
    }
}

impl Default for DispatchPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}
