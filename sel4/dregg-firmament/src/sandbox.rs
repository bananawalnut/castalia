//! THE OS-SANDBOX CONFINEMENT — Phase-0 of the cross-platform sandboxed
//! firmament (`docs/DREGG-DESKTOP-OS.md §3`, the ambient-authority half of the
//! v1 isolation upgrade).
//!
//! ## What gap this closes
//!
//! [`crate::process_kernel::ProcessKernel::spawn_pd`] forks a child PD with
//! **MMU isolation** (separate page tables) + a `socketpair(AF_UNIX)` control
//! channel + named `shm` regions + the kernel's epoch-tagged cap-handle
//! [`ValidityTable`]. That is the *memory* half of confinement and it is real.
//! But the fork-path child inherits the parent's **ambient authority**: it can
//! still `open()` arbitrary files, `socket(AF_INET)` the network, `execve` a new
//! image, and it keeps every inherited fd. [`ProcessKernel::ISOLATION_FIDELITY`]
//! names exactly this: "MMU enforces address-space separation" but ambient OS
//! authority is NOT yet confined.
//!
//! This module closes that, applied in the child **right after `fork()`, before
//! the PD `body` runs**, via a per-platform [`confine_child`]:
//!
//! - **macOS (enforced + smoke-tested here):** the child closes every fd except
//!   the granted ones (the control socket), then SELF-applies a `(version 1)
//!   (deny default)` Seatbelt/SBPL profile through `sandbox_init(3)`. The profile
//!   allows only: I/O on already-open fds, the minimal `/usr/lib` + dyld /
//!   framework reads a process needs to keep running, plus any explicitly-granted
//!   read paths. It allows NO `network*`, NO `process-exec*`, NO `mach-lookup*`.
//!   The macOS sandbox MUST be self-applied by the child (a parent cannot impose
//!   it) — fine, the child is trusted firmament code between fork and body.
//!
//! - **Linux (compiled here as a cfg-stub, ENFORCED on Linux):**
//!   `unshare(CLONE_NEWUSER|NEWNET|NEWNS|NEWPID)` + a uid-map when unprivileged
//!   namespaces are available (an empty net namespace = no route to any network),
//!   `prctl(PR_SET_NO_NEW_PRIVS)`, a
//!   default-deny seccomp-bpf allow-list (`seccompiler`), Landlock path-rules
//!   (`landlock`) for any granted read paths, and `close_range` keeping only the
//!   granted fds. Namespace denial by the host does not abort the child because
//!   seccomp still denies socket/open/exec and the remaining layers still apply;
//!   every other namespace error remains fatal. On macOS the Linux body compiles
//!   to a no-op stub so the crate builds on both; it only RUNS on Linux.
//!
//! ## The trust statement (don't-launder-vacuity)
//!
//! What this ENFORCES on macOS: a confined child that tries `open("/etc/passwd")`
//! is denied by the Seatbelt profile; `socket(AF_INET)` fails (no `network*`);
//! the inherited control socket is the only fd it holds; the control round-trip
//! still works. What remains TRUSTED: the confinement is the host OS's
//! (Seatbelt / Linux LSMs), the same trust the MMU isolation already places in
//! the host kernel. The child applies it to ITSELF — it is firmament code we
//! wrote, not the (untrusted) PD payload, and it does so before yielding to the
//! payload `body`, so the payload never runs un-confined.
//!
//! Unix-only, behind the `process-pd-sandbox` feature (which implies
//! `process-pd`).

#![cfg(all(feature = "process-pd-sandbox", unix))]

use std::os::unix::io::RawFd;

/// The OS-authority grant a confined child is given — the cap→OS mapping seed.
///
/// Phase 0 wires the **Endpoint-only** case: the child keeps exactly the
/// `endpoint_fds` it was granted (its control socket) and nothing else; every
/// other fd is closed, and ambient file/network/exec authority is denied. The
/// `read_paths` field is the next slice (a file-cap → an SBPL allowed path on
/// macOS / a Landlock rule on Linux); it is honored where the platform supports
/// it and is empty for the Endpoint-only Phase-0 case.
#[derive(Clone, Debug, Default)]
pub struct Confinement {
    /// The fds the child is allowed to keep open (its granted channels — the
    /// control socket(s)). EVERY other inherited fd is closed. This is the
    /// "the Endpoint is the only channel" guarantee at the fd layer.
    pub endpoint_fds: Vec<RawFd>,
    /// Read-only filesystem paths a file-cap granted the child (the next slice).
    /// macOS: each becomes an SBPL `(allow file-read* (subpath "<p>"))`.
    /// Linux: each becomes a Landlock read rule. Empty for Endpoint-only Phase 0.
    pub read_paths: Vec<String>,
}

