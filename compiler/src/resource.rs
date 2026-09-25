//! A counting global allocator, so a check can be bounded in memory as well as in stack.
//!
//! Several of the checker's analyses copy or substitute expressions, and some shapes of valid
//! source make that grow exponentially or quadratically (review of the analysis limits,
//! 2026-09-24). Unbounded, such a check exhausts the machine: the kernel's out-of-memory killer
//! then takes down whatever shares the check's session, the terminal included.
//!
//! The command line installs [`CountingAlloc`] as its global allocator (`tools/anubis`). While a
//! check request is in progress (`middle::analysis_limit::check`), two budgets apply, counted from
//! what was allocated when the request began:
//! - past the soft budget, the analysis in progress is refused at its next guarded step
//!   (`ANUBIS_ANALYSIS_LIMIT`);
//! - past the hard budget (an analysis the guard does not reach kept allocating), the process
//!   prints that diagnostic and exits with status 1 at once.
//!
//! The budgets come from the memory this process can use when the request begins: the smaller of
//! what the system has available (`MemAvailable`: free memory plus what the kernel can reclaim)
//! and, for every cgroup v2 above the process with a `memory.max`, what it has left (its
//! `memory.max` less its `memory.current`, counting its inactive file cache as free). The soft
//! budget is half of it, the hard one two thirds (the process's resident memory runs about a third
//! above what it has allocated, so at the hard budget it still fits). While the request grows, that
//! headroom is read again every 64 MiB it allocates: when what is left falls below a reserve (an
//! eighth of the limit, at least 128 MiB), the analysis refuses at its next guarded step. Checks
//! run side by side in one scope each armed with the whole scope's headroom and were killed
//! together by its OOM killer before either refused (review of the checker limits); now whichever
//! reaches its next step first refuses. An earlier version took a sixth of the limit,
//! at most 4 GiB: review showed it refused valid programs that fit in the machine several times
//! over (a 600-branch `else if`, a 2000-piece string, closure stress programs), so a check's
//! verdict depended on the machine far more than it had to.
//!
//! The verdict of a program near its budget still depends on the memory the machine has free.
//! `ANUBIS_ANALYSIS_MEMORY_MIB` fixes the soft budget (the hard one is twice it), for a
//! reproducible verdict. The budgets bound the checker's analyses (a check request); parsing before
//! it, and the solver, evidence and code generation after it, are not counted against them, and
//! checks run side by side each take their budget from the memory free when they begin. Outside a
//! check request nothing is limited. A program that embeds the compiler without installing the
//! allocator has no memory budget, only the stack and depth limits.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{
    AtomicBool, AtomicPtr, AtomicUsize,
    Ordering::{Acquire, Relaxed, Release},
};

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Past this, the analysis in progress is refused (0: no request in progress).
static SOFT: AtomicUsize = AtomicUsize::new(0);
/// Past this, the process exits (0: no request in progress).
static HARD: AtomicUsize = AtomicUsize::new(0);
static OVER: AtomicBool = AtomicBool::new(false);
/// Past this much allocated, the headroom is read again (0: never).
static NEXT_LOOK: AtomicUsize = AtomicUsize::new(0);
/// Below this much headroom the request in progress refuses; below half of it, it exits.
static RESERVE: AtomicUsize = AtomicUsize::new(0);
/// The cgroup with the least memory left when the request began: its `memory.current` and
/// `memory.max` (NUL-terminated paths, leaked, so the allocator can read them without allocating)
/// and its inactive file cache then.
static CG_CURRENT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static CG_MAX: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static CG_CACHE: AtomicUsize = AtomicUsize::new(0);
/// How much allocation passes between two readings of the headroom.
const LOOK_EVERY: usize = 64 * MIB;
/// The soft budget of the request in progress, in bytes (for its refusal's message).
static BUDGET: AtomicUsize = AtomicUsize::new(0);
/// What the hard exit also writes to standard output (a machine-readable refusal, when the command
/// line was asked for one), set once before a check.
static EXIT_REPORT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static EXIT_REPORT_LEN: AtomicUsize = AtomicUsize::new(0);

const MIB: usize = 1 << 20;
/// The budgets when the platform reports no memory figure at all.
const SOFT_DEFAULT: usize = 4096 * MIB;
const HARD_DEFAULT: usize = 8192 * MIB;
/// The hard exit's refusal. Written from inside the allocator, so it cannot be formatted there.
pub const HARD_EXIT_DIAGNOSTIC: &str = "ANUBIS_ANALYSIS_LIMIT: the checker's analysis of this \
program kept allocating past its hard memory budget (two thirds of the memory free when the check \
began, or twice ANUBIS_ANALYSIS_MEMORY_MIB); the check did not complete, so the program is \
refused. This is a limit of the checker, not a finding about the program: simplify it, or give \
the check more memory (ANUBIS_ANALYSIS_MEMORY_MIB, in MiB) if the machine has it to spare. No \
evidence bundle is written on this exit";
const EXIT_NEWLINE: &[u8] = b"\n";

