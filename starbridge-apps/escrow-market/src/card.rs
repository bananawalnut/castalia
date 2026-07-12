//! # escrow-market — the UI as a deos-view CARD (a `deos.ui.*` view-tree).
//!
//! The fourth axis of a modern starbridge-app: the app lives IN the deos world by
//! shipping its surface as a **renderer-independent card** — a serializable
//! `deos.ui.*` element-tree ([`deos_view::ViewNode`] in the renderer crate). The
//! SAME tree renders three ways (the renderer-independence seam): native gpui
//! pixels in the cockpit, a browser-loadable HTML document, and a discord embed —
//! all from this one piece of DATA. See `deos-view/src/{render,web,discord}.rs`.
//!
//! ## Why the card is DATA, not a renderer call
//!
//! `deos-view` pulls BOTH the SpiderMonkey (`mozjs`) and the gpui native
//! elephants, so it is a STANDALONE workspace EXCLUDED from the repo-root
//! workspace. A starbridge-app must never depend on it — that would feature-unify
//! the elephants onto the main build. So the app's contribution is the
//! **view-tree JSON** (this module): pure `serde_json`, no elephant.
//!
//! ## The card shape — a rich, live trustless-swap surface
//!
//! A titled column ([`deos.ui.vstack`](deos_view)) built from the rich deos-view
//! vocabulary (`section` / `pill` / `gauge` / `breadcrumb` / `divider` / `icon`),
//! telling the whole "I give you X iff you give me Y" escrow story:
//!
//!   - a **status header** — the app name + a LIVE leg-status `pill` reading
//!     [`STATE_SLOT`](crate::STATE_SLOT) (the value maps to a word + color:
//!     `LISTED`/`FUNDED`/`SHIPPED`/`SETTLED`), a row of static **cap-tier** `pill`s
//!     naming the three roles on the `observer ⊂ buyer ⊂ seller` lattice, and a
//!     `breadcrumb` of the whole lifecycle with the current step marked;
//!   - an **"Escrow" `section`** — the trustline + parties: the KILLER VISUAL is a
//!     `gauge` of the escrowed amount ([`ESCROWED_SLOT`](crate::ESCROWED_SLOT)) read
//!     against the listing ceiling (the TRUSTLINE draw `ESCROWED ≤ CEILING`,
//!     visualized), plus `bind`s on the ceiling / escrowed amounts (grouped digits),
//!     the seller / buyer keys (avatar handles), and the sealed-delivery digest
//!     (adept — the mailbox commitment bone);
//!   - a **"Settlement" `section`** — the FLASHWELL split: `bind`s on the funds
//!     released to the seller ([`RELEASED_SLOT`](crate::RELEASED_SLOT)) and refunded
//!     to the buyer ([`REFUNDED_SLOT`](crate::REFUNDED_SLOT)) — the conserving
//!     `RELEASED + REFUNDED == ESCROWED` payout, shown live;
//!   - an **"Actions" `section`** of one `icon`+`button` row per sealed-escrow method
//!     (`open` / `deposit` / `settle` / `reclaim`), each carrying its
//!     `onClick = { turn, arg }` — the method symbol the [`service`](crate::service)
//!     face routes.
//!
//! The button `turn` names match the [`service`](crate::service) method vocabulary
//! ([`METHOD_OPEN`](crate::service::METHOD_OPEN), …) so the card and the service
//! cell speak the same sealed-escrow lifecycle.

use serde_json::{Value, json};

use crate::service::{METHOD_DEPOSIT, METHOD_OPEN, METHOD_RECLAIM, METHOD_SETTLE};
use crate::{
    BUYER_HASH_SLOT, CEILING_SLOT, DELIVERY_HASH_SLOT, ESCROWED_SLOT, REFUNDED_SLOT, RELEASED_SLOT,
    SELLER_HASH_SLOT, STATE_FUNDED, STATE_LISTED, STATE_SETTLED, STATE_SHIPPED, STATE_SLOT,
};

/// The escrow gauge's denominator — a representative listing ceiling so a fully-drawn
/// escrow fills the gauge (the seeded ceiling is `1_000`). The TRUSTLINE invariant
/// (`FieldLteField { ESCROWED ≤ CEILING }`) means a live escrow never exceeds it, so
/// the bar is the trustline-draw ratio made visible.
const CEILING_GAUGE_MAX: u64 = 1_000;

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

/// A `deos.ui.section` node — a titled, bordered container.
fn section(title: &str, tag: &str, children: Vec<Value>) -> Value {
    json!({ "kind": "section", "props": { "title": title, "tag": tag }, "children": children })
}

/// A `deos.ui.row` node — a horizontal flex of children.
fn row(children: Vec<Value>) -> Value {
    json!({ "kind": "row", "props": {}, "children": children })
}

