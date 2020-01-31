## 1.2.0 (TBD)
* Add `Cleaver`, a `Stream` adapter that splits buffers from a source to a
  given, maximum length (_cleaver_ feature).

* Add `YieldStream`, an adapter which yields between `Stream` items
  (_yield-stream_ feature).

* Refine must_use attributes for `Future` and `Stream` types.

* Many (rust)doc improvements.

* Extend (optional) futures-intrusive dependency to include 0.3.

* Extend num_cpus dependency to include 1.12.

## 1.1.0 (2020-1-15)
* If the `DispatchPool` queue is bounded and becomes full, [`spawn`] now pops
  the oldest operation off the queue before pushing the newest (just passed)
  operation, to ensure space while holding its lock. Then, as a fallback, it
  runs the old operation. It continues to enlist calling threads once the queue
  reaches the limit, but operation order (at least from perspective of a single
  calling thread) is preserved.

## 1.0.0 (2020-1-12)
* As a performance optimization, replace use of _crossbeam_'s MPMC channel in
  `DispatchPool` with direct use of _parking_lot's_ `Mutex` and `Condvar`, and
  a (std) `VecDeque`.  All practical features remain.  However, with
  `DispatchPoolBuilder`, setting `pool_size(0)` is no longer allowed. The same
  effect can be achieved by setting `queue_length(0)`.

## 0.1.0 (2019-12-20)
* Initial release, and hopefully the last in an 0.x state.
