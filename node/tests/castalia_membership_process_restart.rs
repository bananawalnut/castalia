#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dregg_cell::{AuthRequired, CapabilityRef, CellId, FACET_STATE_WRITER};
use dregg_sdk::{AgentCipherclerk, SignedTurn};
use dregg_turn::Effect;
use starbridge_castalia_membership::{
    CHANGED_AT_SLOT, CastaliaMemberApplicationV1, GENERATION_SLOT, MembershipStatus, STATUS_SLOT,
    castalia_membership_factory, field_from_u64, membership_birth_token_id,
    membership_initial_fields,
};
use zeroize::Zeroizing;

const NODE_BIN: &str = env!("CARGO_BIN_EXE_dregg-node");
const PASSPHRASE: &str = "castalia-process-restart-test";
const WAIT: Duration = Duration::from_secs(90);

struct NodeProc {
    child: Option<Child>,
    log: std::path::PathBuf,
}

impl NodeProc {
    fn stop(mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for NodeProc {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn http_get(port: u16, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().ok()?,
        Duration::from_secs(2),
    )
    .ok()?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

fn http_post(
    port: u16,
    path: &str,
    content_type: &str,
    bearer: Option<&str>,
    body: &[u8],
) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().ok()?,
        Duration::from_secs(2),
    )
    .ok()?;
    let authorization = bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: {content_type}\r\n{authorization}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).ok()?;
    stream.write_all(body).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

fn response_json(response: &str) -> serde_json::Value {
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or(response);
    serde_json::from_str(body)
        .unwrap_or_else(|error| panic!("response body is not JSON ({error}): {response}"))
}

fn wait_ready(port: u16) -> bool {
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        if http_get(port, "/status")
            .as_deref()
            .is_some_and(|response| response.contains("200 OK"))
        {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    false
}

fn launch(data_dir: &std::path::Path, http: u16, gossip: u16, log_name: &str) -> NodeProc {
    let log = data_dir.join(log_name);
    let log_file = std::fs::File::create(&log).unwrap();
    let child = Command::new(NODE_BIN)
        .arg("run")
        .arg("--data-dir")
        .arg(data_dir)
        .args(["--key-file", "node.key"])
        .args(["--node-index", "0"])
        .args(["--federation-size", "1"])
        .args(["--port", &http.to_string()])
        .args(["--gossip-port", &gossip.to_string()])
        .args(["--bind", "127.0.0.1"])
        .args(["--federation-mode", "solo"])
        .args(["--consensus", "blocklace"])
        .args(["--idle-heartbeat-ms", "2000"])
        .args(["--block-cadence-ms", "500"])
        .arg("--enable-faucet")
        .env("DREGG_LEAN_PRODUCER", "0")
        // This subprocess drill exercises persistence/composition against the
        // Rust executor when the local test artifact is marshal-only. Production
        // startup remains fail-closed without the verified Lean archive.
        .env("DREGG_ALLOW_UNVERIFIED_CONSENSUS", "1")
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file))
        .spawn()
        .expect("spawn dregg-node process");
    NodeProc {
        child: Some(child),
        log,
    }
}

fn unlock(port: u16) -> String {
    let response = http_post(
        port,
        "/cipherclerk/unlock",
        "application/json",
        None,
        format!("{{\"passphrase\":\"{PASSPHRASE}\"}}").as_bytes(),
    )
    .expect("unlock request reached node");
    let json = response_json(&response);
    json["bearer_token"]
        .as_str()
        .expect("unlock returned bearer token")
        .to_string()
}

fn submit(port: u16, bearer: &str, signed: &SignedTurn) -> serde_json::Value {
    let wire = postcard::to_stdvec(signed).expect("encode SignedTurn");
    let response = http_post(
        port,
        "/turns/submit",
        "application/octet-stream",
        Some(bearer),
        &wire,
    )
    .expect("turn submission reached node");
    response_json(&response)
}

fn cell_detail(port: u16, cell: CellId) -> Option<serde_json::Value> {
    let response = http_get(port, &format!("/api/cell/{}", hex(&cell.0)))?;
    let json = response_json(&response);
    json["found"].as_bool().unwrap_or(false).then_some(json)
}

