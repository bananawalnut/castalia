//! THE CONFINED-LAUNCH ACCEPTANCE TEST — Hermes (a stand-in ACP agent) running
//! inside an OS-sandboxed firmament host-PD, reachable ONLY over its Endpoint.
//!
//! This proves the two things Phase-0 of the confined-agent must prove
//! (`deos-hermes/src/confined.rs`):
//!
//!   1. **ACP round-trips over the firmament Endpoint.** The deos
//!      [`AcpClient`](deos_hermes::AcpClient) — UNCHANGED — drives a full
//!      `initialize` → `session/new` → `session/prompt` session against the
//!      confined child over ndjson on the Endpoint, answering each
//!      `session/request_permission` through the proven
//!      [`HermesGateway`](deos_hermes::HermesGateway) (a cap-gated, metered,
//!      receipted dregg turn or an in-band refusal). The session completes with
//!      real verdicts.
//!
//!   2. **The child is OS-confined.** The stand-in agent ALSO runs the four
//!      sandbox probes inside the PD and folds the verdict into its exit code:
//!      `open(/etc/passwd)` denied, `socket(AF_INET)` denied, exactly one non-std
//!      fd open (the Endpoint), and the ACP round-trip worked. All four set =>
//!      [`probe::ALL`](deos_hermes::confined::probe::ALL).
//!
//! HONEST SCOPE: the agent body is a Rust STAND-IN, not a live `hermes acp`
//! subprocess — the confined child has NO exec authority (the sandbox's whole
//! point), so a confined agent must BE a Rust ACP peer, and the live `hermes acp`
//! venv is broken here anyway (`ModuleNotFoundError: No module named 'acp'`). The
//! CONFINEMENT and the ACP wire are real; what is stood-in is the agent's brain.
//!
//! Unix only (the sandbox + fork path). The macOS Seatbelt backend is enforced +
//! run here; the Linux ns+seccomp+landlock backend runs on Linux.

#![cfg(unix)]

use std::sync::{Arc, RwLock};

#[cfg(target_os = "linux")]
use deos_hermes::confined::launch_confined_with_egress;
use deos_hermes::confined::{probe, spawn_hermes_in_pd};
#[cfg(target_os = "linux")]
use deos_hermes::egress::EgressPolicy;
use deos_hermes::{AcpClient, GrantRegistry, HermesGateway, ScriptedCall};
#[cfg(target_os = "linux")]
use dregg_firmament::process_kernel::CONFINE_FAILED_EXIT;
use dregg_firmament::process_kernel::ProcessKernel;
use dregg_firmament::{ConfineError, NamespaceSetupFailure, NamespaceSetupOperation};
use dregg_sdk::{AgentCipherclerk, AgentRuntime};

#[test]
fn confinement_diagnostic_exposes_only_operation_and_errno() {
    let error = ConfineError::linux_io(
        "landlock_create",
        std::io::Error::from_raw_os_error(libc::EPERM),
    );

    assert_eq!(error.diagnostic(), "operation=landlock_create errno=1");
}

#[test]
fn host_refused_user_namespace_id_maps_are_optional_layer_unavailability() {
    for operation in [
        NamespaceSetupOperation::UidMap,
        NamespaceSetupOperation::GidMap,
    ] {
        for errno in [libc::EPERM, libc::EACCES] {
            let refused =
                NamespaceSetupFailure::new(operation, std::io::Error::from_raw_os_error(errno));
            assert!(refused.is_host_policy_unavailable());
        }
    }

    let missing_proc_entry = NamespaceSetupFailure::new(
        NamespaceSetupOperation::UidMap,
        std::io::Error::from_raw_os_error(libc::ENOENT),
    );
    assert!(
        !missing_proc_entry.is_host_policy_unavailable(),
        "unexpected namespace setup errors must remain fatal"
    );
    for errno in [libc::EINVAL, libc::ENOSYS] {
        let unexpected = NamespaceSetupFailure::new(
            NamespaceSetupOperation::GidMap,
            std::io::Error::from_raw_os_error(errno),
        );
        assert!(!unexpected.is_host_policy_unavailable());
    }
}

