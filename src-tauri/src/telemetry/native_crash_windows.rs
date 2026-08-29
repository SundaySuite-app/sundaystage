//! Windows half of [`super`] — the unhandled-exception filter.
//!
//! `crash-handler` installs a `SetUnhandledExceptionFilter` chain plus the CRT
//! hooks (`abort`, invalid parameter, pure-call) that otherwise terminate a
//! process without ever reaching a filter. On `Handled(false)` it falls through
//! to whatever filter was registered before it, so Windows Error Reporting still
//! gets the crash and the process still exits with the exception code.
//!
//! An SEH filter is not a signal handler — there is no async-signal-safe list to
//! honour — but the heap may be corrupt (that is one of the things being
//! reported), so [`super::on_crash`] allocates nothing here either.
//!
//! **This platform is compile-verified but not behaviour-tested by us.** CI
//! builds Windows; it does not run `cargo test` there, and the real-crash
//! integration test in [`super::tests`] therefore only proves the Unix paths.
//! The Windows-specific surface is deliberately small for that reason: three
//! reads out of structures the OS hands us, and one module-information call.
//!
//! The webview is out of scope here for the same reason as on macOS: WebView2
//! renders in separate `msedgewebview2.exe` processes, so a crash in page
//! content never reaches this filter.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::{FaultAddress, NativeCrash, NativeSignal, Site};

// ── NTSTATUS values (`winnt.h`) ─────────────────────────────────────────────
const EXCEPTION_ACCESS_VIOLATION: i32 = 0xC000_0005u32 as i32;
const EXCEPTION_IN_PAGE_ERROR: i32 = 0xC000_0006u32 as i32;
const EXCEPTION_DATATYPE_MISALIGNMENT: i32 = 0x8000_0002u32 as i32;
const EXCEPTION_ILLEGAL_INSTRUCTION: i32 = 0xC000_001Du32 as i32;
const EXCEPTION_PRIV_INSTRUCTION: i32 = 0xC000_0096u32 as i32;
const EXCEPTION_INT_DIVIDE_BY_ZERO: i32 = 0xC000_0094u32 as i32;
const EXCEPTION_FLT_DIVIDE_BY_ZERO: i32 = 0xC000_008Eu32 as i32;
const EXCEPTION_STACK_OVERFLOW: i32 = 0xC000_00FDu32 as i32;
const EXCEPTION_BREAKPOINT: i32 = 0x8000_0003u32 as i32;
const EXCEPTION_HEAP_CORRUPTION: i32 = 0xC000_0374u32 as i32;
const EXCEPTION_FAIL_FAST: i32 = 0xC000_0409u32 as i32;

// HANDLER-SAFE-BEGIN
pub(super) fn observe(cc: &crash_context::CrashContext) -> NativeCrash {
    let signal = match cc.exception_code {
        EXCEPTION_ACCESS_VIOLATION | EXCEPTION_IN_PAGE_ERROR => NativeSignal::Segv,
        EXCEPTION_DATATYPE_MISALIGNMENT => NativeSignal::Bus,
        EXCEPTION_ILLEGAL_INSTRUCTION | EXCEPTION_PRIV_INSTRUCTION => NativeSignal::Ill,
        EXCEPTION_INT_DIVIDE_BY_ZERO | EXCEPTION_FLT_DIVIDE_BY_ZERO => NativeSignal::Fpe,
        EXCEPTION_STACK_OVERFLOW => NativeSignal::StackOverflow,
        EXCEPTION_BREAKPOINT => NativeSignal::Trap,
        EXCEPTION_HEAP_CORRUPTION | EXCEPTION_FAIL_FAST => NativeSignal::Abort,
        _ => NativeSignal::Other,
    };

    let (fault, pc) = registers(cc);

    NativeCrash {
        signal,
        // The `NTSTATUS` as an integer. Sign-extended by the OS; widened here
        // without reinterpretation so the number in the aggregate is the number
        // in Microsoft's own table.
        code: cc.exception_code as i64,
        fault,
        site: match pc {
            Some(pc) => super::site_for_pc_armed(pc),
            None => Site::Unknown,
        },
        thread: super::thread_role(
            u64::from(cc.thread_id),
            super::MAIN_THREAD.load(Ordering::Relaxed),
        ),
    }
}

/// Read the faulting address class and the program counter out of the
/// `EXCEPTION_POINTERS` the OS handed the filter.
fn registers(cc: &crash_context::CrashContext) -> (FaultAddress, Option<u64>) {
    if cc.exception_pointers.is_null() {
        return (FaultAddress::Unknown, None);
    }
    // SAFETY: the OS guarantees `exception_pointers` and the two records it
    // points at are live for the duration of the filter, and both structures are
    // `#[repr(C)]` mirrors of the documented layout.
    unsafe {
        let ep = &*cc.exception_pointers;

        let fault = if ep.ExceptionRecord.is_null() {
            FaultAddress::Unknown
        } else {
            let rec = &*ep.ExceptionRecord;
            // For an access violation `ExceptionInformation[1]` is the address
            // that was touched; nothing else defines that slot.
            let is_access = rec.ExceptionCode == EXCEPTION_ACCESS_VIOLATION
                || rec.ExceptionCode == EXCEPTION_IN_PAGE_ERROR;
            if is_access && rec.NumberParameters >= 2 {
                FaultAddress::classify(rec.ExceptionInformation[1] as u64)
            } else {
                FaultAddress::Unknown
            }
        };

        let pc = if ep.ContextRecord.is_null() {
            None
        } else {
            let ctx = &*ep.ContextRecord;
            #[cfg(target_arch = "x86_64")]
            {
                Some(ctx.Rip)
            }
            #[cfg(target_arch = "aarch64")]
            {
                Some(ctx.Pc)
            }
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            {
                let _ = ctx;
                None
            }
        };

        (fault, pc)
    }
}

// HANDLER-SAFE-END

/// This thread's id. Called on the main thread at arm time, and compared against
/// `CrashContext::thread_id`, which is the same kind of id.
pub(super) fn current_thread_id() -> u64 {
    // SAFETY: no arguments, cannot fail.
    u64::from(unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() })
}

/// Resolve our own executable's mapped range.
///
/// `GetModuleHandleW(NULL)` is the process's own image base, and `SizeOfImage`
/// is how much of the address space it occupies. Slightly coarser than macOS's
/// `__TEXT` walk — it includes the data sections — which can only ever make a
/// program counter read as [`Site::App`] when a finer range would have said
/// [`Site::Foreign`]. Since instructions do not execute out of the data
/// sections, that difference does not arise in practice, and the coarse range
/// never invents an offset for code that is not ours.
pub(super) fn capture_image_range(base: &AtomicUsize, end: &AtomicUsize) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: a null module name asks for the process's own image; the
    // `MODULEINFO` buffer and its size match what the call expects.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        if module.is_null() {
            return;
        }
        let mut info: MODULEINFO = std::mem::zeroed();
        let ok = GetModuleInformation(
            GetCurrentProcess(),
            module,
            &mut info,
            std::mem::size_of::<MODULEINFO>() as u32,
        );
        if ok == 0 || info.lpBaseOfDll.is_null() || info.SizeOfImage == 0 {
            return;
        }
        let start = info.lpBaseOfDll as usize;
        base.store(start, Ordering::Relaxed);
        end.store(
            start.saturating_add(info.SizeOfImage as usize),
            Ordering::Relaxed,
        );
    }
}
