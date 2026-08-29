//! macOS half of [`super`] — Mach exception ports.
//!
//! ## The shape of a crash here
//!
//! `crash-handler` claims the task's exception ports and listens on a dedicated
//! thread. When the process faults, the kernel **suspends the faulting thread**
//! and delivers a message to that thread, so our callback runs on an ordinary
//! thread rather than inside a signal handler. That is a materially safer
//! context than Linux's — but the callback still allocates nothing, because the
//! suspended thread may hold the allocator's lock.
//!
//! ## What the message does and does not carry
//!
//! A Mach exception message carries the exception kind, a code and (for
//! `EXC_BAD_ACCESS`) the faulting address. It carries **no register state**, so
//! the program counter has to be fetched with `thread_get_state` against the
//! suspended thread. That is one Mach call on a thread the kernel is holding
//! still for us, and it is the only reason `mach2` is a dependency.
//!
//! `SIGABRT` has no Mach exception at all, so `crash-handler` hooks the signal
//! and re-injects it as `EXC_SOFTWARE` / `EXC_SOFT_SIGNAL` with the signal
//! number in the subcode. An abort therefore carries no faulting address —
//! there is none — and [`FaultAddress::Unknown`] says so rather than reporting
//! the zero that an uninitialised field would have held.
//!
//! ## Notarisation and the hardened runtime
//!
//! Setting your OWN task's exception ports needs no entitlement and is allowed
//! under the hardened runtime — it is what every notarised app shipping Crashpad
//! or Breakpad does. Nothing here affects signing or the notarisation ticket.
//! What it DOES require is that we hand the exception back (`Handled(false)` in
//! [`super::on_crash`]), so `ReportCrash` still writes the OS's own report and
//! the OS still tells the operator the app quit unexpectedly.
//!
//! ## The webview is out of scope, by construction
//!
//! `WKWebView` runs its content in separate `com.apple.WebKit.WebContent`
//! processes. A crash in there never reaches this handler — and should not: it
//! does not kill SundayStage, and the renderer's own `window.onerror` /
//! `unhandledrejection` path already covers what the operator sees.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::{FaultAddress, NativeCrash, NativeSignal, Site, ThreadRole};

// ── Mach exception kinds ────────────────────────────────────────────────────
//
// Taken from `mach2::exception_types` rather than written out here. The first
// draft did write them out, got `EXC_SOFTWARE` wrong by one (it is 5, not 4 —
// `EXC_EMULATION` sits at 4), and every abort on macOS was reported as
// `sig=other`. The test that caught it is
// `a_real_abort_is_captured_as_an_abort`, which aborts a real child process:
// a hand-copied constant is exactly the kind of mistake only a real crash finds.
use mach2::exception_types as et;

/// `EXC_SOFTWARE` code meaning "a Unix signal was re-injected"
/// (`mach/exception_types.h`). Not in `mach2`; `crash-handler` carries its own
/// copy for the same reason.
const EXC_SOFT_SIGNAL: u64 = 0x1_0003;

/// `THREAD_IDENTIFIER_INFO` (`mach/thread_info.h`). Gives the 64-bit thread id
/// that `pthread_threadid_np` reports, which is the only id comparable with one
/// captured on another thread at arm time.
const THREAD_IDENTIFIER_INFO: i32 = 4;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ThreadIdentifierInfo {
    thread_id: u64,
    thread_handle: u64,
    dispatch_qaddr: u64,
}

// ── Mach-O image headers (`mach-o/loader.h`), only the fields we walk ───────
const MH_EXECUTE: u32 = 0x2;
const LC_SEGMENT_64: u32 = 0x19;

#[repr(C)]
struct MachHeader64 {
    magic: u32,
    cputype: i32,
    cpusubtype: i32,
    filetype: u32,
    ncmds: u32,
    sizeofcmds: u32,
    flags: u32,
    reserved: u32,
}

#[repr(C)]
struct LoadCommand {
    cmd: u32,
    cmdsize: u32,
}

#[repr(C)]
struct SegmentCommand64 {
    cmd: u32,
    cmdsize: u32,
    segname: [u8; 16],
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    maxprot: i32,
    initprot: i32,
    nsects: u32,
    flags: u32,
}

