//! **THE REFLECTIVE-COCKPIT PANE** — a desktop window whose body IS a real
//! [`deos_view::ViewNode`] (a card-as-cell), rendered through deos-view's NATIVE
//! renderer, AND the surface a confined agent reflects-on then rewrites LIVE.
//!
//! Every other desktop window is raw native gpui chrome (the NT inspector, the
//! World Explorer, the Document Explorer): the shell paints those directly. THIS
//! window proves the shell can ALSO host PORTABLE-IR content — a view-tree authored
//! as backend-independent DATA ([`deos_view::ViewNode`], the Rust mirror of the
//! `deos.ui.*` element-tree) and walked into pixels by the SAME renderer
//! ([`deos_view::AppletView`]) the card-pane / cockpit use, the SAME tree a web
//! renderer would walk into HTML ([`deos_view::web`]). One IR, two backends; the
//! native desktop is one of them.
//!
//! ## The reflective surface — a World-Status panel
//!
//! The surface hosted here is the canonical **World-Status panel** the reflective-
//! cockpit loop (`deos-view/tests/agent_reflects_and_rewrites_a_surface_live.rs`,
//! `0c3e567b`) proved over a headless render: a titled header over three live `bind`
//! rows (cells · receipts · refreshes) with a `refresh` affordance. That test proved
//! the loop over a deos-view render in isolation; THIS lifts the SAME panel source up
//! into the SHIPPED desktop window, so the loop runs against a real cockpit pane:
//!
//!   1. REFLECT-ON — the host reads the live surface's own view-tree
//!      ([`AppletView::tree`]); a confined agent's JS reads the same tree through
//!      `deos.editor.view()`. A pure read — no patch, no turn.
//!   2. REWRITE — the agent's JS adds a `refresh` button + relabels the header
//!      (`World Status` → `World Status (live)`) through `deos.editor.editView`. Each
//!      gesture is a RECEIPTED patch blamed on the agent (the proven `CardEditor`
//!      machinery), bounded by the authoring cap tooth.
//!   3. THE LIVE SURFACE RE-RENDERS — the re-folded tree is swapped into the SAME live
//!      [`AppletView`] entity the desktop window hosts ([`AppletView::set_tree`]), so
//!      the real desktop window repaints the agent's rewrite (before/after differ).
//!
//! ## Light by construction
//!
//! The static panel ([`status_panel_tree`] / [`status_panel_applet`]) needs NO mozjs:
//! it parses a fixed [`status_panel_source`] and mints an embedded verified cell. Only
//! the agent REWRITE ([`agent_rewrite_status_panel`]) reaches the SpiderMonkey
//! authoring path — and only when the loop is driven.
//!
//! Gated on `card-pane` (which pulls `deos-view` + `deos-js` via `agent-js`); the
//! desktop's window-type registration falls back to the inspector body when the
//! feature is off, so the gpui-free / headless builds still compile.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, AppContext, Entity};

use deos_js::applet::Slot;
use deos_js::card_editor::{Author, BindProps, CardEditor, TextProps, ViewTree as EditorViewTree};
use deos_js::portable::{AffordanceSpec, AppletManifest, ApplyOp, PortableApplet};
use deos_js::{Applet, JsRuntime};
use deos_view::{parse_view_tree, AppletView, SharedApplet, ViewNode};
use dregg_cell::AuthRequired;
use dregg_types::CellId;

/// The status panel's model slots — the live values its `bind` rows re-read off the
/// ledger (a status face is a function of state). `refreshes` is what the panel's
/// `refresh` affordance bumps (the affordance the agent's rewrite adds a button for).
pub const STATUS_SLOT_CELLS: Slot = 0;
pub const STATUS_SLOT_RECEIPTS: Slot = 1;
pub const STATUS_SLOT_REFRESHES: Slot = 2;

