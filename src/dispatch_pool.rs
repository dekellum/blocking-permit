use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::panic::{catch_unwind, AssertUnwindSafe};

use tao_log::{error, trace};
use parking_lot::{Condvar, Mutex};

/// A specialized thread pool and queue for dispatching _blocking_
/// (synchronous, long running) operations.
///
/// This pool is not an _executor_, has no _waking_ facilities, etc. As
/// compared with other thread pools supporting `spawn`, or `spawn_blocking`
/// in _tokio_, here also called [`dispatch()`](crate::dispatch()) or
/// [`dispatch_rx()`](crate::dispatch_rx()) this pool has some unique features:
///
/// * A configurable, fixed number of threads created before return from
///   construction and terminated on `Drop::drop`.  Consistent memory
///   footprint. No warmup required. No per-task thread management overhead.
///
/// * Configurable panic handling policy: Either catches and logs dispatch
///   panics, or aborts the process, on panic unwind.
///
/// * Supports fixed (bounded) or unbounded queue length.
///
/// * When the queue is bounded and becomes full, [`DispatchPool::spawn`] pops
///   the oldest operation off the queue before pushing the newest passed
///   operation, to ensure space while holding a lock. Then as a fallback it
///   runs the old operation. Thus we enlist calling threads once the queue
///   reaches limit, but operation order (at least from perspective of a single
///   thread) is preserved.
///
/// ## Usage
///
/// By default, the pool uses an unbounded queue, with the assumption that
/// resource/capacity is externally constrained. Once constructed, a fixed
/// number of threads are spawned and the instance acts as a handle to the
/// pool. This may be inexpensively cloned for additional handles to the same
/// pool.
///
/// See [`DispatchPoolBuilder`] for an extensive set of options.
///
/// ### With tokio's threaded runtime
///
/// One can schedule a clone of the `DispatchPool` (handle) on each tokio
/// runtime thread (tokio's _rt-threaded_ feature).
///
#[cfg_attr(feature = "tokio-threaded", doc = r##"
``` rust
use blocking_permit::{
    DispatchPool, register_dispatch_pool, deregister_dispatch_pool
};

let pool = DispatchPool::builder().create();

let mut rt = tokio::runtime::Builder::new_multi_thread()
    .on_thread_start(move || {
        register_dispatch_pool(pool.clone());
    })
    .on_thread_stop(|| {
        deregister_dispatch_pool();
    })
    .build()
    .unwrap();
```
"##)]

#[derive(Clone)]
pub struct DispatchPool {
    sender: Arc<Sender>,
    ignore_panics: bool,
}

// `Arc`s may look a bit redundant above and below, but `Sender` has the `Drop`
// implementation, and counter and ws are used/moved independently in the work
// loop.

#[derive(Debug)]
struct Sender {
    ws: Arc<WorkState>,
    counter: Arc<AtomicUsize>,
}

type AroundFn = Arc<dyn Fn(usize) + Send + Sync>;

/// A builder for [`DispatchPool`] supporting an extenstive set of
/// configuration options.
pub struct DispatchPoolBuilder {
    pool_size: Option<usize>,
    queue_length: Option<usize>,
    stack_size: Option<usize>,
    name_prefix: Option<String>,
    after_start: Option<AroundFn>,
    before_stop: Option<AroundFn>,
    ignore_panics: bool
}

enum Work {
    Unit(Box<dyn FnOnce() + Send>),
    SafeUnit(AssertUnwindSafe<Box<dyn FnOnce() + Send>>),
    Terminate,
}

impl DispatchPool {
    /// Create new pool using defaults.
    pub fn new() -> DispatchPool {
        DispatchPoolBuilder::default().create()
    }

    /// Create a new builder for configuring a new pool.
    pub fn builder() -> DispatchPoolBuilder {
        DispatchPoolBuilder::new()
    }

    /// Enqueue a blocking operation to be executed.
    ///
    /// This first attempts to send to the associated queue, which will always
    /// succeed if _unbounded_, e.g. no [`DispatchPoolBuilder::queue_length`]
    /// is set, the default. If however the queue is _bounded_ and at capacity,
    /// then this task will be pushed after taking the oldest task, which is
    /// then run on the calling thread.
    pub fn spawn(&self, f: Box<dyn FnOnce() + Send>) {
        let work = if self.ignore_panics {
            Work::SafeUnit(AssertUnwindSafe(f))
        } else {
            Work::Unit(f)
        };

        let work = self.sender.send(work);

        match work {
            None => {},
            // Full, so run here. Panics will propagate.
            Some(Work::Unit(f)) => f(),
            // Full, so run here. Ignore panic unwinds.
            Some(Work::SafeUnit(af)) => {
                if catch_unwind(af).is_err() {
                    error!("DispatchPool: panic on calling thread \
                            was caught and ignored");
                }
            }
            _ => {
                // Safety: `send` will never return anything but Unit or
                // SafeUnit.
                unsafe { std::hint::unreachable_unchecked() }
            }

        }
    }
}