/// A `deos.ui.bind` node tagged with the model `slot` it re-reads + a label prefix and
/// a display `fmt` (`"id"` avatar-handle / `"hash"` short-hex / `"amount"` grouped /
/// `"raw"` plain). `adept` hides a dev-y row (the sealed delivery digest) from the
/// simple projection; revealed in the adept "see the bones" view.
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

/// An action row — an `icon` + a sealed-escrow `button` (the verified-turn affordance).
fn action(glyph: &str, label: &str, turn: &str) -> Value {
    row(vec![icon(glyph, "accent"), button(label, turn, 0)])
}

/// **The escrow-market card as a `deos.ui.*` view-tree** (a `serde_json::Value`).
///
/// A rich, live trustless-swap surface: a status header (name, LIVE leg-status
/// pill, the three cap-tier pills, and lifecycle breadcrumb), an "Escrow" section
/// surfacing the trustline-draw gauge (escrowed vs ceiling), the ceiling /
/// escrowed amounts, the seller / buyer avatar handles, and the sealed-delivery
/// digest (adept), a "Settlement" section with the live released / refunded binds
/// (the FLASHWELL split), and an "Actions" section of the four icon-labelled
/// sealed-escrow buttons.
///
/// Renderer-independent DATA. The button `turn` names are the
/// [`service`](crate::service) method symbols.
pub fn escrow_card_value() -> Value {
    json!({
        "kind": "vstack",
        "props": {},
        "children": [
            row(vec![text("Sealed Escrow"), pill_live(STATE_SLOT, "LISTED", "muted", json!([
                { "value": STATE_LISTED,  "label": "LISTED",  "tag": "warn" },
                { "value": STATE_FUNDED,  "label": "FUNDED",  "tag": "accent" },
                { "value": STATE_SHIPPED, "label": "SHIPPED", "tag": "accent" },
                { "value": STATE_SETTLED, "label": "SETTLED", "tag": "good" },
            ]))]),
            // The three roles on the attenuation lattice (observer ⊂ buyer ⊂ seller).
            row(vec![
                pill("observer · reads", "muted"),
                pill("buyer · funds", "accent"),
                pill("seller · settles", "good"),
            ]),
            breadcrumb(&["Listed", "Funded", "Shipped", "Settled"], 1),
            divider(),
            section("Escrow", "genuine", vec![
                // THE KILLER VISUAL: the trustline draw (escrowed bounded by the ceiling).
                gauge(ESCROWED_SLOT, CEILING_GAUGE_MAX, "escrowed vs ceiling "),
                bind(CEILING_SLOT,      "ceiling · ",  "amount", false),
                bind(ESCROWED_SLOT,     "escrowed · ", "amount", false),
                bind(SELLER_HASH_SLOT,  "seller · ",   "id",     false),
                bind(BUYER_HASH_SLOT,   "buyer · ",    "id",     false),
                // The sealed mailbox delivery digest — a dev-y bone.
                bind(DELIVERY_HASH_SLOT, "delivery · ", "hash",  true),
            ]),
            section("Settlement", "", vec![
                bind(RELEASED_SLOT, "released · ", "amount", false),
                bind(REFUNDED_SLOT, "refunded · ", "amount", false),
            ]),
            section("Actions", "", vec![
                action("⊕", "Open",    METHOD_OPEN),
                action("$", "Deposit", METHOD_DEPOSIT),
                action("✓", "Settle",  METHOD_SETTLE),
                action("↩", "Reclaim", METHOD_RECLAIM),
            ]),
        ]
    })
}