/// The witnessed values seeded into the panel's rows — so the `bind`s paint real values
/// on first paint (before any turn fires).
pub const STATUS_SEED_CELLS: u64 = 3;
pub const STATUS_SEED_RECEIPTS: u64 = 12;
pub const STATUS_SEED_REFRESHES: u64 = 0;

/// The window title shown above the IR-rendered body (the surface chrome).
pub const STATUS_PANEL_TITLE: &str = "World Status · deos_view::ViewNode";

/// The agent's blame identity — every patch the agent's rewrite lands is attributed to
/// it (the accountable-patch face: the surface rewrites are blamed on the AGENT).
pub const AGENT_AUTHOR: Author = Author(99);

/// **The reflect-then-rewrite script a confined agent runs against the live surface.**
/// (0) REFLECT-ON — read the surface's own view-tree (`deos.editor.view()`) and confirm
/// it carries the header + three status rows IT DID NOT AUTHOR; (1) REWRITE — add a
/// `refresh` button (fires the panel's real `refresh` affordance) and (2) relabel the
/// header. Returns 1 only if the reflect saw the pre-existing surface AND both rewrites
/// landed in the re-folded tree. This is the SAME script the `0c3e567b` loop proved.
pub const AGENT_REWRITE_JS: &str = r#"
    // (0) REFLECT-ON — read the live surface's OWN view-tree before touching it.
    // The agent did NOT build this surface; it is reading a real cockpit panel.
    var surface = deos.editor.view();
    var sawHeader = false, statusRows = 0;
    (function walk(n) {
        if (!n) return;
        if (n.kind === "text" && n.props && n.props.text === "World Status") sawHeader = true;
        if (n.kind === "bind") statusRows += 1;
        var kids = n.children || [];
        for (var i = 0; i < kids.length; i++) walk(kids[i]);
    })(surface);
    var reflected = sawHeader && (statusRows === 3);

    // (1) REWRITE — add a `refresh` button (fires the panel's real `refresh` affordance).
    var card = deos.editor.card();
    deos.editor.editView(card, {
        op: "addButton", label: "refresh", affordance: "refresh", arg: 1
    });
    // (2) relabel the header "World Status" -> "World Status (live)".
    var tree = deos.editor.editView(card, {
        op: "relabel", target: "World Status", text: "World Status (live)"
    });

    var hasButton = false, relabelled = false;
    (function walk(n) {
        if (!n) return;
        if (n.kind === "button" && n.props && n.props.on_click &&
            n.props.on_click.turn === "refresh") hasButton = true;
        if (n.kind === "text" && n.props && n.props.text === "World Status (live)")
            relabelled = true;
        var kids = n.children || [];
        for (var i = 0; i < kids.length; i++) walk(kids[i]);
    })(tree);
    (reflected && hasButton && relabelled) ? 1 : 0;
"#;

/// **The World-Status panel AS structured view-source** — a titled header over three
/// live `bind` rows. The exact `{kind, props, children}` JSON the renderer's
/// [`parse_view_tree`] consumes AND the card-editor's `ViewTree` authors (so the
/// agent's rewrite splices it as patches). Built via the card-editor's `ViewTree` so the
/// shipped pane and the authoring path share ONE source shape.
pub fn status_panel_source() -> String {
    let view = EditorViewTree::VStack {
        children: vec![
            EditorViewTree::Text {
                props: TextProps {
                    text: "World Status".into(),
                },
            },
            EditorViewTree::Bind {
                props: BindProps {
                    slot: STATUS_SLOT_CELLS,
                    label: "cells: ".into(),
                },
            },
            EditorViewTree::Bind {
                props: BindProps {
                    slot: STATUS_SLOT_RECEIPTS,
                    label: "receipts: ".into(),
                },
            },
            EditorViewTree::Bind {
                props: BindProps {
                    slot: STATUS_SLOT_REFRESHES,
                    label: "refreshes: ".into(),
                },
            },
        ],
    };
    view.to_json()
}

