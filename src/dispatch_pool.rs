use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::panic::{catch_unwind, AssertUnwindSafe};

use log::{debug, error, trace};
use crossbeam_channel as cbch;
use num_cpus;

/// A specialized thread pool and queue for dispatching _blocking_
/// (synchronous, long running) operations.
///
/// This pool is not an _executor_, has no _waking_ facilities, etc. As
/// compared with other thread pools supporting `spawn_dispatch` (here called
/// just `dispatch`), for example in _tokio_ and _async-std_, this pool has
/// some unique features:
///
/// * A configurable, fixed number of threads created before return from
///   construction and terminated on `Drop::drop`.  Consistent memory
///   footprint. No warmup required. No per-task thread management overhead.
///
/// * Supports configuration for all crossbeam-channel MPMC queue options
///   including fixed (bounded), including zero capacity, or an unbounded
///   queue.
///
/// * Configurable panic handling policy: Either catches and logs dispatch
///   panics, or aborts the process, on panic unwind.
///
/// * For comparative testing, can also be configured with zero (0) threads.
///
/// * With zero threads or when the queue is bounded and becomes full,
///   [`DispatchPool::spawn`] runs the provided task on the calling thread as a
///   fallback.  This is entirely optional and avoided with default settings of
///   an unbounded queue and one or more threads. Its most useful for testing
///   and benchmarking.
///
/// ## Usage
///
/// By default, the pool uses an unbounded MPMC channel, with the assumption
/// that resource/capacity is externally constrained, for example via
/// [`Semaphore`](crate::Semaphore). Once constructed, a fixed number of
/// threads are spawned and the instance acts as a handle to the pool. This may
/// be inexpensively cloned for additional handles to the same pool.
///
/// See [`DispatchPoolBuilder`] for an extensive set of options, some of which
/// are more appropriate for testing than production use.
#[derive(Clone)]
pub struct DispatchPool {
    sender: Arc<Sender>,
    ignore_panics: bool,
}

