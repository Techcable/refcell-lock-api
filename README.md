refcell-lock-api
=================
An single-threaded implementation of [lock_api] using a [RefCell].

This is primarily intended to allow abstracting over single-threaded and multi-threaded code.

## Example
```rust
use cell_lock_api::CellRwLock;
fn main() {
    let lock = CellRwLock::new(vec![7i32]);
    {
        let guard = lock.read();
        assert_eq!(*guard, vec![7]);
    }
    {
        let mut guard = lock.write();
        guard.push(18);
        guard.push(19);
    }
    assert_eq!(lock.into_inner(), vec![7, 18, 19])
}
```

[lock_api]: https://docs.rs/lock_api/
[RefCell]: https://doc.rust-lang.org/std/cell/struct.RefCell.html