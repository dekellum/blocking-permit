## 1.3.3 (2021-10-28)
* Broaden tokio dependency to include 1.2-1.12 releases.

* Broaden _bytes_ dependency to include 1.1.

## 1.3.2 (2021-1-27)
* Use tao-log dependency exclusively, no direct log dep.

## 1.3.1 (2021-1-23)
* Broaden tokio dependency to include new 1.1.z releases.

* Add previously missing LICENSE-(APACHE/MIT) files to repo and package.

* Update dev. dependencies rand (to 0.8.0) and broaden tempfile (incl. 3.2).

* Add clippy config for primordial MSRV build.rs and for current MSRV.

## 1.3.0 (2021-1-8)
* Update to bytes 1.0 and tokio 1.0.1 (MSRV 1.45.2)

* Update futures-intrusive to include 0.4, and parking_lot min to 0.10.0.

* Minimum supported rust version is now 1.45.2 (to match above dep updates).

* Remove prior remove_dir_all transitive restriction based on MSRV.

* Misc. documentation improvements.

## 1.2.1 (2020-8-29)
* Extend num_cpus dependency to include 1.13.

* Extend parking_lot dependency to include 0.11.

* Restrict remove_dir_all transitive dep of tempfile to 0.5.2 to preserve MSRV.

* Misc. documentation improvements.

## 1.2.0 (2020-2-4)
* Add `Cleaver`, a `Stream` adapter that splits buffers from a source to a
  given, maximum length (_cleaver_ feature).

* Add `YieldStream`, an adapter which yields between `Stream` items
  (_yield-stream_ feature).

* Refine must_use attributes for `Future` and `Stream` types.

* Many (rust)doc improvements.

* Extend (optional) futures-intrusive dependency to include 0.3.

* Extend num_cpus dependency to include 1.12.

* Replace env_logger (dev dependency) with piccolog.

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
