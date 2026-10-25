//! Run a job for each item of a list on several threads and give the results back in the order of the list.
//! Used to hash many files at once (Folder, Verify, Duplicates); `crate::file_workers` says how many threads.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// `job(i)` for i in 0..count on `workers` threads. Items are taken in order. Returns None if `cancel` was set
/// before every item was done; otherwise the results in order. A job that notices `cancel` itself should return
/// quickly; its result is dropped in that case.
pub fn map_indexed<T: Send>(count: usize, workers: usize, cancel: &AtomicBool, job: impl Fn(usize) -> T + Sync) -> Option<Vec<T>> {
    let workers = workers.clamp(1, count.max(1));
    if workers == 1 {
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            if cancel.load(Ordering::Relaxed) { return None; }
            out.push(job(i));
        }
        return Some(out);
    }
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<T>>> = (0..count).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) { break; }
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= count { break; }
                let r = job(i);
                *slots[i].lock().unwrap() = Some(r);
            });
        }
    });
    let out: Vec<Option<T>> = slots.into_iter().map(|m| m.into_inner().unwrap()).collect();
    if out.iter().any(Option::is_none) { None } else { Some(out.into_iter().map(Option::unwrap).collect()) }
}