// Guard type that can increment thread count, then on Drop: decrements count
// and runs any before_stop function on thread.
struct Turnstile {
    index: usize,
    counter: Arc<AtomicUsize>,
    before_stop: Option<AroundFn>
}

impl Turnstile {
    fn increment(&self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for Turnstile {
    fn drop(&mut self) {
        trace!("Turnstile::drop entered");
        self.counter.fetch_sub(1, Ordering::SeqCst);
        if let Some(bsfn) = &self.before_stop {
            bsfn(self.index);
        }
    }
}

// Aborts the process if dropped
struct AbortOnPanic;

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        error!("DispatchPool: aborting due to panic on dispatch thread");
        tao_log::log::logger().flush();
        std::process::abort();
    }
}

struct WorkState {
    queue: Mutex<VecDeque<Work>>,
    limit: usize,
    condvar: Condvar,
}

impl fmt::Debug for WorkState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkState")
            .finish()
    }
}

fn work(
    index: usize,
    counter: Arc<AtomicUsize>,
    after_start: Option<AroundFn>,
    before_stop: Option<AroundFn>,
    ws: Arc<WorkState>)
{
    if let Some(ref asfn) = after_start {
        asfn(index);
    }
    drop(after_start);

    {
        let ts = Turnstile { index, counter, before_stop };
        let ws = ws; // moved to here so it drops before ts.
        let mut lock = ws.queue.lock();
        ts.increment();
        'worker: loop {
            while let Some(w) = lock.pop_front() {
                drop(lock);
                match w {
                    Work::Unit(bfn) => {
                        let abort = AbortOnPanic;
                        bfn();
                        std::mem::forget(abort);
                    }
                    Work::SafeUnit(abfn) => {
                        if catch_unwind(abfn).is_err() {
                            error!("DispatchPool: panic on pool \
                                    was caught and ignored");
                        }
                    }
                    Work::Terminate => break 'worker,
                }
                lock = ws.queue.lock();
            }

            ws.condvar.wait(&mut lock);
        }
    }
}

impl Default for DispatchPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Sender {
    // Send new work, possibly returning some, different, older work if the
    // queue is bound and its limit is reached. If queue has limit 0, then
    // always return the work given.
    fn send(&self, work: Work) -> Option<Work> {
        let mut queue = self.ws.queue.lock();
        let exempt = match work {
            Work::Terminate => true,
            _ => false
        };
        let qlen = queue.len();
        if exempt || qlen < self.ws.limit {
            queue.push_back(work);
            self.ws.condvar.notify_one();
            None
        } else if qlen > 0 && qlen == self.ws.limit {
            // Avoid the swap if front (oldest) element is a `Terminate`
            if let Some(&Work::Terminate) = queue.front() {
                Some(work)
            } else {
                // Otherwise swap old for new work
                let old = queue.pop_front().unwrap();
                queue.push_back(work);
                self.ws.condvar.notify_one();
                Some(old)
            }
        } else {
            Some(work)
        }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        trace!("Sender::drop entered");
        let threads = self.counter.load(Ordering::SeqCst);
        for _ in 0..threads {
            assert!(self.send(Work::Terminate).is_none());
        }

        // This intentionally only yields a number of times equivalent to the
        // termination messages sent, to avoid risk of hanging.
        for _ in 0..threads {
            let size = self.counter.load(Ordering::SeqCst);
            if size > 0 {
                trace!("DipatchPool::(Sender::)drop yielding, \
                        pool size: {}", size);
                thread::yield_now();
            } else {
                break;
            }
        }
    }
}

impl fmt::Debug for DispatchPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchPool")
            .field("threads", &self.sender.counter.load(Ordering::Relaxed))
            .field("ignore_panics", &self.ignore_panics)
            .finish()
    }
}

impl DispatchPoolBuilder {
    /// Create new dispatch pool builder, for configuration.
    pub fn new() -> DispatchPoolBuilder {
        DispatchPoolBuilder {
            pool_size: None,
            queue_length: None,
            stack_size: None,
            name_prefix: None,
            after_start: None,
            before_stop: None,
            ignore_panics: false,
        }
    }

