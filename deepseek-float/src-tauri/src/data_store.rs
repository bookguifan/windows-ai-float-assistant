//! Coordination for the three user-data files that participate in backup/restore.
//!
//! Public reads take a shared guard and public writes take an exclusive guard. A restore can
//! therefore update config.json, prompts.json and favorites.json as one visible operation.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    RwLock, RwLockReadGuard, RwLockWriteGuard,
};

static DATA_LOCK: RwLock<()> = RwLock::new(());
static REVISION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn read_guard() -> RwLockReadGuard<'static, ()> {
    DATA_LOCK
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn write_guard() -> RwLockWriteGuard<'static, ()> {
    DATA_LOCK
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn revision() -> u64 {
    REVISION.load(Ordering::Acquire)
}

pub(crate) fn mark_changed() {
    REVISION.fetch_add(1, Ordering::AcqRel);
}
