## 0.2.0 (TBD)

* As a performance optimization, replace use of _crossbeam_'s MPMC channel in
  `DispatchPool` with direct use of _parking_lot's_ `Mutex` and `Condvar`, and
  a (std) `VecDeque`.  All practical features remain.  On
  `DispatchPoolBuilder`, setting `pool_size(0)` is no longer allowed, but the
  same effect can be achieved by setting `queue_length(0)`.

## 0.1.0 (2019-12-20)

* Initial release, and hopefully the last in an 0.x state.