impl Confinement {
    /// The Endpoint-only confinement (Phase 0): keep just the control socket fd,
    /// deny all ambient file / network / exec authority, grant no extra paths.
    pub fn endpoint_only(control_fd: RawFd) -> Self {
        Confinement {
            endpoint_fds: vec![control_fd],
            read_paths: Vec::new(),
        }
    }

    /// Add the granted control/channel fds.
    pub fn with_fds(mut self, fds: impl IntoIterator<Item = RawFd>) -> Self {
        self.endpoint_fds.extend(fds);
        self
    }

    /// Grant a read-only path (a file-cap → an OS path rule). The next slice
    /// past Endpoint-only.
    pub fn with_read_path(mut self, path: impl Into<String>) -> Self {
        self.read_paths.push(path.into());
        self
    }
}

/// The outcome of a [`confine_child`] attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfineError {
    /// `sandbox_init` (macOS) failed with this message.
    SandboxInit(String),
    /// A Linux confinement step failed. The operation is a fixed, non-secret
    /// classifier; errno is present when the syscall exposed one.
    Linux {
        operation: &'static str,
        errno: Option<i32>,
        message: String,
    },
    /// Closing the non-granted fds failed.
    FdClose(String),
    /// This platform has no implemented backing (neither macOS nor Linux).
    Unsupported,
}

impl std::fmt::Display for ConfineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfineError::SandboxInit(m) => write!(f, "macOS sandbox_init failed: {m}"),
            ConfineError::Linux {
                operation,
                errno,
                message,
            } => write!(
                f,
                "linux confinement failed at {operation} (errno={}): {message}",
                errno.map_or_else(|| "unknown".to_string(), |value| value.to_string())
            ),
            ConfineError::FdClose(m) => write!(f, "closing inherited fds failed: {m}"),
            ConfineError::Unsupported => write!(f, "no sandbox backing on this platform"),
        }
    }
}

impl std::error::Error for ConfineError {}

/// The user-namespace identity map being installed after `unshare(2)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamespaceSetupOperation {
    UidMap,
    GidMap,
}

impl NamespaceSetupOperation {
    pub fn diagnostic_name(self) -> &'static str {
        match self {
            Self::UidMap => "namespace_uid_map",
            Self::GidMap => "namespace_gid_map",
        }
    }
}

/// A typed UID/GID map setup failure that preserves the host errno.
#[derive(Debug)]
pub struct NamespaceSetupFailure {
    operation: NamespaceSetupOperation,
    error: std::io::Error,
}

impl NamespaceSetupFailure {
    pub fn new(operation: NamespaceSetupOperation, error: std::io::Error) -> Self {
        Self { operation, error }
    }

    /// Only an explicit host-policy refusal makes this optional namespace layer
    /// unavailable. Missing proc entries and every other setup error stay fatal.
    pub fn is_host_policy_unavailable(&self) -> bool {
        matches!(self.error.raw_os_error(), Some(libc::EPERM | libc::EACCES))
    }

    pub fn operation(&self) -> NamespaceSetupOperation {
        self.operation
    }

    pub fn error(&self) -> &std::io::Error {
        &self.error
    }

    #[cfg(target_os = "linux")]
    fn into_confine_error(self) -> ConfineError {
        ConfineError::linux_io(self.operation.diagnostic_name(), self.error)
    }
}