/// The panel's portable manifest — its program (seed fields + the `refresh` affordance +
/// the structured view source). The SAME manifest the static pane mints from and the
/// agent's `CardEditor` adopts (so the live surface and the authored surface agree).
pub fn status_panel_manifest() -> AppletManifest {
    AppletManifest {
        seed_fields: vec![
            (STATUS_SLOT_CELLS, STATUS_SEED_CELLS),
            (STATUS_SLOT_RECEIPTS, STATUS_SEED_RECEIPTS),
            (STATUS_SLOT_REFRESHES, STATUS_SEED_REFRESHES),
        ],
        affordances: vec![AffordanceSpec {
            name: "refresh".into(),
            required: AuthRequired::Signature,
            op: ApplyOp::AddToSlot {
                slot: STATUS_SLOT_REFRESHES,
            },
        }],
        held: AuthRequired::Signature,
        view_source: status_panel_source(),
    }
}

/// Parse the panel's view-source into the typed [`ViewNode`] the NATIVE renderer
/// ([`AppletView`]) and the WEB renderer ([`deos_view::web`]) BOTH consume.
pub fn status_panel_tree() -> ViewNode {
    parse_view_tree(&status_panel_source()).expect("the World-Status panel source must parse")
}

/// Mint the panel's backing applet on a fresh EMBEDDED verified executor (no mozjs): one
/// sovereign cell seeded with the witnessed status values, carrying the `refresh`
/// affordance. A rendered `bind` reads a slot off this cell (a witnessed read); the
/// agent-added `refresh` button fires `refresh` = ONE cap-gated verified turn that bumps
/// the `refreshes` slot.
pub fn status_panel_applet() -> Applet {
    // A deterministic identity for the desktop's World-Status panel cell.
    let public_key = [0x57u8; 32]; // 'W' — World-Status
    let token_id = [0x1du8; 32];
    PortableApplet::mint(public_key, token_id, &status_panel_manifest())
}

/// **Build the World-Status pane** — a [`deos_view::AppletView`] gpui entity over the
/// panel's [`ViewNode`] backed by the embedded [`status_panel_applet`]. This IS
/// deos-view's native renderer; the desktop hosts the returned entity as a window body
/// (exactly as `dock::card_surface` hosts a `CardPane`), so a desktop window's body
/// becomes the reflective World-Status surface.
pub fn build_viewnode_view(cx: &mut App) -> Entity<AppletView> {
    let applet: SharedApplet = Rc::new(RefCell::new(status_panel_applet()));
    let tree = status_panel_tree();
    cx.new(|_cx| AppletView::new(applet, tree))
}

/// Render the SAME panel tree through the WEB renderer (HTML) — the renderer-
/// independence proof the bake asserts beside the live native pane. The web renderer
/// walks the IDENTICAL [`ViewNode`] [`build_viewnode_view`] hands the native renderer.
pub fn status_panel_html() -> String {
    let tree = status_panel_tree();
    deos_view::web::render_card_document(
        STATUS_PANEL_TITLE,
        &tree,
        &[
            STATUS_SEED_CELLS,
            STATUS_SEED_RECEIPTS,
            STATUS_SEED_REFRESHES,
        ],
    )
}

/// REFLECT-ON (host side) — walk a rendered [`ViewNode`] and report whether it carries
/// the `World Status` header and how many `bind` rows it has. The bake reads this off the
/// LIVE pane entity to prove the surface the agent reflects-on is the real one.
pub fn reflect_status(tree: &ViewNode) -> (bool, usize) {
    fn walk(n: &ViewNode, saw_header: &mut bool, rows: &mut usize) {
        match n {
            ViewNode::Text(s) if s == "World Status" => *saw_header = true,
            ViewNode::Bind { .. } => *rows += 1,
            ViewNode::VStack(kids)
            | ViewNode::Row(kids)
            | ViewNode::List(kids)
            | ViewNode::Table(kids) => {
                for k in kids {
                    walk(k, saw_header, rows);
                }
            }
            _ => {}
        }
    }
    let mut saw_header = false;
    let mut rows = 0;
    walk(tree, &mut saw_header, &mut rows);
    (saw_header, rows)
}