/// The system allocator, counting the bytes in use.
pub struct CountingAlloc;

// SAFETY: every call is forwarded unchanged to `System`; the counters only observe sizes.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: the caller's contract for `alloc` is passed through unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: as for `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        ALLOCATED.fetch_sub(layout.size(), Relaxed);
        // SAFETY: `ptr` was allocated by `System` with `layout` (every allocation goes through it).
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() {
            charge(new_size - layout.size());
        } else {
            ALLOCATED.fetch_sub(layout.size() - new_size, Relaxed);
        }
        // SAFETY: as for `dealloc`, and `new_size` is the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn charge(n: usize) {
    let now = ALLOCATED.fetch_add(n, Relaxed).saturating_add(n);
    // Every LOOK_EVERY allocated, what the machine and the tightest cgroup have left is read again
    // (without allocating: this is the allocator). Checks run side by side in one scope, or a
    // machine filling up, would otherwise reach the OOM killer before any budget: below the
    // reserve the analysis refuses at its next guarded step; below half of it, at once.
    let look = NEXT_LOOK.load(Relaxed);
    if look != 0 && now > look {
        NEXT_LOOK.store(now.saturating_add(LOOK_EVERY), Relaxed);
        let reserve = RESERVE.load(Relaxed);
        if reserve > 0 {
            if let Some(left) = left_now() {
                if left < reserve / 2 {
                    exit_over_budget();
                }
                if left < reserve {
                    OVER.store(true, Relaxed);
                }
            }
        }
    }
    let soft = SOFT.load(Relaxed);
    if soft != 0 && now > soft {
        OVER.store(true, Relaxed);
        let hard = HARD.load(Relaxed);
        if hard != 0 && now > hard {
            exit_over_budget();
        }
    }
}

/// Exit at once with the diagnostic, allocating nothing (this runs inside the allocator).
fn exit_over_budget() -> ! {
    #[cfg(unix)]
    // SAFETY: `write` reads static bytes only (the diagnostic, and the report `set_exit_report` was
    // given, which is leaked, so it lives for the process); `_exit` does not return.
    unsafe {
        libc::write(2, EXIT_NEWLINE.as_ptr().cast(), 1);
        libc::write(
            2,
            HARD_EXIT_DIAGNOSTIC.as_ptr().cast(),
            HARD_EXIT_DIAGNOSTIC.len(),
        );
        libc::write(2, EXIT_NEWLINE.as_ptr().cast(), 1);
        let report = EXIT_REPORT.load(Relaxed);
        let len = EXIT_REPORT_LEN.load(Relaxed);
        if !report.is_null() && len > 0 {
            libc::write(1, report.cast(), len);
        }
        libc::_exit(1)
    }
    #[cfg(not(unix))]
    std::process::abort()
}

/// Declare that [`CountingAlloc`] is the global allocator (the command line, at start).
pub fn install() {
    INSTALLED.store(true, Relaxed);
}

/// What the hard exit also writes to standard output (the command line's `--message-format json`
/// refusal), so a consumer reading the stream sees a verdict rather than nothing.
pub fn set_exit_report(report: &'static [u8]) {
    EXIT_REPORT_LEN.store(0, Relaxed);
    EXIT_REPORT.store(report.as_ptr().cast_mut(), Relaxed);
    EXIT_REPORT_LEN.store(report.len(), Relaxed);
}

/// The soft budget of the request in progress (or the last one), in MiB.
pub fn soft_budget_mib() -> usize {
    BUDGET.load(Relaxed) / MIB
}

/// Whether the memory in use has passed the soft budget of the request in progress, or the memory
/// left to the process (read again as it grows) has fallen below the reserve.
pub(crate) fn over_budget() -> bool {
    OVER.load(Relaxed)
}

