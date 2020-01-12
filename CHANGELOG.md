## 1.0.0 (2020-1-12)

* As a performance optimization, replace use of _crossbeam_'s MPMC channel in
  `DispatchPool` with direct use of _parking_lot's_ `Mutex` and `Condvar`, and
  a (std) `VecDeque`.  All practical features remain.  However, with
  `DispatchPoolBuilder`, setting `pool_size(0)` is no longer allowed. The
  same effect can be achieved by setting `queue_length(0)`.

## 0.1.0 (2019-12-20)

* Initial release, and hopefully the last in an 0.x state.