/// Walk a rendered [`ViewNode`] looking for the agent's `refresh` button.
pub fn tree_has_refresh_button(tree: &ViewNode) -> bool {
    match tree {
        ViewNode::Button { turn, .. } => turn == "refresh",
        ViewNode::VStack(kids)
        | ViewNode::Row(kids)
        | ViewNode::List(kids)
        | ViewNode::Table(kids) => kids.iter().any(tree_has_refresh_button),
        _ => false,
    }
}

/// The outcome of a confined agent reflecting-on + rewriting the World-Status surface.
pub struct RewriteResult {
    /// The agent's re-folded view-tree (a renderer re-paints it).
    pub after_tree: ViewNode,
    /// How many receipted provenance turns the rewrite gestures committed (addButton +
    /// relabel = 2).
    pub receipt_count: usize,
    /// Whether the surface's blame attributes the rewrites to the AGENT (accountable).
    pub blamed_agent: bool,
    /// Whether the re-folded tree carries the agent's `refresh` button.
    pub after_has_button: bool,
}

/// **Run the confined agent's reflect-then-rewrite loop over the World-Status panel** —
/// the proven `0c3e567b` machinery. Adopt the panel's manifest into a [`CardEditor`]
/// (held `None`, the panel's authoring needs `Signature` — but the panel is mounted as a
/// single-custody surface here, so authoring is admitted; the cap tooth still runs), run
/// [`AGENT_REWRITE_JS`] in real SpiderMonkey against it (the agent reads the surface, then
/// rewrites it as receipted patches), and hand back the re-folded tree + accountability.
///
/// SpiderMonkey is a process-global thread-bound singleton (its engine is one-shot per
/// process), so the runtime is created ONCE by the caller and threaded in — the same
/// `rt` drives the rewrite AND the [`world_board_editor`] compose loop.
pub fn agent_rewrite_status_panel(rt: &mut JsRuntime) -> Result<RewriteResult, String> {
    let manifest = status_panel_manifest();
    let card = status_panel_applet();
    // The editor holds `Signature`, which satisfies the panel's authoring authority
    // (`Signature`) — the cap tooth still runs before each gesture (an over-reach is
    // refused in-band; the `0c3e567b` test proves the refusal arm).
    let editor = CardEditor::adopt(
        card,
        manifest,
        AGENT_AUTHOR,
        AuthRequired::Signature,
        AuthRequired::Signature,
    );

    let (result, editor) = rt.run_authoring(editor, AGENT_REWRITE_JS)?;
    if result != Some(1) {
        return Err(format!(
            "the agent's reflect-then-rewrite run did not complete (returned {result:?})"
        ));
    }

    let after_source = editor.view_source();
    let after_tree = parse_view_tree(&after_source)?;
    let receipt_count = editor.card().receipt_count();
    let blamed_agent = editor.view_blame().iter().any(|l| l.author == AGENT_AUTHOR);
    let after_has_button = tree_has_refresh_button(&after_tree);

    Ok(RewriteResult {
        after_tree,
        receipt_count,
        blamed_agent,
        after_has_button,
    })
}