/// The budgets of one check request, from what is in use now. Returns what to restore when the
/// request ends (`None`: the allocator is not installed, nothing was changed).
pub(crate) fn arm() -> Option<(usize, usize, bool)> {
    if !INSTALLED.load(Relaxed) {
        return None;
    }
    let (soft, hard) = budgets();
    BUDGET.store(soft, Relaxed);
    let base = ALLOCATED.load(Relaxed);
    // Read the headroom again every LOOK_EVERY allocated (not under ANUBIS_ANALYSIS_MEMORY_MIB,
    // which fixes the budget): an eighth of what was usable, at least 128 MiB, is kept free.
    let fixed = std::env::var_os("ANUBIS_ANALYSIS_MEMORY_MIB").is_some();
    RESERVE.store(if fixed { 0 } else { (soft / 4).max(128 * MIB) }, Relaxed);
    #[cfg(target_os = "linux")]
    match cgroup_scan() {
        Some((_, dir, cache)) => {
            set_path(&CG_CURRENT, &dir.join("memory.current"));
            set_path(&CG_MAX, &dir.join("memory.max"));
            CG_CACHE.store(cache, Relaxed);
        }
        None => {
            CG_CURRENT.store(std::ptr::null_mut(), Release);
            CG_MAX.store(std::ptr::null_mut(), Release);
        }
    }
    NEXT_LOOK.store(base.saturating_add(LOOK_EVERY), Relaxed);
    let prev = (SOFT.load(Relaxed), HARD.load(Relaxed), OVER.load(Relaxed));
    OVER.store(false, Relaxed);
    SOFT.store(base.saturating_add(soft), Relaxed);
    HARD.store(base.saturating_add(hard), Relaxed);
    Some(prev)
}

/// Put back the budgets `arm` replaced.
pub(crate) fn disarm(prev: (usize, usize, bool)) {
    SOFT.store(prev.0, Relaxed);
    HARD.store(prev.1, Relaxed);
    OVER.store(prev.2, Relaxed);
}

fn budgets() -> (usize, usize) {
    if let Some(mib) = std::env::var("ANUBIS_ANALYSIS_MEMORY_MIB")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|m| *m > 0)
    {
        let soft = mib.saturating_mul(MIB);
        return (soft, soft.saturating_mul(2));
    }
    match usable_memory() {
        Some(free) => (free / 2, free / 3 * 2),
        None => (SOFT_DEFAULT, HARD_DEFAULT),
    }
}

/// The memory this process can use now: what the system has available, and at most what every
/// cgroup above the process with a `memory.max` has left.
fn usable_memory() -> Option<usize> {
    let system = available_memory().or_else(physical_memory);
    match (system, cgroup_left()) {
        (Some(s), Some(c)) => Some(s.min(c)),
        (s, c) => s.or(c),
    }
}

