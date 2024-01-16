//! The actual implementation of [lock_api::RawRwLock] based on a [RefCell](core::cell::RefCell).
//!
//! ## Implementation Differences
//! Unfortunately, the underlying implementation cannot reuse [`core::cell::RefCell`] directly,
//! because it needs access to implementation internals.
//!
//! However, the implementation should behave identically from an external point of view.
//!
//! This implementation was based on the version from the stdlib on Jan. 15, 2014.
//! Here is a link to the original source code:
//! <https://github.com/rust-lang/rust/blob/714b29a17ff5/library/core/src/cell.rs>

use core::cell::Cell;
use core::fmt::{Display, Formatter};
use core::panic::Location;
use lock_api::{GuardNoSend, RawMutex, RawRwLock, RawRwLockRecursive};

pub struct CellMutex(CellRwLock);
unsafe impl RawMutex for CellMutex {
    #[allow(clippy::declare_interior_mutable_const)] // Used as workaround for `const fn` in trait
    const INIT: Self = CellMutex(CellRwLock::INIT);
    type GuardMarker = GuardNoSend;

    #[inline]
    #[track_caller]
    fn lock(&self) {
        self.0.lock_exclusive()
    }

    #[inline]
    #[track_caller]
    fn try_lock(&self) -> bool {
        self.0.try_lock_exclusive()
    }

    #[inline]
    #[track_caller]
    unsafe fn unlock(&self) {
        self.0.unlock_exclusive()
    }

    #[inline]
    #[track_caller]
    fn is_locked(&self) -> bool {
        self.0.is_locked()
    }
}

/// Maintains a count of the number of borrows active,
/// and whether they are mutable or immutable.
///
/// ## Original stdlib docs
/// Positive values represent the number of `Ref` active. Negative values
/// represent the number of `RefMut` active. Multiple `RefMut`s can only be
/// active at a time if they refer to distinct, nonoverlapping components of a
/// `RefCell` (e.g., different ranges of a slice).
///
/// `Ref` and `RefMut` are both two words in size, and so there will likely never
/// be enough `Ref`s or `RefMut`s in existence to overflow half of the `usize`
/// range. Thus, a `BorrowFlag` will probably never overflow or underflow.
/// However, this is not a guarantee, as a pathological program could repeatedly
/// create and then mem::forget `Ref`s or `RefMut`s. Thus, all code must
/// explicitly check for overflow and underflow in order to avoid unsafety, or at
/// least behave correctly in the event that overflow or underflow happens (e.g.,
/// see BorrowRef::new).
///
/// ## Differences from stdlib implementation
/// There are some differences from the implementation used in the stdlib:
/// 1. Multiple mutable references are forbidden
/// 2. Uses a newtype instead of a type alias
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct BorrowFlag {
    count: isize,
}
impl BorrowFlag {
    pub const UNUSED: BorrowFlag = BorrowFlag { count: 0 };
    #[inline]
    pub fn state(self) -> BorrowState {
        // USing comparison chain for speed
        #[allow(clippy::comparison_chain)]
        if self.count < 0 {
            BorrowState::MutableBorrow
        } else if self.count > 0 {
            BorrowState::SharedBorrow
        } else {
            BorrowState::Unused
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum BorrowState {
    MutableBorrow,
    Unused,
    SharedBorrow,
}

/// A single-threaded implementation of [lock_api::RawRwLock]
/// that is implemented using a [RefCell](core::cell::RefCell).
///
/// This can be used to abstract over single-threaded and multi-threaded code.
#[derive(Debug)]
pub struct CellRwLock {
    borrow_count: Cell<BorrowFlag>,
    /// Stores the location of the earliest active borrow.
    ///
    /// Should be present whenever `self.is_locked()`.
    ///
    /// Used for giving better panic messages.
    /// This is enabled in debug mode by default,
    /// but can be controlled by feature flags.
    #[cfg(debug_location)]
    earliest_borrow_location: Cell<Option<&'static Location<'static>>>,
}

impl CellRwLock {
    #[inline]
    fn earliest_borrow_location(&self) -> Option<&'static Location<'static>> {
        #[cfg(debug_location)]
        {
            self.earliest_borrow_location.get()
        }
        #[cfg(not(debug_location))]
        {
            None
        }
    }

    #[inline]
    #[track_caller]
    fn try_borrow_exclusively(&self) -> Result<(), BorrowFailError> {
        if matches!(self.borrow_count.get().state(), BorrowState::Unused) {
            assert_eq!(self.borrow_count.get().count, 0);
            self.borrow_count.set(BorrowFlag { count: -1 });
            #[cfg(debug_location)]
            self.earliest_borrow_location.set(Location::caller());
            Ok(())
        } else {
            Err(BorrowFailError {
                is_exclusive: true,
                existing_location: self.earliest_borrow_location(),
            })
        }
    }

    #[inline]
    #[track_caller]
    fn try_borrow_shared(&self) -> Result<(), BorrowFailError> {
        if matches!(
            self.borrow_count.get().state(),
            BorrowState::Unused | BorrowState::SharedBorrow
        ) {
            self.borrow_count.set(BorrowFlag {
                /*
                 * Overflow can happen if repeatedly calling mem::forget
                 *
                 * A program that leaks this rapid is so degenerate
                 * that we unconditionally panic without giving a Result::Err
                 */
                count: self
                    .borrow_count
                    .get()
                    .count
                    .checked_add(1)
                    .expect("Overflow shared borrows"),
            });
            Ok(())
        } else {
            debug_assert_eq!(self.borrow_count.get().state(), BorrowState::MutableBorrow);
            Err(BorrowFailError {
                is_exclusive: false,
                existing_location: self.earliest_borrow_location(),
            })
        }
    }
}
#[derive(Debug)]
struct BorrowFailError {
    is_exclusive: bool,
    existing_location: Option<&'static Location<'static>>,
}

impl BorrowFailError {
    #[cold]
    #[track_caller]
    pub fn panic(&self) -> ! {
        panic!("{self}")
    }
}
impl Display for BorrowFailError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str("Unable to ")?;
        if self.is_exclusive {
            f.write_str("exclusively ")?
        }
        f.write_str("borrow")?;
        if let Some(existing_location) = self.existing_location {
            write!(
                f,
                ": {existing_borrow_kind} borrowed at {existing_location}",
                existing_borrow_kind = if self.is_exclusive {
                    "Already"
                } else {
                    "Exclusively"
                }
            )?;
        }
        Ok(())
    }
}
unsafe impl RawRwLock for CellRwLock {
    #[allow(clippy::declare_interior_mutable_const)] // Used as workaround for `const fn` in trait
    const INIT: Self = CellRwLock {
        borrow_count: Cell::new(BorrowFlag::UNUSED),
        #[cfg(debug_location)]
        earliest_borrow_location: Cell::new(None),
    };
    type GuardMarker = GuardNoSend;