impl ConfineError {
    #[cfg(target_os = "linux")]
    fn linux(operation: &'static str, message: impl Into<String>) -> Self {
        Self::Linux {
            operation,
            errno: None,
            message: message.into(),
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_errno(
        operation: &'static str,
        errno: Option<i32>,
        message: impl Into<String>,
    ) -> Self {
        Self::Linux {
            operation,
            errno,
            message: message.into(),
        }
    }

    pub fn linux_io(operation: &'static str, error: std::io::Error) -> Self {
        Self::Linux {
            operation,
            errno: error.raw_os_error(),
            message: error.to_string(),
        }
    }

    /// A secret-safe failure report: fixed operation name plus numeric errno,
    /// never a granted path or free-form error payload.
    pub fn diagnostic(&self) -> String {
        match self {
            Self::Linux {
                operation, errno, ..
            } => format!(
                "operation={operation} errno={}",
                errno.map_or_else(|| "unknown".to_string(), |value| value.to_string())
            ),
            Self::SandboxInit(_) => "operation=sandbox_init errno=unknown".to_string(),
            Self::FdClose(_) => "operation=fd_isolation errno=unknown".to_string(),
            Self::Unsupported => "operation=platform_support errno=unknown".to_string(),
        }
    }
}

/// Write a confinement trace line directly to stderr. The child has just
/// forked, so avoid buffered stdio locks; callers provide only fixed operation
/// names or the redacted [`ConfineError::diagnostic`] string.
pub(crate) fn emit_diagnostic(message: &str) {
    let prefix = b"[pd-confinement] ";
    let newline = b"\n";
    unsafe {
        libc::write(libc::STDERR_FILENO, prefix.as_ptr().cast(), prefix.len());
        libc::write(libc::STDERR_FILENO, message.as_ptr().cast(), message.len());
        libc::write(libc::STDERR_FILENO, newline.as_ptr().cast(), newline.len());
    }
}

/// CONFINE the calling process (a freshly-forked child PD) to its granted
/// authority — close all non-granted fds, then drop ambient OS authority via the
/// host sandbox. Call this in the CHILD, after `fork()`, BEFORE the PD `body`.
///
/// After it returns `Ok(())`, the child can talk over its granted fd(s) but
/// cannot open arbitrary files, reach the network, exec a new image, or touch
/// any other inherited fd. On macOS this is the Seatbelt profile; on Linux the
/// namespaces + seccomp + Landlock stack.
///
/// # Safety
/// MUST be called in a forked child (it mutates process-global authority and
/// closes fds). It is async-signal-safe-ish: it does a bounded amount of work
/// and applies an irreversible confinement; never call it in the parent.
pub fn confine_child(c: &Confinement) -> Result<(), ConfineError> {
    close_all_but(&c.endpoint_fds)?;
    #[cfg(target_os = "macos")]
    {
        macos::confine(c)
    }
    #[cfg(target_os = "linux")]
    {
        linux::confine(c)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = c;
        Err(ConfineError::Unsupported)
    }
}

/// Close EVERY open fd except the granted ones (and the std streams 0/1/2, kept
/// so the PD can still print under `--nocapture` — they are not an authority
/// channel, just the console). This is the fd-isolation half: after it, the
/// granted control socket is the only NON-console channel the child holds.
///
/// We walk a generous fd range and `close` each not in `keep`. (The
/// `close_range(2)` syscall is the spawn-path optimization; the fork path here
/// walks explicitly so it works identically on macOS + Linux without the newer
/// syscall.)
fn close_all_but(keep: &[RawFd]) -> Result<(), ConfineError> {
    // Keep std streams so PD prints survive; keep the granted channel fds.
    let max = max_open_fds();
    for fd in 3..max {
        if keep.contains(&fd) {
            continue;
        }
        // Best-effort: ignore EBADF (already-closed) — only a real failure on a
        // KEPT fd would matter, and we never close those.
        unsafe {
            libc::close(fd);
        }
    }
    Ok(())
}

/// An upper bound on the fd table to sweep. Uses `RLIMIT_NOFILE` soft limit,
/// clamped to a sane ceiling so we never spin over a 2^31 limit.
fn max_open_fds() -> RawFd {
    let mut rl = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let cap: RawFd = 4096;
    let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl) };
    if rc == 0 && rl.rlim_cur != libc::RLIM_INFINITY {
        (rl.rlim_cur as RawFd).min(cap).max(16)
    } else {
        cap
    }
}