    /// Set the fixed number of threads in the pool.
    ///
    /// This must at least be one (1) thread, asserted. However the value is
    /// ignored (and no threads are spawned) if queue_length is zero (0).
    ///
    /// Default: the number of logical CPU's minus one, but one at minimum:
    ///
    /// | Detected CPUs | Default Pool Size |
    /// | -------------:| -----------------:|
    /// |       0       |         1         |
    /// |       1       |         1         |
    /// |       2       |         1         |
    /// |       3       |         2         |
    /// |       4       |         3         |
    ///
    /// Detected CPUs may be influenced by simultaneous multithreading (SMT,
    /// e.g. Intel hyper-threading) or scheduler affinity. Zero (0) detected
    /// CPUs is likely an error.
    pub fn pool_size(&mut self, size: usize) -> &mut Self {
        assert!(size > 0);
        self.pool_size = Some(size);
        self
    }

    /// Set the length (aka maximum capacity or depth) of the associated
    /// dispatch task queue.
    ///
    /// The length may be zero, in which case the pool is always considered
    /// _full_ and no threads are spawned.  If the queue is ever _full_, the
    /// oldest tasks will be executed on the _calling thread_, see
    /// [`DispatchPool::spawn`].
    ///
    /// Default: unbounded (unlimited)
    pub fn queue_length(&mut self, length: usize) -> &mut Self {
        self.queue_length = Some(length);
        self
    }

    /// Set whether to catch and ignore unwinds for dispatch tasks that panic,
    /// or to abort.
    ///
    /// If true, panics are ignored. Note that the unwind safety of dispatched
    /// tasks is not well assured by the `UnwindSafe` marker trait and may
    /// later result in undefined behavior (UB) or logic bugs.
    ///
    /// If false, a panic in a dispatch pool thread will result in process
    /// abort.
    ///
    /// Default: false
    pub fn ignore_panics(&mut self, ignore: bool) -> &mut Self {
        self.ignore_panics = ignore;
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
    /// The (unique) thread index is appended to form the complete thread name.
    ///
    /// Default: "dpx-pool-N-" where N is a 0-based global pool counter.
    pub fn name_prefix<S: Into<String>>(&mut self, name_prefix: S) -> &mut Self {
        self.name_prefix = Some(name_prefix.into());
        self
    }

    /// Set a closure to be called immediately after each thread is started.
    ///
    /// The closure is passed a 0-based index of the thread.
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

    /// Create a new [`DispatchPool`] with the provided configuration.
    pub fn create(&mut self) -> DispatchPool {

        let pool_size = if let Some(0) = self.queue_length {
            // Zero pool size if zero queue length
            0
        } else if let Some(size) = self.pool_size {
            size
        } else {
            let mut size = num_cpus::get();
            if size > 1 {
                size -= 1;
            }
            if size == 0 {
                size = 1;
            }
            size
        };

        static POOL_CNT: AtomicUsize = AtomicUsize::new(0);
        let name_prefix = if let Some(ref prefix) = self.name_prefix {
            prefix.to_owned()
        } else {
            format!(
                "dpx-pool-{}-",
                POOL_CNT.fetch_add(1, Ordering::SeqCst))
        };

        let ws = if let Some(l) = self.queue_length {
            Arc::new(WorkState {
                queue: Mutex::new(VecDeque::with_capacity(l)),
                limit: l,
                condvar: Condvar::new(),
            })
        } else {
            Arc::new(WorkState {
                queue: Mutex::new(VecDeque::with_capacity(pool_size*2)),
                limit: usize::max_value(),
                condvar: Condvar::new()
            })
        };

        let sender = Arc::new(Sender {
            ws: ws.clone(),
            counter: Arc::new(AtomicUsize::new(0))
        });

        for i in 0..pool_size {
            let after_start = self.after_start.clone();
            let before_stop = self.before_stop.clone();
            let ws = ws.clone();

            let mut builder = thread::Builder::new();
            builder = builder.name(format!("{}{}", name_prefix, i));
            if let Some(size) = self.stack_size {
                builder = builder.stack_size(size);
            }
            let cnt = sender.counter.clone();
            builder
                .spawn(move || work(i, cnt, after_start, before_stop, ws))
                .expect("DispatchPoolBuilder::create thread spawn");
        }

        // Wait until counter reaches pool size.
        while sender.counter.load(Ordering::SeqCst) < pool_size {
            thread::yield_now();
        }

        DispatchPool {
            sender,
            ignore_panics: self.ignore_panics,
        }
    }
}

impl Default for DispatchPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}
