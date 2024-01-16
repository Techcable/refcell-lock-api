#![cfg_attr(not(test), no_std)]
#![doc = include_str!("../README.md")]

pub mod raw;

/// A single-threaded [lock_api::Mutex] using a [RefCell](core::cell::RefCell) internally.
///
/// A [CellRwLock] is typically more useful,
/// and has no additional overhead.
pub type CellMutex<T> = lock_api::Mutex<raw::CellMutex, T>;

/// A single-threaded [lock_api::RwLock] using a [RefCell](core::cell::RefCell) internally.
///
/// Useful to abstract between single-threaded and multi-threaded code.
pub type CellRwLock<T> = lock_api::RwLock<raw::CellRwLock, T>;

#[cfg(test)]
mod test {
    use super::CellRwLock;

    #[test]
    fn basic_rwlock() {
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
        {
            let guard = lock.read();
            assert_eq!(*guard, vec![7, 18, 19]);
            {
                let guard = lock.read();
                assert_eq!(guard.first(), Some(&7));
                assert_eq!(guard.last(), Some(&19))
            }
        }
        {
            let mut guard = lock.write();
            guard.push(42);
        }
        assert_eq!(lock.into_inner(), vec![7, 18, 19, 42]);
    }
}