/// `MemAvailable` from `/proc/meminfo`, in bytes.
#[cfg(target_os = "linux")]
fn available_memory() -> Option<usize> {
    mem_available(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(not(target_os = "linux"))]
fn available_memory() -> Option<usize> {
    None
}

fn mem_available(meminfo: &str) -> Option<usize> {
    let kib = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemAvailable:"))?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<usize>()
        .ok()?;
    Some(kib.saturating_mul(1024))
}

#[cfg(unix)]
fn physical_memory() -> Option<usize> {
    // SAFETY: `sysconf` only reads system configuration.
    let (pages, page) = unsafe {
        (
            libc::sysconf(libc::_SC_PHYS_PAGES),
            libc::sysconf(libc::_SC_PAGESIZE),
        )
    };
    (pages > 0 && page > 0).then(|| (pages as usize).saturating_mul(page as usize))
}

/// The least memory any cgroup v2 on the path from this process's cgroup to the root has left: its
/// `memory.max` less its `memory.current`, its inactive file cache counted as free (a check run in
/// a memory-capped scope must refuse before the scope's OOM killer ends it, whatever else runs in
/// the scope).
#[cfg(target_os = "linux")]
fn cgroup_left() -> Option<usize> {
    cgroup_scan().map(|(left, _, _)| left)
}

/// The cgroup with the least memory left on the path from this process's to the root: what it has
/// left, its directory, and its inactive file cache.
#[cfg(target_os = "linux")]
fn cgroup_scan() -> Option<(usize, std::path::PathBuf, usize)> {
    let own = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let rel = own.lines().find_map(|l| l.strip_prefix("0::"))?.trim();
    let mut dir = std::path::PathBuf::from("/sys/fs/cgroup");
    dir.push(rel.trim_start_matches('/'));
    let mut best: Option<(usize, std::path::PathBuf, usize)> = None;
    loop {
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
        if let Some(max) = read("memory.max").and_then(|v| v.trim().parse::<usize>().ok()) {
            let current = read("memory.current")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let cache = read("memory.stat").map_or(0, |s| inactive_file(&s));
            let here = max.saturating_sub(current.saturating_sub(cache));
            if best.as_ref().is_none_or(|(l, _, _)| here < *l) {
                best = Some((here, dir.clone(), cache));
            }
        }
        if dir == std::path::Path::new("/sys/fs/cgroup") || !dir.pop() {
            break;
        }
    }
    best
}

/// Point `slot` at a NUL-terminated copy of `path`, leaked (kept when it is the same path).
#[cfg(target_os = "linux")]
fn set_path(slot: &AtomicPtr<u8>, path: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    let cur = slot.load(Acquire);
    // SAFETY: a non-null slot points at a leaked NUL-terminated copy (below).
    if !cur.is_null() && unsafe { std::ffi::CStr::from_ptr(cur.cast()) }.to_bytes() == bytes {
        return;
    }
    let mut v = bytes.to_vec();
    v.push(0);
    slot.store(Box::leak(v.into_boxed_slice()).as_mut_ptr(), Release);
}

/// What the machine and the tightest cgroup have left now, read without allocating (from inside
/// the allocator).
#[cfg(target_os = "linux")]
fn left_now() -> Option<usize> {
    let mut buf = [0u8; 1024];
    // SAFETY: a literal NUL-terminated path; `read_raw` writes only into `buf`.
    let meminfo = unsafe { read_raw(c"/proc/meminfo".as_ptr().cast(), &mut buf) };
    let mut left = meminfo.and_then(|n| {
        let text = &buf[..n];
        let at = text.windows(13).position(|w| w == b"MemAvailable:")?;
        let digits = text[at + 13..].iter().skip_while(|c| **c == b' ');
        leading_number(digits.copied()).map(|kib| kib.saturating_mul(1024))
    });
    let (cur, max) = (CG_CURRENT.load(Acquire), CG_MAX.load(Acquire));
    if !cur.is_null() && !max.is_null() {
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        // SAFETY: both point at leaked NUL-terminated paths (`set_path`).
        let (c, m) = unsafe { (read_raw(cur, &mut a), read_raw(max, &mut b)) };
        if let (Some(c), Some(m)) = (c, m) {
            if let (Some(c), Some(m)) = (
                leading_number(a[..c].iter().copied()),
                leading_number(b[..m].iter().copied()),
            ) {
                let here = m.saturating_sub(c.saturating_sub(CG_CACHE.load(Relaxed)));
                left = Some(left.map_or(here, |l| l.min(here)));
            }
        }
    }
    left
}

#[cfg(not(target_os = "linux"))]
fn left_now() -> Option<usize> {
    None
}

/// Read a file into `buf` with raw system calls (no allocation). Returns how many bytes.
///
/// # Safety
/// `path` must point at a NUL-terminated path.
#[cfg(target_os = "linux")]
unsafe fn read_raw(path: *const u8, buf: &mut [u8]) -> Option<usize> {
    let fd = libc::open(path.cast(), libc::O_RDONLY | libc::O_CLOEXEC);
    if fd < 0 {
        return None;
    }
    let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
    libc::close(fd);
    (n > 0).then_some(n as usize)
}

/// The decimal number at the start of `bytes`.
fn leading_number(bytes: impl Iterator<Item = u8>) -> Option<usize> {
    let mut v: usize = 0;
    let mut any = false;
    for c in bytes {
        if !c.is_ascii_digit() {
            break;
        }
        v = v.checked_mul(10)?.checked_add(usize::from(c - b'0'))?;
        any = true;
    }
    any.then_some(v)
}

/// `inactive_file` from a cgroup's `memory.stat` (page cache the kernel reclaims first).
fn inactive_file(stat: &str) -> usize {
    stat.lines()
        .find_map(|l| l.strip_prefix("inactive_file "))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn cgroup_left() -> Option<usize> {
    None
}

#[cfg(not(unix))]
fn physical_memory() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_available_reads_the_meminfo_line() {
        let meminfo = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\n\
                       MemAvailable:   16000000 kB\nBuffers:          100000 kB\n";
        assert_eq!(mem_available(meminfo), Some(16_000_000 * 1024));
        assert_eq!(mem_available("MemTotal: 5 kB\n"), None);
        assert_eq!(mem_available("MemAvailable: lots\n"), None);
    }

    #[test]
    fn leading_number_reads_digits() {
        assert_eq!(
            leading_number(b"3221225472\n".iter().copied()),
            Some(3221225472)
        );
        assert_eq!(leading_number(b"max\n".iter().copied()), None);
        assert_eq!(leading_number(b"".iter().copied()), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn left_now_reads_the_machine() {
        assert!(left_now().is_some_and(|l| l > 0));
    }

    #[test]
    fn inactive_file_reads_the_stat_line() {
        let stat = "anon 1000\nfile 5000\nactive_file 3000\ninactive_file 2000\n";
        assert_eq!(inactive_file(stat), 2000);
        assert_eq!(inactive_file("anon 1\n"), 0);
    }
}