fn wait_cell(
    port: u16,
    cell: CellId,
    generation: u64,
    status: MembershipStatus,
) -> serde_json::Value {
    let deadline = Instant::now() + WAIT;
    let mut latest = None;
    while Instant::now() < deadline {
        if let Some(detail) = cell_detail(port, cell) {
            let observed_generation = field_u64(&detail, GENERATION_SLOT as usize);
            let observed_status = field_u64(&detail, STATUS_SLOT as usize);
            latest = Some(detail.clone());
            if observed_generation == generation && observed_status == status as u64 {
                return detail;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    panic!(
        "membership did not reach generation {generation}, status {status:?}; latest={latest:?}"
    );
}

fn wait_cell_with_log(
    port: u16,
    cell: CellId,
    generation: u64,
    status: MembershipStatus,
    node: &NodeProc,
) -> serde_json::Value {
    let deadline = Instant::now() + WAIT;
    let mut latest = None;
    while Instant::now() < deadline {
        if let Some(detail) = cell_detail(port, cell) {
            let observed_generation = field_u64(&detail, GENERATION_SLOT as usize);
            let observed_status = field_u64(&detail, STATUS_SLOT as usize);
            latest = Some(detail.clone());
            if observed_generation == generation && observed_status == status as u64 {
                return detail;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    panic!(
        "membership did not reach generation {generation}, status {status:?}; latest={latest:?}; node_log={}",
        node.log_text()
    );
}

fn field_u64(detail: &serde_json::Value, slot: usize) -> u64 {
    let encoded = detail["fields"][slot]
        .as_str()
        .unwrap_or_else(|| panic!("missing field slot {slot}: {detail}"));
    let bytes = decode_32(encoded).unwrap_or_else(|| panic!("invalid field encoding: {encoded}"));
    u64::from_be_bytes(bytes[24..].try_into().unwrap())
}

fn decode_32(encoded: &str) -> Option<[u8; 32]> {
    if encoded.len() != 64 {
        return None;
    }
    let mut out = [0; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn wallet_owner_public_key() -> [u8; 32] {
    match std::env::var("CASTALIA_WALLET_OWNER_PUBLIC_KEY") {
        Ok(encoded) => {
            let owner = decode_32(&encoded)
                .expect("CASTALIA_WALLET_OWNER_PUBLIC_KEY must be exactly 32-byte hex");
            assert_ne!(owner, [0; 32], "wallet owner public key must be non-zero");
            owner
        }
        Err(std::env::VarError::NotPresent) => [0x52; 32],
        Err(error) => panic!("invalid CASTALIA_WALLET_OWNER_PUBLIC_KEY: {error}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_plus_hour() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 3600
}

fn lifecycle_turn(
    clerk: &AgentCipherclerk,
    federation_id: &[u8; 32],
    actor: CellId,
    member: CellId,
    actor_detail: &serde_json::Value,
    method: &str,
    status: MembershipStatus,
    generation: u64,
    changed_at: u64,
) -> SignedTurn {
    let nonce = actor_detail["nonce"].as_u64().expect("authority nonce");
    let effects = vec![
        Effect::SetField {
            cell: member,
            index: STATUS_SLOT as usize,
            value: field_from_u64(status as u64),
        },
        Effect::SetField {
            cell: member,
            index: GENERATION_SLOT as usize,
            value: field_from_u64(generation),
        },
        Effect::SetField {
            cell: member,
            index: CHANGED_AT_SLOT as usize,
            value: field_from_u64(changed_at),
        },
    ];
    let action = clerk.make_action(member, method, effects, federation_id);
    let mut turn = clerk.make_turn(action);
    turn.agent = actor;
    turn.nonce = nonce;
    turn.previous_receipt_hash = actor_detail["last_receipt_hash"]
        .as_str()
        .and_then(decode_32);
    turn.fee = 1_000;
    turn.valid_until = Some(now_plus_hour());
    clerk.sign_turn(&turn)
}

fn assert_accepted(response: &serde_json::Value, label: &str) {
    assert_eq!(
        response["accepted"].as_bool(),
        Some(true),
        "{label} was not accepted: {response}"
    );
}

fn expose_active_membership_to_wallet_smoke(
    http: u16,
    member: CellId,
    authority: [u8; 32],
    application: &CastaliaMemberApplicationV1,
) {
    let Some(snapshot_path) = std::env::var_os("CASTALIA_WALLET_LIVE_SNAPSHOT") else {
        return;
    };
    let snapshot_path = std::path::PathBuf::from(snapshot_path);
    let acknowledgement = std::path::PathBuf::from(format!("{}.done", snapshot_path.display()));
    let snapshot = serde_json::json!({
        "nodeUrl": format!("http://127.0.0.1:{http}"),
        "expectation": {
            "cellId": hex(&member.0),
            "authorityPublicKey": hex(&authority),
            "applicationCommitment": hex(&application.commitment()),
            "application": {
                "factoryId": hex(&application.factory_id),
                "programId": hex(&application.program_id),
                "officialDreggCellId": hex(&application.official_dregg_cell_id.0),
                "ownerPublicKey": hex(&application.owner_pubkey),
                "applicationKind": application.application_kind,
                "applicationVersion": application.application_version,
                "applicationNonce": application.application_nonce,
                "membershipClass": application.membership_class,
                "jurisdictionCode": application.jurisdiction_code,
                "applicationFlags": application.application_flags,
                "createdAt": application.created_at,
            },
        },
    });
    std::fs::write(
        &snapshot_path,
        serde_json::to_vec_pretty(&snapshot).expect("serialize public wallet smoke metadata"),
    )
    .expect("write public wallet smoke metadata");

    let deadline = Instant::now() + WAIT;
    while !acknowledgement.exists() {
        assert!(
            Instant::now() < deadline,
            "wallet smoke did not acknowledge live membership metadata at {}",
            snapshot_path.display()
        );
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn castalia_registration_survives_real_node_restart_and_lifecycle_continues() {
    let temp = tempfile::tempdir().unwrap();
    let genesis_dir = temp.path().join("genesis");
    std::fs::create_dir_all(&genesis_dir).unwrap();
    assert!(
        Command::new(NODE_BIN)
            .args(["genesis", "--validators", "1", "--output"])
            .arg(&genesis_dir)
            .status()
            .unwrap()
            .success()
    );

    let clerk = AgentCipherclerk::from_key_bytes(Zeroizing::new([0xa1; 32]));
    let authority = clerk.public_key().0;

    let genesis_path = genesis_dir.join("genesis.json");
    let mut genesis: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&genesis_path).unwrap()).unwrap();
    genesis["castalia_membership_authority"] = serde_json::Value::String(hex(&authority));
    std::fs::write(&genesis_path, serde_json::to_vec_pretty(&genesis).unwrap()).unwrap();
    let federation_id = decode_32(
        genesis["federation_id"]
            .as_str()
            .expect("genesis federation id"),
    )
    .unwrap();

    let node_dir = temp.path().join("node");
    std::fs::create_dir_all(&node_dir).unwrap();
    std::fs::copy(&genesis_path, node_dir.join("genesis.json")).unwrap();
    let _ = std::fs::copy(genesis_dir.join(".devnet"), node_dir.join(".devnet"));
    std::fs::copy(genesis_dir.join("node-0.key"), node_dir.join("node.key")).unwrap();

    let http = free_port();
    let gossip = free_port();
    let first = launch(&node_dir, http, gossip, "first-boot.log");
    assert!(wait_ready(http), "first boot failed: {}", first.log_text());
    let bearer = unlock(http);

    let actor = clerk.cell_id("default");
    // Active blocklace is the sole authoritative writer even at committee size
    // one. The historical amount=0 shortcut created a hosted cell directly at
    // admission, outside finalization and therefore outside the durable commit
    // overlay. It must now fail closed and leave the ledger untouched.
    let zero_materialization = response_json(
        &http_post(
            http,
            "/api/faucet",
            "application/json",
            None,
            format!("{{\"recipient\":\"{}\",\"amount\":0}}", hex(&actor.0)).as_bytes(),
        )
        .expect("zero-amount faucet request reached node"),
    );
    assert_eq!(zero_materialization["success"].as_bool(), Some(false));
    assert!(
        cell_detail(http, actor).is_none(),
        "rejected zero-amount admission must not materialize a cell"
    );

    let authority_funding = response_json(
        &http_post(
            http,
            "/api/faucet",
            "application/json",
            None,
            format!("{{\"recipient\":\"{}\",\"amount\":10000}}", hex(&actor.0)).as_bytes(),
        )
        .expect("authority faucet request reached node"),
    );
    assert_eq!(
        authority_funding["success"].as_bool(),
        Some(true),
        "external Castalia authority funding failed: {authority_funding}"
    );
    let funding_deadline = Instant::now() + WAIT;
    let authority_detail = loop {
        if let Some(detail) = cell_detail(http, actor)
            && detail["balance"]
                .as_i64()
                .is_some_and(|balance| balance >= 10_000)
        {
            break detail;
        }
        assert!(
            Instant::now() < funding_deadline,
            "authority faucet grant did not finalize"
        );
        thread::sleep(Duration::from_millis(500));
    };

    let factory = castalia_membership_factory(authority).unwrap();
    let wallet_owner = wallet_owner_public_key();
    let official_cell = CellId::derive_raw(
        &wallet_owner,
        &*blake3::hash(b"castalia-official-cell").as_bytes(),
    );
    let application = CastaliaMemberApplicationV1 {
        factory_id: factory.factory_vk(),
        program_id: factory.child_program_vk(),
        official_dregg_cell_id: official_cell,
        owner_pubkey: wallet_owner,
        application_kind: 7,
        application_version: 1,
        application_nonce: 1,
        membership_class: 2,
        jurisdiction_code: 840,
        application_flags: 0,
        created_at: 1_700_000_000,
    };
    let params = factory.creation_params(&application).unwrap();
    let member_token = membership_birth_token_id(factory.factory_vk(), application.commitment(), 7);
    let member = CellId::derive_raw(&authority, &member_token);

    let mut birth = clerk.create_from_factory(
        actor,
        factory.factory_vk(),
        authority,
        member_token,
        params,
        &federation_id,
    );
    let birth_live_nonce = authority_detail["nonce"].as_u64().unwrap();
    let mut birth_effects = birth.call_forest.roots[0].action.effects.clone();
    birth_effects.push(Effect::GrantCapability {
        from: actor,
        to: actor,
        cap: CapabilityRef {
            target: member,
            slot: 0,
            permissions: AuthRequired::Signature,
            breadstuff: None,
            expires_at: None,
            allowed_effects: Some(FACET_STATE_WRITER),
            stored_epoch: None,
            provenance: [0; 32],
        },
    });
    birth_effects.push(Effect::Transfer {
        from: actor,
        to: member,
        amount: 1_000,
    });
    let reserve_action =
        clerk.make_action(actor, "create_from_factory", birth_effects, &federation_id);
    birth.call_forest.roots[0].action = reserve_action;
    birth.nonce = birth_live_nonce;
    birth.previous_receipt_hash = authority_detail["last_receipt_hash"]
        .as_str()
        .and_then(decode_32);
    birth.fee = 5_000;
    birth.valid_until = Some(now_plus_hour());
    let birth_response = submit(http, &bearer, &clerk.sign_turn(&birth));
    assert_accepted(&birth_response, "membership birth");

    let pending = wait_cell(http, member, 0, MembershipStatus::Pending);
    assert_eq!(pending["balance"].as_i64(), Some(1_000));
    assert_eq!(pending["program_kind"].as_str(), Some("Cases"));
    assert_eq!(pending["fields"].as_array().unwrap().len(), 16);

    let activate = lifecycle_turn(
        &clerk,
        &federation_id,
        actor,
        member,
        &cell_detail(http, actor).unwrap(),
        "activate",
        MembershipStatus::Active,
        1,
        application.created_at + 1,
    );
    assert_accepted(&submit(http, &bearer, &activate), "activation");
    let before_restart = wait_cell_with_log(http, member, 1, MembershipStatus::Active, &first);
    let fields_before_restart = before_restart["fields"].clone();
    let program_before_restart = before_restart["program"].clone();
    let balance_before_restart = before_restart["balance"].clone();
    let nonce_before_restart = before_restart["nonce"].clone();
    let authority_nonce_before_restart = cell_detail(http, actor).unwrap()["nonce"].clone();
    expose_active_membership_to_wallet_smoke(http, member, authority, &application);

    let first_boot_log = first.log_text();
    first.stop();
    let second = launch(&node_dir, http, gossip, "second-boot.log");
    assert!(
        wait_ready(http),
        "restart failed: first_boot_log={first_boot_log}; second_boot_log={}",
        second.log_text()
    );
    let bearer = unlock(http);
    let reconstructed = wait_cell(http, member, 1, MembershipStatus::Active);
    assert_eq!(reconstructed["fields"], fields_before_restart);
    assert_eq!(reconstructed["nonce"], nonce_before_restart);
    assert_eq!(reconstructed["program"], program_before_restart);
    assert_eq!(reconstructed["balance"], balance_before_restart);
    assert_eq!(
        cell_detail(http, actor).unwrap()["nonce"],
        authority_nonce_before_restart,
        "authority replay nonce must survive the physical restart"
    );

    let steps = [
        (
            "suspend",
            MembershipStatus::Suspended,
            2,
            application.created_at + 2,
        ),
        (
            "resume",
            MembershipStatus::Active,
            3,
            application.created_at + 3,
        ),
        (
            "revoke",
            MembershipStatus::Revoked,
            4,
            application.created_at + 4,
        ),
    ];
    let mut detail = reconstructed;
    for (method, status, generation, changed_at) in steps {
        let signed = lifecycle_turn(
            &clerk,
            &federation_id,
            actor,
            member,
            &cell_detail(http, actor).unwrap(),
            method,
            status,
            generation,
            changed_at,
        );
        assert_accepted(&submit(http, &bearer, &signed), method);
        detail = wait_cell(http, member, generation, status);
    }

    assert_eq!(
        detail["fields"].as_array().unwrap().len(),
        membership_initial_fields(&application).len()
    );
    assert_eq!(
        field_u64(&detail, STATUS_SLOT as usize),
        MembershipStatus::Revoked as u64
    );
    assert_eq!(field_u64(&detail, GENERATION_SLOT as usize), 4);
    assert_eq!(
        field_u64(&detail, CHANGED_AT_SLOT as usize),
        application.created_at + 4
    );
    assert_eq!(detail["balance"], balance_before_restart);

    second.stop();
}