unsafe extern "C" {
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_header(image_index: u32) -> *const MachHeader64;

    /// `mach/thread_act.h`. Declared here because `mach2` binds
    /// `thread_get_state` but not `thread_info`; the ABI is fixed and public.
    fn thread_info(
        target_act: mach2::mach_types::thread_t,
        flavor: u32,
        thread_info_out: *mut i32,
        thread_info_out_cnt: *mut u32,
    ) -> mach2::kern_return::kern_return_t;
}

// HANDLER-SAFE-BEGIN
/// Read the crash out of the Mach exception message.
pub(super) fn observe(cc: &crash_context::CrashContext) -> NativeCrash {
    let Some(exc) = cc.exception else {
        return NativeCrash {
            signal: NativeSignal::Other,
            code: 0,
            fault: FaultAddress::Unknown,
            site: Site::Unknown,
            thread: ThreadRole::Unknown,
        };
    };

    let signal = match exc.kind {
        et::EXC_BAD_ACCESS => NativeSignal::Segv,
        et::EXC_BAD_INSTRUCTION => NativeSignal::Ill,
        et::EXC_ARITHMETIC => NativeSignal::Fpe,
        et::EXC_BREAKPOINT => NativeSignal::Trap,
        // A Unix signal re-injected as an exception — `abort()` takes this
        // route, because macOS has no Mach exception for a process aborting and
        // `crash-handler` hooks `SIGABRT` and forwards it. `subcode` is the
        // signal number.
        et::EXC_SOFTWARE if exc.code == EXC_SOFT_SIGNAL => exc
            .subcode
            .map(from_unix_signal)
            .unwrap_or(NativeSignal::Other),
        // A fatal signal delivered as `EXC_CRASH`: xnu packs the POSIX signal
        // into the high byte of the code.
        et::EXC_CRASH => from_unix_signal((exc.code >> 24) & 0xff),
        // A guard violation — a guarded file descriptor closed, a vnode guard
        // tripped. The process is going down either way.
        et::EXC_GUARD => NativeSignal::Abort,
        _ => NativeSignal::Other,
    };

    let fault = match (exc.kind, exc.subcode) {
        (et::EXC_BAD_ACCESS, Some(addr)) => FaultAddress::classify(addr),
        _ => FaultAddress::Unknown,
    };

    let site = match program_counter(cc.thread) {
        Some(pc) => super::site_for_pc_armed(pc),
        None => Site::Unknown,
    };

    let thread = super::thread_role(
        thread_identifier(cc.thread),
        super::MAIN_THREAD.load(Ordering::Relaxed),
    );

    NativeCrash {
        signal,
        code: exc.code as i64,
        fault,
        site,
        thread,
    }
}

/// Map a POSIX signal number onto the closed set. Shared by the two routes a
/// signal can reach us on macOS (`EXC_SOFTWARE`/`EXC_SOFT_SIGNAL` and
/// `EXC_CRASH`), so the two cannot disagree about what `6` means.
fn from_unix_signal(sig: u64) -> NativeSignal {
    match sig as i32 {
        libc::SIGSEGV => NativeSignal::Segv,
        libc::SIGBUS => NativeSignal::Bus,
        libc::SIGILL => NativeSignal::Ill,
        libc::SIGFPE => NativeSignal::Fpe,
        libc::SIGABRT => NativeSignal::Abort,
        libc::SIGTRAP => NativeSignal::Trap,
        _ => NativeSignal::Other,
    }
}