    #[track_caller]
    #[inline]
    fn lock_shared(&self) {
        /*
         * TODO: Do we want to require using read_recursive?
         *
         * This may be a stumbling block when switching to a real
         * lock which blocks on recursive block.
         */
        match self.try_borrow_shared() {
            Ok(()) => {}
            Err(fail) => fail.panic(),
        }
    }

    #[track_caller]
    #[inline]
    fn try_lock_shared(&self) -> bool {
        self.try_borrow_shared().is_ok()
    }

    #[inline]
    #[track_caller]
    unsafe fn unlock_shared(&self) {
        debug_assert_eq!(self.borrow_count.get().state(), BorrowState::SharedBorrow);
        debug_assert!(self.borrow_count.get().count > 0);
        self.borrow_count.set(BorrowFlag {
            count: self.borrow_count.get().count - 1,
        });
        if !self.is_locked() {
            #[cfg(debug_location)]
            self.earliest_borrow_location.set(None);
        }
    }

    #[inline]
    #[track_caller]
    fn lock_exclusive(&self) {
        match self.try_borrow_exclusively() {
            Ok(()) => (),
            Err(e) => e.panic(),
        }
    }

    #[inline]
    #[track_caller]
    fn try_lock_exclusive(&self) -> bool {
        self.try_borrow_exclusively().is_ok()
    }

    #[inline]
    #[track_caller]
    unsafe fn unlock_exclusive(&self) {
        debug_assert_eq!(self.borrow_count.get().state(), BorrowState::MutableBorrow);
        debug_assert!(self.borrow_count.get().count < 0);
        self.borrow_count.set(BorrowFlag {
            count: self.borrow_count.get().count + 1,
        });
        if !self.is_locked() {
            #[cfg(debug_location)]
            self.earliest_borrow_location.set(None);
        }
    }

    #[inline]
    fn is_locked(&self) -> bool {
        match self.borrow_count.get().state() {
            BorrowState::Unused => false,
            BorrowState::MutableBorrow | BorrowState::SharedBorrow => true,
        }
    }

    #[inline]
    fn is_locked_exclusive(&self) -> bool {
        matches!(self.borrow_count.get().state(), BorrowState::MutableBorrow)
    }
}
unsafe impl RawRwLockRecursive for CellRwLock {
    #[inline]
    #[track_caller]
    fn lock_shared_recursive(&self) {
        self.lock_shared()
    }

    #[inline]
    #[track_caller]
    fn try_lock_shared_recursive(&self) -> bool {
        self.try_lock_shared()
    }
}