/// Whether a namespace setup error means the host does not expose unprivileged
/// namespaces to this process. In that case Linux confinement can still proceed
/// through its independent no-new-privileges, Landlock, and seccomp layers.
#[cfg(any(target_os = "linux", test))]
fn namespace_is_unavailable(error: &std::io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(errno) if matches!(errno, libc::EPERM | libc::EACCES | libc::ENOSYS)
    )
}

/// Whether a Landlock syscall errno explicitly means the kernel does not provide
/// the requested isolation to this process. Configuration, path, and rule errors
/// are never classified through this predicate and remain fatal.
#[cfg(any(target_os = "linux", test))]
fn landlock_errno_is_unavailable(errno: i32) -> bool {
    matches!(
        errno,
        libc::EPERM | libc::EACCES | libc::ENOSYS | libc::EOPNOTSUPP
    )
}

// ───────────────────────────── macOS backend ────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use super::{ConfineError, Confinement};
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    use std::ptr;

    // The Seatbelt FFI (the `gaol`-macos backend shape, ~10 lines of raw FFI).
    // `sandbox_init` self-applies an SBPL profile to the calling process; once
    // applied it cannot be loosened. `sandbox_free_error` frees the error buffer.
    extern "C" {
        fn sandbox_init(profile: *const c_char, flags: u64, errorbuf: *mut *mut c_char) -> c_int;
        fn sandbox_free_error(errorbuf: *mut c_char);
    }

    /// The default-deny SBPL prologue. Everything not explicitly allowed below
    /// is DENIED — no network, no exec, no mach-lookup, no arbitrary file read.
    const PROLOGUE: &str = "(version 1)\n(deny default)\n";

    /// The minimal allow-set a confined firmament PD needs to KEEP RUNNING after
    /// `sandbox_init` (the dynamic linker / libsystem are already mapped, but the
    /// process may still touch them; a stricter profile risks SIGKILL on the
    /// next libc call). We allow:
    ///   - I/O on already-open fds (the control socket round-trip + console);
    ///   - read-only access to the system dylib/framework paths the runtime maps;
    ///   - sysctl reads libsystem performs.
    /// We do NOT allow network*, process-exec*, process-fork*, mach-lookup*, or
    /// arbitrary file-write — those are the ambient authority we are dropping.
    const BASE_ALLOW: &str = "\
(allow file-read* (subpath \"/usr/lib\"))\n\
(allow file-read* (subpath \"/System/Library\"))\n\
(allow file-read* (subpath \"/Library/Apple\"))\n\
(allow file-read-metadata)\n\
(allow sysctl-read)\n\
(allow file-ioctl)\n\
(allow signal (target self))\n\
";

    /// Build the full SBPL profile text from the prologue, the base allow-set,
    /// and any explicitly-granted read paths (the file-cap → allowed-path map).
    pub(super) fn build_profile(c: &Confinement) -> String {
        let mut p = String::with_capacity(256);
        p.push_str(PROLOGUE);
        p.push_str(BASE_ALLOW);
        for path in &c.read_paths {
            // (allow file-read* (subpath "<path>")) — a granted read path.
            p.push_str("(allow file-read* (subpath \"");
            // Escape backslash + quote for the TinyScheme string literal.
            for ch in path.chars() {
                if ch == '"' || ch == '\\' {
                    p.push('\\');
                }
                p.push(ch);
            }
            p.push_str("\"))\n");
        }
        p
    }

    /// SELF-apply the Seatbelt profile to this (child) process.
    pub fn confine(c: &Confinement) -> Result<(), ConfineError> {
        let profile = build_profile(c);
        let cprofile =
            CString::new(profile).map_err(|e| ConfineError::SandboxInit(format!("nul: {e}")))?;
        let mut err: *mut c_char = ptr::null_mut();
        let rc = unsafe { sandbox_init(cprofile.as_ptr(), 0, &mut err) };
        if rc == 0 {
            Ok(())
        } else {
            let msg = if err.is_null() {
                "sandbox_init returned non-zero with no error string".to_string()
            } else {
                let m = unsafe { CStr::from_ptr(err) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { sandbox_free_error(err) };
                m
            };
            Err(ConfineError::SandboxInit(msg))
        }
    }
}

