## 1.2.0 (TBD)
* Add `Cleaver`, a `Stream` adapter that splits buffers from a source to a
  given, maximum length.

* Add must_use attributes to `Future` returning functions.

## 1.1.0 (2020-1-15)
* If the `DispatchPool` queue is bounded and becomes full, [`spawn`] now pops
  the oldest operation off the queue before pushing the newest (just passed)
  operation, to ensure space while holding its lock. Then, as a fallback, it runs
  the old operation. Thus we enlist calling threads once the queue reaches
  the limit, but operation order (at least from perspective of a single thread) is
  preserved.

## 1.0.0 (2020-1-12)
* As a performance optimization, replace use of _crossbeam_'s MPMC channel in
  `DispatchPool` with direct use of _parking_lot's_ `Mutex` and `Condvar`, and
  a (std) `VecDeque`.  All practical features remain.  However, with
  `DispatchPoolBuilder`, setting `pool_size(0)` is no longer allowed. The
  same effect can be achieved by setting `queue_length(0)`.

## 0.1.0 (2019-12-20)
* Initial release, and hopefully the last in an 0.x state.