#[derive(Debug)]
struct Sender {
    tx: cbch::Sender<Work>,
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
    Count,
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
    /// or the pool is configured with zero (`pool_size`) threads, then the
    /// task is directly run by the _calling_ thread.
    pub fn spawn(&self, f: Box<dyn FnOnce() + Send>) {
        let work = if self.ignore_panics {
            Work::SafeUnit(AssertUnwindSafe(f))
        } else {
            Work::Unit(f)
        };

        let work = match self.sender.tx.try_send(work) {
            Ok(()) => return,
            Err(cbch::TrySendError::Disconnected(w)) => {
                // Rx side is dropped if zero pool threads
                // Should be expected so
                w
            }
            Err(cbch::TrySendError::Full(w)) => {
                debug!("DispathPool::spawn failed to send: full; \
                        running on calling thread!");
                w
            }
        };

        // Failed to send, so run on this calling thread. Ignore panics unwinds
        // if requested, but no need to abort otherwise (as that panic will
        // propagate).
        match work {
            Work::Unit(f) => f(),
            Work::SafeUnit(af) => {
                if catch_unwind(af).is_err() {
                    error!("DispatchPool: panic on calling thread \
                            was caught and ignored");
                }
            }
            _ => {
                // Safety: work is constructed above as Unit or SafeUnit only.
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
        log::logger().flush();
        std::process::abort();
    }
}

fn work(
    index: usize,
    counter: Arc<AtomicUsize>,
    after_start: Option<AroundFn>,
    before_stop: Option<AroundFn>,
    rx: cbch::Receiver<Work>)
{
    if let Some(ref asfn) = after_start {
        asfn(index);
    }
    drop(after_start);

    {
        let ts = Turnstile { index, counter, before_stop };
        let rx = rx; // moved to here so it drops before ts.
        loop {
            match rx.recv() {
                Ok(Work::Count) => {
                    ts.increment();
                    // In startup phase. Give another thread a chance to take
                    // the next Work::Count.
                    thread::yield_now();
                }
                Ok(Work::Unit(bfn)) => {
                    let abort = AbortOnPanic;
                    bfn();
                    std::mem::forget(abort);
                }
                Ok(Work::SafeUnit(abfn)) => {
                    if catch_unwind(abfn).is_err() {
                        error!("DispatchPool: panic on pool \
                                was caught and ignored");
                    }
                }
                Ok(Work::Terminate) => break,
                Err(r) => {
                    debug_assert!(false, "DispatchPool::recv error {}", r);
                    error!("DispatchPool::recv error {}", r);
                }
            }
        }
    }
}

impl Default for DispatchPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        trace!("Sender::drop entered");
        let threads = self.counter.load(Ordering::SeqCst);
        let mut terms = 0;
        for _ in 0..threads {
            if let Err(e) = self.tx.try_send(Work::Terminate) {
                debug!("DispatchPool::(Sender::)drop on terminate send: {}", e);
                break;
            }
            terms += 1;
        }
        // This intentionally only yields a number of times equivalent to the
        // termination messages sent, to avoid any risk of hanging.
        for _ in 0..terms {
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
    /// This may be zero, in which case, all tasks are executed on the _calling
    /// thread_, see [`DispatchPool::spawn`]. This is allowed primarly as a
    /// convenience for testing and comparative benchmarking.
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
        self.pool_size = Some(size);
        self
    }

    /// Set the length (aka maximum capacity or depth) of the associated
    /// dispatch task queue.
    ///
    /// Note that if this length is ever exceeded, tasks will be executed on
    /// the _calling thread_, see [`DispatchPool::spawn`].  A length of
    /// zero (0) is an accepted value, and guaruntees a task will be run
    /// _immediately_ by a pool thread, or the calling thread.
    ///
    /// If pool_size is set to zero (0) threads this setting is ignored and no
    /// queue is used.
    ///
    /// Default: unbounded
    pub fn queue_length(&mut self, length: usize) -> &mut Self {
        self.queue_length = Some(length);
        self
    }

    /// Set whether to catch and ignore unwinds for dispatch tasks that panic,
    /// or to abort.
    ///
    /// When true, panics are ignored. Note that the unwind safety of
    /// dispatched tasks is not well assured by the `UnwindSafe` marker trait
    /// and may later result in undefined behavior (UB) or logic bugs.
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

    /// Create a new [`DispatchPool`](DispatchPool) with the provided
    /// configuration.
    pub fn create(&mut self) -> DispatchPool {

        let pool_size = if let Some(size) = self.pool_size {
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

        let (tx, rx) = if pool_size == 0 {
            // If no threads, then for safety, we also don't want to queue.
            cbch::bounded(0)
        } else if let Some(len) = self.queue_length {
            cbch::bounded(len)
        } else {
            cbch::unbounded()
        };

        static POOL_CNT: AtomicUsize = AtomicUsize::new(0);
        let name_prefix = if let Some(ref prefix) = self.name_prefix {
            prefix.to_owned()
        } else {
            format!(
                "dpx-pool-{}-",
                POOL_CNT.fetch_add(1, Ordering::SeqCst))
        };

        let sender = Sender {
            tx,
            counter: Arc::new(AtomicUsize::new(0))
        };

        for i in 0..pool_size {
            let after_start = self.after_start.clone();
            let before_stop = self.before_stop.clone();
            let rx = rx.clone();

            let mut builder = thread::Builder::new();
            builder = builder.name(format!("{}{}", name_prefix, i));
            if let Some(size) = self.stack_size {
                builder = builder.stack_size(size);
            }
            let cnt = sender.counter.clone();
            builder
                .spawn(move || work(i, cnt, after_start, before_stop, rx))
                .expect("DispatchPoolBuilder::create thread spawn");

            // Send a task to count the new thread, possibly blocking if
            // bounded, until _some_ thread is available.
            sender.tx.send(Work::Count).expect("success blank");
        }

        // Wait until counter reaches pool size. This is not particularly a
        // guaruntee of _all_ threads, but does guaruntee _one_ thread,
        // which is material for the zero queue length case.
        while sender.counter.load(Ordering::SeqCst) < pool_size {
            thread::yield_now();
        }

        DispatchPool {
            sender: Arc::new(sender),
            ignore_panics: self.ignore_panics,
        }
    }
}

impl Default for DispatchPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}