#[test]
fn confined_hermes_round_trips_acp_over_the_endpoint_and_is_sandboxed() {
    // The grantor: deos holds the root token; the registry confines the session
    // (per-kind floors + the curated per-tool tightenings).
    let mut cclerk = AgentCipherclerk::new();
    let root = cclerk.mint_token(&[7u8; 32], "deos");
    let rt = AgentRuntime::new(Arc::new(RwLock::new(cclerk)), "deos");
    let registry = GrantRegistry::default_for_session(1000).with_standard_tool_grants(1000);
    let gateway = HermesGateway::new(&rt, root, registry);

    // The scripted turn the confined stand-in agent plays — a search, a write,
    // and a terminal call (each a permission request the gateway decides).
    let script = vec![
        ScriptedCall::new("web_search", serde_json::json!({"query": "dregg ocap"})),
        ScriptedCall::new(
            "write_file",
            serde_json::json!({"path": "notes/plan.md", "content": "the plan"}),
        ),
        ScriptedCall::new("terminal", serde_json::json!({"command": "cargo build"})),
    ];

    // LAUNCH the agent INTO a confined host-PD — the `spawn_hermes_in_pd` seam.
    // No cwd-cap / net-cap in Phase-0 (Endpoint-only confinement).
    let kernel = ProcessKernel::new();
    let agent =
        spawn_hermes_in_pd(&kernel, "sess-confined", script, None, None).expect("fork confined PD");

    // The parent-side ACP transport over the confined child's Endpoint. The
    // UNCHANGED client drives the confined agent through it.
    let transport = agent.transport().expect("Endpoint transport");
    let mut client = AcpClient::new(transport, gateway, 10);

    // (1) DRIVE the ACP session end-to-end over the Endpoint.
    let run = match client.run_prompt("/sandboxed/cwd", "do the confined turn") {
        Ok(run) => run,
        Err(error) => {
            let verdict = agent
                .join_verdict()
                .expect("reap confined child after ACP transport failure");
            panic!(
                "the ACP loop runs end-to-end over the firmament Endpoint: {error}; \
                 confined child exit/verdict={verdict:#x}"
            );
        }
    };

    // The session round-tripped: agent text streamed, three permission verdicts.
    assert!(
        run.agent_text.contains("confined"),
        "the confined agent's streamed text came back over the Endpoint: {:?}",
        run.agent_text
    );
    assert_eq!(
        run.verdicts.len(),
        3,
        "the gateway decided all three scripted tool-calls over the Endpoint"
    );
    // web_search + write_file are allowed under the standard registry; assert at
    // least one is a real receipted ALLOW (proves the metered turn committed).
    let allows = run.verdicts.iter().filter(|(_, o)| o.allowed()).count();
    assert!(
        allows >= 1,
        "at least one tool-call committed a receipted turn through the gate: {:?}",
        run.verdicts
    );

    // (2) The child exits with its sandbox-probe verdict. Reap it and assert all
    //     four confinement teeth held.
    let verdict = agent.join_verdict().expect("reap confined child");
    assert_eq!(
        verdict & probe::OPEN_DENIED,
        probe::OPEN_DENIED,
        "open(/etc/passwd) must be DENIED inside the confined PD (verdict={verdict:#x})"
    );
    assert_eq!(
        verdict & probe::NET_DENIED,
        probe::NET_DENIED,
        "socket(AF_INET) must be DENIED inside the confined PD (verdict={verdict:#x})"
    );
    assert_eq!(
        verdict & probe::ONLY_ENDPOINT_FD,
        probe::ONLY_ENDPOINT_FD,
        "the firmament Endpoint must be the ONLY non-std fd (verdict={verdict:#x})"
    );
    assert_eq!(
        verdict & probe::IPC_WORKS,
        probe::IPC_WORKS,
        "the ACP round-trip over the Endpoint must have completed (verdict={verdict:#x})"
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        verdict & probe::NO_NEW_PRIVS,
        probe::NO_NEW_PRIVS,
        "PR_SET_NO_NEW_PRIVS must remain mandatory when user namespaces are unavailable \
         (verdict={verdict:#x})"
    );
    assert_eq!(
        verdict,
        probe::ALL,
        "CONFINED-LAUNCH TOOTH: the agent ran ACP over the Endpoint AND was OS-confined \
         (file/network/exec ambient authority denied, one fd held). verdict={verdict:#x}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn malformed_landlock_grant_fails_closed_before_the_child_body() {
    let kernel = ProcessKernel::new();
    let mut policy = EgressPolicy::sealed();
    policy.grant_read("/definitely/missing/castalia-landlock-grant");

    let agent = launch_confined_with_egress(&kernel, &policy, |_sock| 42)
        .expect("fork confined PD with malformed Landlock grant");
    let verdict = agent.join_verdict().expect("reap failed confinement child");

    assert_eq!(
        verdict, CONFINE_FAILED_EXIT,
        "an invalid granted path must fail confinement before the child body runs"
    );
}