// ══════════════════════════════════════════════════════════════════════════════════
//  THE WORLD BOARD — the agent as a CO-AUTHOR of the live cockpit
// ══════════════════════════════════════════════════════════════════════════════════
//
// The `0c3e567b`/`b18447aa` loop proved the agent can REWRITE one pre-existing surface
// (the World-Status panel). This goes deeper: the agent COMPOSES a BRAND-NEW cockpit
// surface — a "World Board" — FROM AN EMPTY ROOT, informed by reading the live World,
// and the board is mounted as a REAL second `viewnode_pane` desktop window. The agent
// stops being an editor of the cockpit and becomes a co-author OF it:
//
//   1. REFLECT-ON  — the agent reads its OWN surface (`deos.editor.view()`) and finds a
//      bare, empty root: it is about to compose from nothing, not tweak an existing pane.
//   2. READ THE WORLD — it crawls the live ledger (`deos.world.cells()`) to DECIDE what
//      to surface (a witnessed read; confers no authority).
//   3. COMPOSE — from the empty root it authors every node: a title, three LIVE
//      state-bound rows (`addBind` — the new authoring primitive), and a `refresh`
//      button. Each gesture is a receipted patch blamed on the agent, cap-toothed.
//   4. MOUNT — the composed tree is painted by the SAME native renderer into a new
//      desktop window (a distinct `ViewNodePane`): the agent ADDED a cockpit surface.

/// The board card's model slots — the live values its `bind` rows surface (a status
/// face is a function of state). The host seeds these from its live-World read.
pub const BOARD_SLOT_CELLS: Slot = 0;
pub const BOARD_SLOT_RECEIPTS: Slot = 1;
pub const BOARD_SLOT_SUM: Slot = 2;
/// What the board's `refresh` affordance bumps (kept off the surfaced stat slots).
pub const BOARD_SLOT_REFRESHES: Slot = 3;

/// The window key the desktop hosts the agent-composed board under — a deterministic,
/// distinct cell so the board opens as its OWN `ViewNodePane` window (beside the
/// World-Status pane keyed on the user cell). Not the board applet's internal cell; it
/// is the window/HashMap key + the marker [`is_world_board`] reads.
pub fn world_board_window_cell() -> CellId {
    CellId::from_bytes([0x42u8; 32])
}

/// Whether `cell` keys the agent-composed World Board window (drives the pane header).
pub fn is_world_board(cell: &CellId) -> bool {
    cell == &world_board_window_cell()
}

/// **The agent's compose-from-scratch script.** (0) REFLECT-ON — read the agent's own
/// surface and confirm it is a bare EMPTY root (composing from nothing); (1) READ THE
/// WORLD — crawl the live ledger's real cells; (2) COMPOSE — from the empty root, author
/// a title + three live state-bound rows (`addBind`) + a `refresh` button. Returns the
/// crawled live-cell count on full success (a positive witness the host cross-checks
/// against its own ledger read), else 0. Each `editView` is a receipted, blamed,
/// cap-toothed patch — the SAME proven `CardEditor` machinery, now building a new surface.
pub const WORLD_BOARD_COMPOSE_JS: &str = r#"
    // (0) REFLECT-ON — the agent reads its OWN surface first: a bare, EMPTY root.
    var surface = deos.editor.view();
    var startedEmpty = surface && surface.kind === "vstack" &&
        (!surface.children || surface.children.length === 0);

    // (1) READ THE LIVE WORLD — crawl the REAL ledger to DECIDE what to surface.
    var n = deos.world.cells().length;

    // (2) COMPOSE A FRESH SURFACE FROM THE EMPTY ROOT — the agent authors every row of
    //     its OWN cockpit board: a title, three live state-bound rows, a refresh button.
    var card = deos.editor.card();
    deos.editor.editView(card, { op: "addText", text: "World Board" });
    deos.editor.editView(card, { op: "addBind", slot: 0, label: "live cells: " });
    deos.editor.editView(card, { op: "addBind", slot: 1, label: "receipts: " });
    deos.editor.editView(card, { op: "addBind", slot: 2, label: "conservation Σ: " });
    var tree = deos.editor.editView(card, {
        op: "addButton", label: "refresh", affordance: "refresh", arg: 1
    });

    // (3) WITNESS — the empty surface now carries the agent's composed board.
    var titled = false, binds = 0, hasButton = false;
    (function walk(node) {
        if (!node) return;
        if (node.kind === "text" && node.props && node.props.text === "World Board") titled = true;
        if (node.kind === "bind") binds += 1;
        if (node.kind === "button" && node.props && node.props.on_click &&
            node.props.on_click.turn === "refresh") hasButton = true;
        var kids = node.children || [];
        for (var i = 0; i < kids.length; i++) walk(kids[i]);
    })(tree);

    (startedEmpty && n >= 1 && titled && binds === 3 && hasButton) ? n : 0;
