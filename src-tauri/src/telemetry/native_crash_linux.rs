//! Linux half of [`super`] — POSIX signals.
//!
//! SundayStage does not ship a Linux build, but Linux is where CI runs
//! `cargo test`, so this is the platform the real-crash integration test in
//! [`super::tests`] actually exercises on every pull request. That makes it the
//! most-tested of the three rather than the least, which is a slightly funny but
//! entirely welcome accident.
//!
//! Here the callback really does run in a signal handler — `crash-handler`
//! installs the handlers on an alternate stack and, on `Handled(false)`,
//! restores the previous `sigaction` and lets the signal re-trigger. That is
//! what keeps Rust's own `thread '…' has overflowed its stack` message and the
//! true exit status.
//!
//! **One honest limit.** The alternate stack is installed for the thread that
//! calls `attach`, which is the main thread. A **stack overflow on a background
//! thread** therefore has no stack left for the handler to run on, and produces
//! no record — the process still dies correctly, we simply learn nothing.
//! `crash-handler` offers a fix (`crash_handler::unix`, which interposes
//! `pthread_create` to give every thread an alternate stack) and it is
//! deliberately NOT used: interposing thread creation across the whole process
//! is a far larger behavioural change than this signal is worth, and every other
//! crash on every other thread is captured without it.

use std::sync::atomic::{AtomicUsize, Ordering};

// `ThreadRole` is deliberately absent: the role is produced by
// `super::thread_role`, never named here. Importing it would be an unused
// import, and CI runs `clippy -D warnings` on exactly this platform.
use super::{FaultAddress, NativeCrash, NativeSignal, Site};

// HANDLER-SAFE-BEGIN
pub(super) fn observe(cc: &crash_context::CrashContext) -> NativeCrash {
    let signal = match cc.siginfo.ssi_signo as i32 {
        libc::SIGSEGV => NativeSignal::Segv,
        libc::SIGBUS => NativeSignal::Bus,
        libc::SIGILL => NativeSignal::Ill,
        libc::SIGFPE => NativeSignal::Fpe,
        libc::SIGABRT => NativeSignal::Abort,
        libc::SIGTRAP => NativeSignal::Trap,
        _ => NativeSignal::Other,
    };

    // A faulting address only means something for the memory faults; for an
    // abort the kernel puts the sending pid there, which is not an address at
    // all and must not be classified as one.
    let fault = match signal {
        NativeSignal::Segv | NativeSignal::Bus => FaultAddress::classify(cc.siginfo.ssi_addr),
        _ => FaultAddress::Unknown,
    };

    let site = match program_counter(cc) {
        Some(pc) => super::site_for_pc_armed(pc),
        None => Site::Unknown,
    };

    NativeCrash {
        signal,
        code: cc.siginfo.ssi_code as i64,
        fault,
        site,
        thread: super::thread_role(
            cc.tid.max(0) as u64,
            super::MAIN_THREAD.load(Ordering::Relaxed),
        ),
    }
}

/// The interrupted program counter, out of the signal's `ucontext`.
fn program_counter(cc: &crash_context::CrashContext) -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    {
        // `REG_RIP` is index 16 in glibc's `gregs` (`sys/ucontext.h`).
        const REG_RIP: usize = 16;
        cc.context.uc_mcontext.gregs.get(REG_RIP).map(|v| *v as u64)
    }

    #[cfg(target_arch = "aarch64")]
    {
        Some(cc.context.uc_mcontext.pc)
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = cc;
        None
    }
}

// HANDLER-SAFE-END

/// This thread's kernel id. Called on the main thread at arm time, and compared
/// against `CrashContext::tid`, which is the same kind of id.
pub(super) fn current_thread_id() -> u64 {
    // SAFETY: `gettid` takes no arguments and cannot fail.
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    if tid > 0 {
        tid as u64
    } else {
        0
    }
}

/// Resolve our own executable's executable-segment range with `dl_iterate_phdr`.
///
/// The first entry the loader reports is the main program; `dlpi_addr` is its
/// load bias (nonzero for a PIE, which every modern build is) and the `PT_LOAD`
/// segments carrying `PF_X` are the code.
pub(super) fn capture_image_range(base: &AtomicUsize, end: &AtomicUsize) {
    struct Range {
        low: usize,
        high: usize,
    }

    unsafe extern "C" fn visit(
        info: *mut libc::dl_phdr_info,
        _size: libc::size_t,
        data: *mut libc::c_void,
    ) -> libc::c_int {
        // SAFETY: the loader hands us a live `dl_phdr_info` and our own `data`
        // pointer; `dlpi_phnum` bounds the header array.
        unsafe {
            let out = &mut *data.cast::<Range>();
            let bias = (*info).dlpi_addr as usize;
            for i in 0..(*info).dlpi_phnum {
                let ph = &*(*info).dlpi_phdr.add(i as usize);
                if ph.p_type != libc::PT_LOAD || ph.p_flags & libc::PF_X == 0 {
                    continue;
                }
                let start = bias.saturating_add(ph.p_vaddr as usize);
                let stop = start.saturating_add(ph.p_memsz as usize);
                if out.high == 0 {
                    out.low = start;
                    out.high = stop;
                } else {
                    out.low = out.low.min(start);
                    out.high = out.high.max(stop);
                }
            }
        }
        // Stop after the first object: that is the main program.
        1
    }

    let mut range = Range { low: 0, high: 0 };
    // SAFETY: `visit` matches the callback ABI and `range` outlives the call.
    unsafe {
        libc::dl_iterate_phdr(Some(visit), (&mut range as *mut Range).cast());
    }
    if range.high > range.low {
        base.store(range.low, Ordering::Relaxed);
        end.store(range.high, Ordering::Relaxed);
    }
}