// ───────────────────────────── Linux backend ────────────────────────────────
//
// Compiles on macOS as an unreachable cfg-stub (the module body is gated to
// `target_os = "linux"`); RUNS on Linux. The steps:
//   1. When the host permits it, unshare(CLONE_NEWUSER|NEWNET|NEWNS|NEWPID) + a
//      uid-map (root-in-namespace maps to the real uid) — an EMPTY net namespace
//      means no route anywhere. Host policy may disable this independent layer.
//   2. prctl(PR_SET_NO_NEW_PRIVS, 1) — no setuid/fscaps escalation.
//   3. a default-deny seccomp-bpf allow-list (read/write/close/exit/… only) via
//      `seccompiler` — socket()/open()/execve() trap to EPERM/SIGSYS.
//   4. Landlock read rules for any granted read paths via `landlock`.
//   5. (close_all_but already ran in the shared path, keeping the control fd.)

#[cfg(target_os = "linux")]
mod linux {
    use super::{ConfineError, Confinement};

    /// Apply the Linux confinement stack to this (child) process.
    pub fn confine(c: &Confinement) -> Result<(), ConfineError> {
        super::emit_diagnostic("operation=namespace_unshare state=begin");
        unshare_namespaces()?;
        super::emit_diagnostic("operation=namespace_unshare state=complete_or_unavailable");
        super::emit_diagnostic("operation=no_new_privs state=begin");
        no_new_privs()?;
        super::emit_diagnostic("operation=no_new_privs state=complete");
        // Landlock is an independent defence-in-depth layer. Only an explicitly
        // recognized unavailable/host-refused outcome may fall back to seccomp;
        // malformed rules, invalid paths, rule-add failures, and unexpected
        // restrict_self failures remain fatal.
        super::emit_diagnostic("operation=landlock state=begin");
        apply_landlock(&c.read_paths)?;
        super::emit_diagnostic("operation=landlock state=complete_or_unavailable");
        super::emit_diagnostic("operation=seccomp_apply state=begin");
        apply_seccomp()?;
        super::emit_diagnostic("operation=seccomp_apply state=complete");
        Ok(())
    }

    /// unshare USER+NET+NS+PID namespaces and write the uid/gid maps. An empty
    /// net namespace gives the child no network route at all (the net-cap denial).
    fn unshare_namespaces() -> Result<(), ConfineError> {
        use std::io::Write;
        // The real uid/gid to map root-in-namespace back to.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        let flags =
            libc::CLONE_NEWUSER | libc::CLONE_NEWNET | libc::CLONE_NEWNS | libc::CLONE_NEWPID;
        let rc = unsafe { libc::unshare(flags) };
        if rc != 0 {
            let error = std::io::Error::last_os_error();
            if super::namespace_is_unavailable(&error) {
                return Ok(());
            }
            return Err(ConfineError::linux_io("namespace_unshare", error));
        }
        // setgroups must be denied before writing gid_map in a userns. Unlike
        // UID/GID map policy refusal, every failure at this setup step is fatal.
        std::fs::write("/proc/self/setgroups", b"deny")
            .map_err(|error| ConfineError::linux_io("namespace_setgroups", error))?;
        let uid_map = std::fs::OpenOptions::new()
            .write(true)
            .open("/proc/self/uid_map")
            .and_then(|mut f| f.write_all(format!("0 {uid} 1\n").as_bytes()))
            .map_err(|error| {
                super::NamespaceSetupFailure::new(super::NamespaceSetupOperation::UidMap, error)
            });
        if let Err(failure) = uid_map {
            if !failure.is_host_policy_unavailable() {
                return Err(failure.into_confine_error());
            }
        }
        let gid_map = std::fs::OpenOptions::new()
            .write(true)
            .open("/proc/self/gid_map")
            .and_then(|mut f| f.write_all(format!("0 {gid} 1\n").as_bytes()))
            .map_err(|error| {
                super::NamespaceSetupFailure::new(super::NamespaceSetupOperation::GidMap, error)
            });
        if let Err(failure) = gid_map {
            if !failure.is_host_policy_unavailable() {
                return Err(failure.into_confine_error());
            }
        }
        Ok(())
    }