"#;

/// The board card's portable manifest — an **EMPTY root view** (the agent composes every
/// node from scratch), its three stat slots seeded from the host's live-World read, and a
/// `refresh` affordance the agent's composed button fires.
pub fn world_board_manifest(cells: u64, receipts: u64, sum: u64) -> AppletManifest {
    AppletManifest {
        seed_fields: vec![
            (BOARD_SLOT_CELLS, cells),
            (BOARD_SLOT_RECEIPTS, receipts),
            (BOARD_SLOT_SUM, sum),
            (BOARD_SLOT_REFRESHES, 0),
        ],
        affordances: vec![AffordanceSpec {
            name: "refresh".into(),
            required: AuthRequired::Signature,
            op: ApplyOp::AddToSlot {
                slot: BOARD_SLOT_REFRESHES,
            },
        }],
        held: AuthRequired::Signature,
        // The agent composes the whole tree — the surface starts as a bare vstack.
        view_source: EditorViewTree::root().to_json(),
    }
}

/// Mint the board's backing applet on a fresh embedded verified executor, seeded with the
/// live-World stats its `bind` rows surface.
pub fn world_board_applet(cells: u64, receipts: u64, sum: u64) -> Applet {
    let public_key = [0x42u8; 32]; // 'B' — the World Board
    let token_id = [0x0bu8; 32];
    PortableApplet::mint(
        public_key,
        token_id,
        &world_board_manifest(cells, receipts, sum),
    )
}

/// Adopt a fresh, empty board card for authoring under the agent's identity (held
/// `Signature`, the cap tooth runs before each compose gesture).
pub fn world_board_editor(cells: u64, receipts: u64, sum: u64) -> CardEditor {
    let manifest = world_board_manifest(cells, receipts, sum);
    let card = world_board_applet(cells, receipts, sum);
    CardEditor::adopt(
        card,
        manifest,
        AGENT_AUTHOR,
        AuthRequired::Signature,
        AuthRequired::Signature,
    )
}

/// Count the live `bind` rows in a rendered board tree (the agent's composed state rows).
pub fn count_bind_rows(tree: &ViewNode) -> usize {
    fn walk(n: &ViewNode, rows: &mut usize) {
        if let ViewNode::Bind { .. } = n {
            *rows += 1;
        }
        match n {
            ViewNode::VStack(kids)
            | ViewNode::Row(kids)
            | ViewNode::List(kids)
            | ViewNode::Table(kids) => {
                for k in kids {
                    walk(k, rows);
                }
            }
            _ => {}
        }
    }
    let mut rows = 0;
    walk(tree, &mut rows);
    rows
}

/// Whether a rendered board tree carries the agent's `World Board` title text.
pub fn tree_has_board_title(tree: &ViewNode) -> bool {
    fn walk(n: &ViewNode) -> bool {
        match n {
            ViewNode::Text(s) => s == "World Board",
            ViewNode::VStack(kids)
            | ViewNode::Row(kids)
            | ViewNode::List(kids)
            | ViewNode::Table(kids) => kids.iter().any(walk),
            _ => false,
        }
    }
    walk(tree)
}