/// **The escrow-market card as serialized `deos.ui.*` JSON** — byte-for-byte the
/// `JSON.stringify(tree)` shape a `deos-view` renderer parses (via
/// `deos_view::parse_view_tree`). This is the string a host serves / embeds.
pub fn escrow_card_json() -> String {
    serde_json::to_string(&escrow_card_value()).expect("the escrow card serializes")
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
    fn the_card_is_a_vstack_with_a_header_a_live_status_pill_and_sections() {
        let card = escrow_card_value();
        assert_eq!(card["kind"], "vstack");
        let children = card["children"].as_array().expect("children");
        // header row, cap-tier row, breadcrumb, divider, Escrow, Settlement, Actions
        assert_eq!(children.len(), 7);
        let texts = of_kind(&card, "text");
        assert!(texts.iter().any(|t| t["props"]["text"] == "Sealed Escrow"));
        let titles: Vec<&str> = of_kind(&card, "section")
            .iter()
            .map(|s| s["props"]["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, vec!["Escrow", "Settlement", "Actions"]);
    }

    #[test]
    fn the_status_pill_reads_the_leg_state_live() {
        let card = escrow_card_value();
        let live_pills: Vec<&Value> = of_kind(&card, "pill")
            .into_iter()
            .filter(|p| p["props"].get("slot").is_some())
            .collect();
        assert_eq!(live_pills.len(), 1);
        assert_eq!(live_pills[0]["props"]["slot"], STATE_SLOT as u64);
        let cases = live_pills[0]["props"]["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 4, "LISTED / FUNDED / SHIPPED / SETTLED");
        assert_eq!(cases[3]["value"], STATE_SETTLED);
        assert_eq!(cases[3]["label"], "SETTLED");
    }

    #[test]
    fn the_three_cap_tiers_are_named_as_static_pills() {
        let card = escrow_card_value();
        let static_pills: Vec<String> = of_kind(&card, "pill")
            .into_iter()
            .filter(|p| p["props"].get("slot").is_none())
            .map(|p| p["props"]["text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            static_pills,
            vec!["observer · reads", "buyer · funds", "seller · settles"]
        );
    }

    #[test]
    fn the_trustline_gauge_reads_the_live_escrowed_slot() {
        let card = escrow_card_value();
        let gauges = of_kind(&card, "gauge");
        assert_eq!(gauges.len(), 1, "the trustline-draw gauge");
        assert_eq!(gauges[0]["props"]["slot"], ESCROWED_SLOT as u64);
        assert_eq!(gauges[0]["props"]["max"], CEILING_GAUGE_MAX);
    }

    #[test]
    fn the_lifecycle_breadcrumb_marks_the_current_step() {
        let card = escrow_card_value();
        let crumbs = of_kind(&card, "breadcrumb");
        assert_eq!(crumbs.len(), 1);
        let items = crumbs[0]["props"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 4, "Listed → Funded → Shipped → Settled");
        assert_eq!(items[1]["label"], "› Funded", "the active step is marked");
    }

    #[test]
    fn the_binds_surface_the_full_escrow_and_settlement_terms() {
        let card = escrow_card_value();
        let binds = of_kind(&card, "bind");
        let slots: Vec<u64> = binds
            .iter()
            .map(|b| b["props"]["slot"].as_u64().unwrap())
            .collect();
        assert_eq!(
            slots,
            vec![
                CEILING_SLOT as u64,
                ESCROWED_SLOT as u64,
                SELLER_HASH_SLOT as u64,
                BUYER_HASH_SLOT as u64,
                DELIVERY_HASH_SLOT as u64,
                RELEASED_SLOT as u64,
                REFUNDED_SLOT as u64,
            ],
            "ceiling / escrowed / seller / buyer / delivery, then released / refunded"
        );
    }

    #[test]
    fn the_party_and_amount_binds_carry_their_display_fmt_and_the_digest_is_adept() {
        let card = escrow_card_value();
        let binds = of_kind(&card, "bind");
        let find = |slot: usize| -> &Value {
            binds
                .iter()
                .find(|b| b["props"]["slot"].as_u64() == Some(slot as u64))
                .copied()
                .unwrap()
        };
        assert_eq!(find(SELLER_HASH_SLOT)["props"]["fmt"], "id");
        assert_eq!(find(BUYER_HASH_SLOT)["props"]["fmt"], "id");
        assert_eq!(find(CEILING_SLOT)["props"]["fmt"], "amount");
        assert_eq!(find(ESCROWED_SLOT)["props"]["fmt"], "amount");
        assert_eq!(find(RELEASED_SLOT)["props"]["fmt"], "amount");
        assert_eq!(find(REFUNDED_SLOT)["props"]["fmt"], "amount");
        // The sealed delivery digest paints short hex and hides in the simple view.
        assert_eq!(find(DELIVERY_HASH_SLOT)["props"]["fmt"], "hash");
        assert_eq!(
            find(DELIVERY_HASH_SLOT)["props"]["adept"],
            true,
            "the sealed delivery digest is adept-only"
        );
        // The human-meaningful terms stay visible.
        assert_eq!(find(CEILING_SLOT)["props"]["adept"], false);
        assert_eq!(find(ESCROWED_SLOT)["props"]["adept"], false);
    }

    #[test]
    fn every_button_carries_its_service_method_as_the_turn_payload() {
        let card = escrow_card_value();
        let buttons = of_kind(&card, "button");
        assert_eq!(buttons.len(), 4);
        let turns: Vec<&str> = buttons
            .iter()
            .map(|b| b["props"]["onClick"]["turn"].as_str().unwrap())
            .collect();
        assert_eq!(
            turns,
            vec![METHOD_OPEN, METHOD_DEPOSIT, METHOD_SETTLE, METHOD_RECLAIM]
        );
    }

    #[test]
    fn the_card_serializes_to_parseable_json() {
        let s = escrow_card_json();
        let back: Value = serde_json::from_str(&s).expect("the card JSON parses");
        assert_eq!(back["kind"], "vstack");
        assert_eq!(back["children"].as_array().unwrap().len(), 7);
    }
}