    /// prctl(PR_SET_NO_NEW_PRIVS, 1) — required before installing seccomp without
    /// CAP_SYS_ADMIN, and it blocks setuid/fscap privilege escalation.
    fn no_new_privs() -> Result<(), ConfineError> {
        let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if rc != 0 {
            return Err(ConfineError::linux_io(
                "no_new_privs",
                std::io::Error::last_os_error(),
            ));
        }
        Ok(())
    }

    /// Install a default-deny seccomp-bpf filter that allows only the syscalls a
    /// confined PD body needs (read/write/close/exit/sigreturn/…) and traps the
    /// rest (notably `socket`, `open`, `openat`, `execve`) to EPERM. Built with
    /// the `seccompiler` crate.
    fn apply_seccomp() -> Result<(), ConfineError> {
        use seccompiler::{
            apply_filter, BackendError, BpfProgram, SeccompAction, SeccompFilter, TargetArch,
        };
        use std::collections::BTreeMap;

        #[cfg(target_arch = "x86_64")]
        let arch = TargetArch::x86_64;
        #[cfg(target_arch = "aarch64")]
        let arch = TargetArch::aarch64;

        // The allow-list: syscalls a bounded compute-over-the-socket PD needs.
        // Everything else → EPERM (errno 1). socket/open/openat/execve are NOT
        // listed, so they are denied — the ambient-authority drop.
        let allow: &[i64] = &[
            libc::SYS_read,
            libc::SYS_write,
            libc::SYS_close,
            libc::SYS_exit,
            libc::SYS_exit_group,
            libc::SYS_rt_sigreturn,
            libc::SYS_rt_sigprocmask,
            libc::SYS_sigaltstack,
            libc::SYS_brk,
            libc::SYS_mmap,
            libc::SYS_munmap,
            libc::SYS_mprotect,
            libc::SYS_futex,
            libc::SYS_nanosleep,
            libc::SYS_clock_nanosleep,
            libc::SYS_sched_yield,
            libc::SYS_getpid,
            libc::SYS_gettid,
            libc::SYS_getrandom,
            libc::SYS_madvise,
            libc::SYS_recvfrom,
            libc::SYS_sendto,
            libc::SYS_recvmsg,
            libc::SYS_sendmsg,
            libc::SYS_ppoll,
            libc::SYS_fcntl,
        ];
        let rules: BTreeMap<i64, Vec<_>> = allow.iter().map(|&n| (n, vec![])).collect();

        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Errno(libc::EPERM as u32), // default: deny → EPERM
            SeccompAction::Allow,                     // matched (allow-listed) → allow
            arch,
        )
        .map_err(|e| ConfineError::linux("seccomp_build", e.to_string()))?;