/// **Build the agent-composed World Board pane** — a [`deos_view::AppletView`] gpui entity
/// over the agent's composed `tree`, backed by a board applet seeded with the live-World
/// stats the `bind` rows surface. The desktop hosts the returned entity as a new window
/// body (a distinct `ViewNodePane`), so the agent's from-scratch composition reaches the
/// glass through the SAME native renderer the World-Status pane uses.
pub fn build_board_view(
    cx: &mut App,
    cells: u64,
    receipts: u64,
    sum: u64,
    tree: ViewNode,
) -> Entity<AppletView> {
    let applet: SharedApplet = Rc::new(RefCell::new(world_board_applet(cells, receipts, sum)));
    cx.new(|_cx| AppletView::new(applet, tree))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_manifest_starts_empty_and_seeds_live_stats() {
        // The board's authoring surface begins as a bare empty root — the agent composes
        // every node — and its model carries the host's live-World read.
        let tree = parse_view_tree(&world_board_manifest(7, 21, 0).view_source)
            .expect("the board's empty-root view-source parses");
        assert!(
            matches!(&tree, ViewNode::VStack(kids) if kids.is_empty()),
            "a fresh board is a bare empty vstack (nothing authored yet)"
        );
        let mut applet = world_board_applet(7, 21, 0);
        assert_eq!(applet.get_u64(BOARD_SLOT_CELLS), 7);
        assert_eq!(applet.get_u64(BOARD_SLOT_RECEIPTS), 21);
        // The `refresh` affordance the agent's composed button fires is a cap-gated turn.
        applet.fire("refresh", 1).expect("refresh commits");
        assert_eq!(applet.get_u64(BOARD_SLOT_REFRESHES), 1);
    }

    #[test]
    fn add_bind_composes_a_live_row_from_scratch() {
        // The new `AddBind` authoring primitive: from an empty editor, an agent can
        // compose a state-bound row (not just a frozen text snapshot).
        use deos_js::card_editor::ViewPatch;
        let mut editor = world_board_editor(3, 12, 0);
        editor
            .edit_view(ViewPatch::AddText {
                text: "World Board".into(),
            })
            .expect("compose the title");
        editor
            .edit_view(ViewPatch::AddBind {
                slot: BOARD_SLOT_CELLS,
                label: "live cells: ".into(),
            })
            .expect("compose a live bind row");
        let tree = parse_view_tree(&editor.view_source()).expect("composed source parses");
        assert!(tree_has_board_title(&tree), "the title was composed");
        assert_eq!(
            count_bind_rows(&tree),
            1,
            "one live state-bound row composed"
        );
    }

    #[test]
    fn panel_source_parses_to_the_expected_shape() {
        // The panel's view-source parses to a vstack: a header text over 3 bind rows.
        match status_panel_tree() {
            ViewNode::VStack(children) => {
                assert_eq!(children.len(), 4, "header + 3 status rows");
                assert!(matches!(&children[0], ViewNode::Text(t) if t == "World Status"));
                let (saw_header, rows) = reflect_status(&status_panel_tree());
                assert!(
                    saw_header && rows == 3,
                    "the reflective surface: header + 3 binds"
                );
            }
            other => panic!("expected a vstack root, got {other:?}"),
        }
    }

    #[test]
    fn backing_applet_reads_seeds_and_fires_the_refresh_turn() {
        let mut applet = status_panel_applet();
        assert_eq!(applet.get_u64(STATUS_SLOT_CELLS), STATUS_SEED_CELLS);
        assert_eq!(applet.get_u64(STATUS_SLOT_RECEIPTS), STATUS_SEED_RECEIPTS);
        // The `refresh` affordance the agent's button fires = one cap-gated verified turn.
        applet
            .fire("refresh", 1)
            .expect("the refresh affordance must commit");
        assert_eq!(
            applet.get_u64(STATUS_SLOT_REFRESHES),
            STATUS_SEED_REFRESHES + 1
        );
    }

    #[test]
    fn the_web_renderer_renders_the_same_panel() {
        let html = status_panel_html();
        assert!(html.contains("World Status"));
        assert!(
            html.contains("receipts: 12"),
            "the seeded status rows paint their witnessed values"
        );
    }
}
