//! # execution-lease — the UI as a deos-view CARD (a `deos.ui.*` view-tree).
//!
//! The fourth axis of a modern starbridge-app: the app lives IN the deos world by
//! shipping its surface as a **renderer-independent card** — a serializable
//! `deos.ui.*` element-tree. The SAME tree renders three ways (native gpui in the
//! cockpit, a browser HTML document, a discord embed), all from this one piece of
//! DATA. The app's contribution is the view-tree JSON (pure `serde_json`, no
//! renderer dependency): the deos world's renderers consume it.
//!
//! ## The card shape — a live "rent durable execution, metered, pay-or-lapse" dashboard
//!
//! A titled column built from the rich deos-view vocabulary, surfacing the WHOLE
//! economic story of a durable-execution lease:
//!
//!   * a **status header** — the app name + a LIVE `pill` reading
//!     [`LAPSED_SLOT`](crate::LAPSED_SLOT) and naming the lease state
//!     (`LIVE` / `LAPSED`), a row of static **cap-tier** `pill`s naming the two roles
//!     on the attenuation lattice (the AGENT pays + drives; the PROVIDER owns the
//!     slot + may lapse it), and a `breadcrumb` of the lease lifecycle
//!     (Open → Running → Lapsed) with the current band marked;
//!   * a **"Meter" `section`** — the "metered, pay-or-lapse" half: the KILLER VISUAL
//!     is a `gauge` bound to [`PERIODS_PAID_SLOT`](crate::PERIODS_PAID_SLOT) over the
//!     demo lease length (the budget consumed as rent periods elapse), plus `bind`s on
//!     the per-period rent ([`RENT_SLOT`](crate::RENT_SLOT)), the period length in
//!     blocks ([`PERIOD_SLOT`](crate::PERIOD_SLOT)), and the periods-paid count
//!     (adept — the gauge already shows it);
//!   * a **"Durable execution" `section`** — the durable-step half: the checkpoint
//!     cursor ([`STEP_SLOT`](crate::STEP_SLOT) — the umem heap image step that only
//!     moves forward), the committed [`STATE_DIGEST_SLOT`](crate::STATE_DIGEST_SLOT)
//!     (adept), and the [`PROVIDER_SLOT`](crate::PROVIDER_SLOT) as an avatar handle;
//!   * an **"Actions" `section`** of one `icon`+`button` row per service operation
//!     (`open` / `pay` / `advance` / `status`), each `button` carrying its `onClick`.
//!
//! The button `turn` names match the [`service`](crate::service) method vocabulary.

use serde_json::{Value, json};

use crate::service::{METHOD_ADVANCE, METHOD_OPEN, METHOD_PAY, METHOD_STATUS};
use crate::{
    LAPSED_SLOT, PERIOD_SLOT, PERIODS_PAID_SLOT, PROVIDER_SLOT, RENT_SLOT, STATE_DIGEST_SLOT,
    STEP_SLOT,
};

/// The demo lease length the periods-paid gauge fills over — the budget the lease
/// is metered against (the "spent / budget" denominator made visible).
pub const DEMO_LEASE_PERIODS: u64 = 12;

/// A `deos.ui.text` node.
fn text(s: &str) -> Value {
    json!({ "kind": "text", "props": { "text": s } })
}

/// A static `deos.ui.pill` node — a colored status badge (no live slot).
fn pill(label: &str, tag: &str) -> Value {
    json!({ "kind": "pill", "props": { "text": label, "tag": tag } })
}

/// A LIVE `deos.ui.pill` node — reads `slot` immediate-mode and maps the value to a
/// word + color via `cases` (the first `{value,label,tag}` matching the slot wins).
/// `text`/`tag` are the static fallback (discord, or no match).
fn pill_live(slot: usize, label: &str, tag: &str, cases: Value) -> Value {
    json!({ "kind": "pill", "props": { "text": label, "tag": tag, "slot": slot, "cases": cases } })
}