/// The suspended thread's program counter, or `None` if the kernel will not say.
fn program_counter(thread: mach2::mach_types::thread_t) -> Option<u64> {
    if thread == 0 {
        return None;
    }

    #[cfg(target_arch = "aarch64")]
    {
        use mach2::structs::arm_thread_state64_t;
        let mut state = arm_thread_state64_t::new();
        let mut count = arm_thread_state64_t::count();
        // SAFETY: `state`/`count` are a matching buffer and length for the
        // flavour we ask for, and `thread` is the port the kernel handed us for
        // a thread it is holding suspended.
        let kr = unsafe {
            mach2::thread_act::thread_get_state(
                thread,
                mach2::thread_status::ARM_THREAD_STATE64,
                (&mut state as *mut arm_thread_state64_t).cast(),
                &mut count,
            )
        };
        (kr == mach2::kern_return::KERN_SUCCESS).then_some(state.__pc)
    }

    #[cfg(target_arch = "x86_64")]
    {
        use mach2::structs::x86_thread_state64_t;
        let mut state = x86_thread_state64_t::new();
        let mut count = x86_thread_state64_t::count();
        // SAFETY: as above.
        let kr = unsafe {
            mach2::thread_act::thread_get_state(
                thread,
                mach2::thread_status::x86_THREAD_STATE64,
                (&mut state as *mut x86_thread_state64_t).cast(),
                &mut count,
            )
        };
        (kr == mach2::kern_return::KERN_SUCCESS).then_some(state.__rip)
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = thread;
        None
    }
}

/// The suspended thread's 64-bit id, comparable with [`current_thread_id`].
fn thread_identifier(thread: mach2::mach_types::thread_t) -> u64 {
    if thread == 0 {
        return 0;
    }
    let mut info = ThreadIdentifierInfo::default();
    let mut count =
        (std::mem::size_of::<ThreadIdentifierInfo>() / std::mem::size_of::<u32>()) as u32;
    // SAFETY: matching buffer and length for `THREAD_IDENTIFIER_INFO`.
    let kr = unsafe {
        thread_info(
            thread,
            THREAD_IDENTIFIER_INFO as u32,
            (&mut info as *mut ThreadIdentifierInfo).cast(),
            &mut count,
        )
    };
    if kr == mach2::kern_return::KERN_SUCCESS {
        info.thread_id
    } else {
        0
    }
}

// HANDLER-SAFE-END

/// This thread's 64-bit id. Called on the main thread at arm time.
pub(super) fn current_thread_id() -> u64 {
    let mut id: u64 = 0;
    // SAFETY: `pthread_threadid_np(NULL, &out)` reports the CALLING thread.
    let rc = unsafe { libc::pthread_threadid_np(0, &mut id) };
    if rc == 0 {
        id
    } else {
        0
    }
}

/// Resolve our own executable's `__TEXT` range.
///
/// The main executable is the image whose Mach-O `filetype` is `MH_EXECUTE`.
/// Its header address IS its load address, and the `__TEXT` segment's `vmsize`
/// is how much executable image follows — so `[header, header + vmsize)` is the
/// range a program counter in our own code falls inside. Every Rust line we ship
/// is in there: the Tauri app links the whole crate statically.
pub(super) fn capture_image_range(base: &AtomicUsize, end: &AtomicUsize) {
    // SAFETY: the dyld image list is only mutated under dyld's own lock while
    // images load; this runs on the main thread during startup, and every read
    // below stays inside the header's declared `sizeofcmds`.
    unsafe {
        for i in 0.._dyld_image_count() {
            let header = _dyld_get_image_header(i);
            if header.is_null() || (*header).filetype != MH_EXECUTE {
                continue;
            }
            let Some(text_vmsize) = text_segment_vmsize(header) else {
                continue;
            };
            let start = header as usize;
            base.store(start, Ordering::Relaxed);
            end.store(
                start.saturating_add(text_vmsize as usize),
                Ordering::Relaxed,
            );
            return;
        }
    }
}

/// Walk the load commands for `__TEXT`'s `vmsize`.
///
/// # Safety
///
/// `header` must point at a mapped Mach-O header.
unsafe fn text_segment_vmsize(header: *const MachHeader64) -> Option<u64> {
    unsafe {
        let ncmds = (*header).ncmds;
        let mut cursor = header.add(1).cast::<u8>();
        for _ in 0..ncmds {
            let lc = cursor.cast::<LoadCommand>();
            let cmdsize = (*lc).cmdsize as usize;
            if cmdsize == 0 {
                return None;
            }
            if (*lc).cmd == LC_SEGMENT_64 {
                let seg = cursor.cast::<SegmentCommand64>();
                if (*seg).segname.starts_with(b"__TEXT\0") {
                    return Some((*seg).vmsize);
                }
            }
            cursor = cursor.add(cmdsize);
        }
    }
    None
}