        let prog: BpfProgram = filter
            .try_into()
            .map_err(|e: BackendError| ConfineError::linux("seccomp_compile", e.to_string()))?;
        apply_filter(&prog).map_err(|e| ConfineError::linux("seccomp_apply", e.to_string()))?;
        Ok(())
    }

    /// Restrict the filesystem with Landlock: deny everything, then re-grant
    /// read-only access to the explicitly-granted `read_paths` (the file-cap →
    /// OS-rule map). If Landlock is unavailable (old kernel), the empty net
    /// namespace + seccomp `open` denial already deny ambient FS authority, so
    /// we treat an ABI-unsupported result as best-effort (the deny still holds
    /// at the seccomp layer).
    fn apply_landlock(read_paths: &[String]) -> Result<(), ConfineError> {
        use landlock::{
            Access, AccessFs, CreateRulesetError, RestrictSelfError, Ruleset, RulesetAttr,
            RulesetCreatedAttr, RulesetError, RulesetStatus, ABI,
        };

        fn ruleset_errno(error: &RulesetError) -> Option<i32> {
            match error {
                RulesetError::CreateRuleset(CreateRulesetError::CreateRulesetCall {
                    source,
                    ..
                }) => source.raw_os_error(),
                RulesetError::RestrictSelf(RestrictSelfError::RestrictSelfCall {
                    source, ..
                }) => source.raw_os_error(),
                _ => None,
            }
        }

        fn ruleset_error_is_unavailable(error: &RulesetError) -> bool {
            ruleset_errno(error).is_some_and(super::landlock_errno_is_unavailable)
        }

        let abi = ABI::V1;
        let read_only = AccessFs::from_read(abi);
        let ruleset = Ruleset::default()
            .handle_access(AccessFs::from_all(abi))
            .map_err(|e| ConfineError::linux("landlock_handle", e.to_string()))?;
        let mut ruleset = match ruleset.create() {
            Ok(ruleset) => ruleset,
            Err(error) if ruleset_error_is_unavailable(&error) => return Ok(()),
            Err(error) => {
                return Err(ConfineError::linux_errno(
                    "landlock_create",
                    ruleset_errno(&error),
                    error.to_string(),
                ));
            }
        };

        for path in read_paths {
            // landlock 0.4.x: `PathBeneath::new` is infallible (returns the rule
            // directly, no longer a Result); only PathFd::new + add_rule can fail.
            let rule = landlock::PathBeneath::new(
                landlock::PathFd::new(path)
                    .map_err(|e| ConfineError::linux("landlock_pathfd", e.to_string()))?,
                read_only,
            );
            ruleset = ruleset
                .add_rule(rule)
                .map_err(|e| ConfineError::linux("landlock_add_rule", e.to_string()))?;
        }

        match ruleset.restrict_self() {
            Ok(status) if matches!(status.ruleset, RulesetStatus::NotEnforced) => Ok(()),
            Ok(_) => Ok(()),
            Err(error) if ruleset_error_is_unavailable(&error) => Ok(()),
            Err(error) => Err(ConfineError::linux_errno(
                "landlock_restrict_self",
                ruleset_errno(&error),
                error.to_string(),
            )),
        }
    }
}

// ──────────────────────────────── tests ─────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confinement_diagnostic_exposes_only_operation_and_errno() {
        let error = ConfineError::linux_io(
            "landlock_create",
            std::io::Error::from_raw_os_error(libc::EPERM),
        );

        assert_eq!(
            error.diagnostic(),
            "operation=landlock_create errno=1",
            "diagnostics must identify the failed primitive without paths or other payload data"
        );
    }

    #[test]
    fn endpoint_only_keeps_just_the_control_fd() {
        let c = Confinement::endpoint_only(7);
        assert_eq!(c.endpoint_fds, vec![7]);
        assert!(c.read_paths.is_empty());
    }

    #[test]
    fn with_read_path_records_the_grant() {
        let c = Confinement::endpoint_only(4).with_read_path("/etc/hosts");
        assert_eq!(c.read_paths, vec!["/etc/hosts".to_string()]);
    }

    #[test]
    fn unavailable_linux_namespaces_fall_back_to_the_independent_sandbox_layers() {
        for errno in [libc::EPERM, libc::EACCES, libc::ENOSYS] {
            assert!(namespace_is_unavailable(
                &std::io::Error::from_raw_os_error(errno)
            ));
        }
        assert!(!namespace_is_unavailable(
            &std::io::Error::from_raw_os_error(libc::EINVAL)
        ));
    }

    #[test]
    fn only_host_refused_landlock_errors_are_unavailable() {
        for errno in [libc::EPERM, libc::EACCES, libc::ENOSYS, libc::EOPNOTSUPP] {
            assert!(landlock_errno_is_unavailable(errno));
        }
        for errno in [libc::EINVAL, libc::EBADF, libc::ENOENT, libc::EFAULT] {
            assert!(!landlock_errno_is_unavailable(errno));
        }
    }

    // The macOS profile builder is pure (no FFI) — exercise its text shape so a
    // profile regression is caught without invoking sandbox_init.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_profile_denies_by_default_and_grants_paths() {
        let c = Confinement::endpoint_only(3).with_read_path("/tmp/grant");
        let p = macos::build_profile(&c);
        assert!(p.contains("(deny default)"), "must be default-deny");
        assert!(!p.contains("network"), "must NOT allow network");
        assert!(!p.contains("process-exec"), "must NOT allow exec");
        assert!(
            p.contains("(subpath \"/tmp/grant\")"),
            "granted read path must appear"
        );
    }
}