/// A `deos.ui.icon` node — a glyph indicator tinted by `tag`.
fn icon(glyph: &str, tag: &str) -> Value {
    json!({ "kind": "icon", "props": { "glyph": glyph, "tag": tag } })
}

/// A `deos.ui.divider` node — a full-width groove rule.
fn divider() -> Value {
    json!({ "kind": "divider", "props": {} })
}

/// A `deos.ui.row` node — a horizontal flex of children.
fn row(children: Vec<Value>) -> Value {
    json!({ "kind": "row", "props": {}, "children": children })
}

/// A `deos.ui.section` node — a titled, bordered container.
fn section(title: &str, tag: &str, children: Vec<Value>) -> Value {
    json!({ "kind": "section", "props": { "title": title, "tag": tag }, "children": children })
}

/// A `deos.ui.bind` node tagged with the model `slot` it re-reads + a label prefix and a
/// display `fmt` (`"id"` avatar-handle / `"hash"` short-hex / `"amount"` grouped /
/// `"raw"` plain). `adept` hides a dev-y row (a raw digest / a count the gauge already
/// shows) from the simple projection; revealed in the adept "see the bones" view.
fn bind(slot: usize, label: &str, fmt: &str, adept: bool) -> Value {
    json!({ "kind": "bind", "props": { "slot": slot, "label": label, "fmt": fmt, "adept": adept } })
}

/// A `deos.ui.gauge` node — a bound progress bar (`slot_value / max`, immediate-mode).
fn gauge(slot: usize, max: u64, label: &str) -> Value {
    json!({ "kind": "gauge", "props": { "slot": slot, "max": max, "label": label } })
}

/// A `deos.ui.breadcrumb` node — the lifecycle path; the `active` step is marked `›`.
fn breadcrumb(steps: &[&str], active: usize) -> Value {
    let items: Vec<Value> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let label = if i == active {
                format!("› {s}")
            } else {
                s.to_string()
            };
            json!({ "label": label })
        })
        .collect();
    json!({ "kind": "breadcrumb", "props": { "items": items } })
}

/// A `deos.ui.button` node carrying its affordance payload `onClick = {turn, arg}`.
fn button(label: &str, turn: &str, arg: i64) -> Value {
    json!({
        "kind": "button",
        "props": { "label": label, "onClick": { "turn": turn, "arg": arg } }
    })
}

/// An action row — an `icon` + a service `button` (the verified-turn affordance).
fn action(glyph: &str, label: &str, turn: &str) -> Value {
    row(vec![icon(glyph, "accent"), button(label, turn, 0)])
}

/// **The execution-lease card as a `deos.ui.*` view-tree** (a `serde_json::Value`).
///
/// A live durable-execution-lease dashboard telling the full "rent durable
/// execution, metered, pay-or-lapse" story. It shows a status header with the
/// name, LIVE/LAPSED pill, two cap-tier pills, and lease-lifecycle breadcrumb; a
/// "Meter" section whose KILLER VISUAL is the periods-paid gauge (the budget
/// consumed) plus the rent/period, period-length, and periods-paid binds; a
/// "Durable execution" section surfacing the checkpoint cursor, state digest,
/// and provider; and an "Actions" section of the open / pay / advance / status
/// buttons. Renderer-independent DATA.
/// The button `turn` names are the [`service`](crate::service) method symbols.
pub fn lease_card_value() -> Value {
    json!({
        "kind": "vstack",
        "props": {},
        "children": [
            // The header pill reads the LIVE lapsed slot and names the lease state.
            row(vec![text("Durable Execution Lease"), pill_live(LAPSED_SLOT as usize, "LIVE", "good", json!([
                { "value": 0, "label": "LIVE",   "tag": "good" },
                { "value": 1, "label": "LAPSED", "tag": "danger" },
            ]))]),
            // The two roles on the attenuation lattice (cap tiers).
            row(vec![
                pill("agent · pays", "accent"),
                pill("provider · owns", "muted"),
            ]),
            breadcrumb(&["Open", "Running", "Lapsed"], 1),
            divider(),
            section("Meter", "genuine", vec![
                // THE KILLER VISUAL: the budget consumed as rent periods elapse.
                gauge(PERIODS_PAID_SLOT as usize, DEMO_LEASE_PERIODS, "rent paid "),
                bind(RENT_SLOT as usize, "rent/period · ", "amount", false),
                bind(PERIOD_SLOT as usize, "period · ", "raw", false),
                // The count the gauge already shows — adept bones.
                bind(PERIODS_PAID_SLOT as usize, "periods paid · ", "raw", true),
            ]),
            section("Durable execution", "", vec![
                // The durable checkpoint cursor (the umem heap image step).
                bind(STEP_SLOT as usize, "checkpoint · ", "amount", false),
                // The committed state digest — a dev-y bone.
                bind(STATE_DIGEST_SLOT as usize, "state digest · ", "hash", true),
                bind(PROVIDER_SLOT as usize, "provider · ", "id", false),
            ]),
            section("Actions", "", vec![
                action("⊕", "Open",      METHOD_OPEN),
                action("$", "Pay rent",  METHOD_PAY),
                action("→", "Advance",   METHOD_ADVANCE),
                action("≡", "Status",    METHOD_STATUS),
            ]),
        ]
    })
}

/// **The execution-lease card as serialized `deos.ui.*` JSON** — the string a host
/// serves / embeds (`JSON.stringify(tree)` shape a `deos-view` renderer parses).
pub fn lease_card_json() -> String {
    serde_json::to_string(&lease_card_value()).expect("the lease card serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect<'a>(node: &'a Value, kind: &str, out: &mut Vec<&'a Value>) {
        if node["kind"] == kind {
            out.push(node);
        }
        if let Some(children) = node["children"].as_array() {
            for c in children {
                collect(c, kind, out);
            }
        }
    }

    fn of_kind<'a>(card: &'a Value, kind: &str) -> Vec<&'a Value> {
        let mut out = Vec::new();
        collect(card, kind, &mut out);
        out
    }

    #[test]
    fn the_card_is_a_vstack_with_a_named_header_and_a_live_status_pill() {
        let card = lease_card_value();
        assert_eq!(card["kind"], "vstack");
        let texts = of_kind(&card, "text");
        assert!(
            texts
                .iter()
                .any(|t| t["props"]["text"] == "Durable Execution Lease")
        );
        // The header pill is LIVE: it reads LAPSED_SLOT and maps to LIVE / LAPSED.
        let live_pills: Vec<&Value> = of_kind(&card, "pill")
            .into_iter()
            .filter(|p| p["props"].get("slot").is_some())
            .collect();
        assert_eq!(live_pills.len(), 1, "exactly one LIVE status pill");
        assert_eq!(live_pills[0]["props"]["slot"], LAPSED_SLOT as usize);
        let cases = live_pills[0]["props"]["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2, "LIVE / LAPSED");
        assert_eq!(cases[1]["label"], "LAPSED");
    }

    #[test]
    fn the_two_cap_tiers_are_named_as_static_pills() {
        let card = lease_card_value();
        // The static (no-slot) pills name the two roles on the attenuation lattice.
        let static_pills: Vec<String> = of_kind(&card, "pill")
            .into_iter()
            .filter(|p| p["props"].get("slot").is_none())
            .map(|p| p["props"]["text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(static_pills, vec!["agent · pays", "provider · owns"]);
    }

    #[test]
    fn the_lifecycle_breadcrumb_marks_the_running_band() {
        let card = lease_card_value();
        let crumbs = of_kind(&card, "breadcrumb");
        assert_eq!(crumbs.len(), 1);
        let items = crumbs[0]["props"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 3, "Open → Running → Lapsed");
        assert_eq!(items[1]["label"], "› Running", "the active band is marked");
    }

    #[test]
    fn the_meter_gauge_reads_the_live_periods_paid_slot() {
        let card = lease_card_value();
        let gauges = of_kind(&card, "gauge");
        assert_eq!(gauges.len(), 1, "the killer visual: the periods-paid gauge");
        assert_eq!(gauges[0]["props"]["slot"], PERIODS_PAID_SLOT as usize);
        assert_eq!(gauges[0]["props"]["max"], DEMO_LEASE_PERIODS);
    }

    #[test]
    fn the_meter_and_durable_sections_surface_the_lease_state() {
        let card = lease_card_value();
        // The two titled sections (plus Actions) split the meter from the durable image.
        let titles: Vec<&str> = of_kind(&card, "section")
            .iter()
            .map(|s| s["props"]["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, vec!["Meter", "Durable execution", "Actions"]);

        // The binds surface rent / period / periods-paid (meter), then checkpoint /
        // digest / provider (durable image), in pre-order (the bind-cursor order).
        let binds = of_kind(&card, "bind");
        let slots: Vec<u64> = binds
            .iter()
            .map(|b| b["props"]["slot"].as_u64().unwrap())
            .collect();
        assert_eq!(
            slots,
            vec![
                RENT_SLOT as u64,
                PERIOD_SLOT as u64,
                PERIODS_PAID_SLOT as u64,
                STEP_SLOT as u64,
                STATE_DIGEST_SLOT as u64,
                PROVIDER_SLOT as u64,
            ],
        );
    }

    #[test]
    fn the_dev_y_rows_are_marked_adept_and_the_human_terms_stay_visible() {
        let card = lease_card_value();
        let adept = |slot: usize| -> bool {
            of_kind(&card, "bind")
                .iter()
                .find(|b| b["props"]["slot"].as_u64() == Some(slot as u64))
                .and_then(|b| b["props"]["adept"].as_bool())
                .unwrap()
        };
        // The raw periods-paid count (the gauge shows it) + the sealed state digest hide.
        assert!(adept(PERIODS_PAID_SLOT as usize), "the raw periods count");
        assert!(adept(STATE_DIGEST_SLOT as usize), "the committed digest");
        // The human-meaningful economics + cursor stay visible.
        assert!(!adept(RENT_SLOT as usize));
        assert!(!adept(PERIOD_SLOT as usize));
        assert!(!adept(STEP_SLOT as usize));
        assert!(!adept(PROVIDER_SLOT as usize));
    }

    #[test]
    fn the_amount_and_identity_binds_carry_their_display_fmt() {
        let card = lease_card_value();
        let binds = of_kind(&card, "bind");
        let fmt = |slot: usize| -> String {
            binds
                .iter()
                .find(|b| b["props"]["slot"].as_u64() == Some(slot as u64))
                .and_then(|b| b["props"]["fmt"].as_str())
                .unwrap()
                .to_string()
        };
        assert_eq!(fmt(RENT_SLOT as usize), "amount", "rent groups digits");
        assert_eq!(fmt(STEP_SLOT as usize), "amount", "the checkpoint cursor");
        assert_eq!(fmt(STATE_DIGEST_SLOT as usize), "hash", "digest paints hex");
        assert_eq!(
            fmt(PROVIDER_SLOT as usize),
            "id",
            "the provider paints an avatar handle"
        );
    }

    #[test]
    fn every_button_carries_its_service_method_as_the_turn_payload() {
        let card = lease_card_value();
        let buttons = of_kind(&card, "button");
        assert_eq!(buttons.len(), 4);
        let turns: Vec<&str> = buttons
            .iter()
            .map(|b| b["props"]["onClick"]["turn"].as_str().unwrap())
            .collect();
        assert_eq!(
            turns,
            vec![METHOD_OPEN, METHOD_PAY, METHOD_ADVANCE, METHOD_STATUS]
        );
    }

    #[test]
    fn the_card_serializes_to_parseable_json() {
        let s = lease_card_json();
        let back: Value = serde_json::from_str(&s).expect("the card JSON parses");
        assert_eq!(back["kind"], "vstack");
        // header row, cap-tier row, breadcrumb, divider, Meter, Durable, Actions.
        assert_eq!(back["children"].as_array().unwrap().len(), 7);
    }
}
