/**
 * The MV3 background service-worker — the holder of the dregg `Identity` and the
 * trusted-path mediator. The key lives ONLY here; it is never sent to a content
 * script or a page.
 *
 *   content-script  --port("dregg")-->  background (this)
 *                                            |
 *                                       TrustedPathMediator (holds the Identity)
 *                                            |
 *                                       approval popup (the user reads explain())
 *                                            |
 *   content-script  <--port("dregg")--  background (.sign().submit() → Receipt)
 *
 * A page's `dregg:turn` request is build+signed by the mediator, the faithful
 * reading is shown in the popup, and the turn is submitted ONLY after the user
 * approves. `dregg:identity` / `dregg:isConnected` are unrestricted reads that
 * never touch key material. Authorization stays inescapable (the mediator builds
 * exclusively through `@dregg/sdk`'s authorized builder — no `Unchecked` path).
 */

import { AgentRuntime, Identity } from "@dregg/sdk/browser";
import { TrustedPathMediator, TurnDeclinedError, receiptView } from "./mediator";
import type { ApprovalView } from "./protocol";
import type { ProviderRequest, ProviderResponse } from "./protocol";
import { RESTRICTED_METHODS, UNRESTRICTED_METHODS } from "./protocol";

// ── configuration (stored; defaults to the public devnet) ────────────────────

const DEFAULT_NODE_URL = "https://devnet.dregg.fg-goose.online";

interface StoredConfig {
  nodeUrl: string;
  devnetKey?: string;
  /** 32-byte Ed25519 seed (hex) — the key at rest. Generated on first run. */
  seedHex: string;
  label?: string;
}

const hexEncode = (b: Uint8Array): string => {
  let s = "";
  for (const x of b) s += x.toString(16).padStart(2, "0");
  return s;
};
const hexDecode = (h: string): Uint8Array => {
  const out = new Uint8Array(h.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
  return out;
};

/**
 * Load the stored config, generating a fresh 32-byte seed on first run. NOTE:
 * this front-door build keeps the seed in `chrome.storage.local` for the demo
 * flow; the production hardening (BIP39 phrase + PBKDF2 + AES-256-GCM at rest,
 * auto-lock) is the same shape the wasm cipherclerk already ships and is the one
 * named seam here — the security PROPERTY this slice proves is the trusted-path
 * mediation (key never reaches the page), not the at-rest encryption.
 */
async function loadConfig(): Promise<StoredConfig> {
  const got = (await chrome.storage.local.get("dregg_front_door")) as {
    dregg_front_door?: StoredConfig;
  };
  if (got.dregg_front_door?.seedHex) return got.dregg_front_door;
  const seed = new Uint8Array(32);
  crypto.getRandomValues(seed);
  const cfg: StoredConfig = { nodeUrl: DEFAULT_NODE_URL, seedHex: hexEncode(seed), label: "my dregg identity" };
  await chrome.storage.local.set({ dregg_front_door: cfg });
  return cfg;
}

let mediatorPromise: Promise<TrustedPathMediator> | undefined;

/** The single mediator instance (lazily built; one Identity per worker). */
function getMediator(): Promise<TrustedPathMediator> {
  if (!mediatorPromise) {
    mediatorPromise = (async () => {
      const cfg = await loadConfig();
      const identity = Identity.fromKeyBytes(hexDecode(cfg.seedHex));
      const runtime = new AgentRuntime(identity, cfg.nodeUrl, { devnetKey: cfg.devnetKey });
      return new TrustedPathMediator(runtime, { label: cfg.label });
    })();
  }
  return mediatorPromise;
}

// ── approval: open the popup, await the user's verdict ────────────────────────

interface PendingApproval {
  view: ApprovalView;
  resolve: (approved: boolean) => void;
}
const pendingApprovals = new Map<string, PendingApproval>();

/**
 * Show the approval popup for `view` and resolve when the user approves or
 * declines. Opens the action popup (or a windowed fallback) carrying the pending
 * id; `popup.ts` reads `dregg:getPending`, renders the reading, and posts back
 * `dregg:approvalResult`.
 */
function requestApproval(view: ApprovalView): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    const pendingId = crypto.randomUUID();
    pendingApprovals.set(pendingId, { view, resolve });
    const url = chrome.runtime.getURL(`popup.html?pending=${pendingId}`);
    // Prefer a focused approval window so the act is deliberate and visible.
    if (chrome.windows?.create) {
      chrome.windows.create({ url, type: "popup", width: 420, height: 600 }).catch(() => {
        chrome.tabs?.create({ url });
      });
    } else {
      chrome.tabs?.create({ url });
    }
  });
}

// ── the page provider port (content-script <-> background) ────────────────────

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "dregg") return;
  // The content script stamps the verified page origin on each message; a page
  // cannot forge it (it is read from `sender`/`location` in the content script,
  // not from page-controlled data).
  port.onMessage.addListener(async (msg: ProviderRequest & { _origin?: string }) => {
    const respond = (r: ProviderResponse): void => port.postMessage(r);
    try {
      const method = msg.type;
      const origin = typeof msg._origin === "string" ? msg._origin : "(unknown)";

      if (UNRESTRICTED_METHODS.has(method)) {
        const mediator = await getMediator();
        if (method === "dregg:identity") {
          respond({ id: msg.id, ok: true, result: mediator.identityView() });
        } else {
          respond({ id: msg.id, ok: true, result: { connected: true } });
        }
        return;
      }

      if (RESTRICTED_METHODS.has(method)) {
        if (method === "dregg:turn") {
          if (!msg.spec) {
            respond({ id: msg.id, ok: false, error: "dregg:turn requires a spec" });
            return;
          }
          const mediator = await getMediator();
          // build → sign → approval popup → submit (iff approved). The key never
          // leaves this worker; only a committed receipt goes back to the page.
          const receipt = await mediator.handleTurnFromOrigin(msg.spec, origin, requestApproval);
          respond({ id: msg.id, ok: true, result: receiptView(receipt) });
          return;
        }
      }

      respond({ id: msg.id, ok: false, error: `method "${method}" is not available from a page` });
    } catch (e) {
      if (e instanceof TurnDeclinedError) {
        respond({ id: msg.id, ok: false, error: "declined: the user did not approve this turn" });
      } else {
        respond({ id: msg.id, ok: false, error: e instanceof Error ? e.message : String(e) });
      }
    }
  });
});

// ── popup <-> background (approval handshake + identity for the popup) ─────────

chrome.runtime.onMessage.addListener((msg: { type: string; pendingId?: string; approved?: boolean }, _sender, sendResponse) => {
  if (msg.type === "dregg:getPending" && msg.pendingId) {
    const p = pendingApprovals.get(msg.pendingId);
    sendResponse(p ? { ok: true, view: p.view } : { ok: false, error: "no such pending approval" });
    return true;
  }
  if (msg.type === "dregg:approvalResult" && msg.pendingId) {
    const p = pendingApprovals.get(msg.pendingId);
    if (p) {
      pendingApprovals.delete(msg.pendingId);
      p.resolve(msg.approved === true);
    }
    sendResponse({ ok: true });
    return true;
  }
  if (msg.type === "dregg:popupIdentity") {
    getMediator()
      .then((m) => sendResponse({ ok: true, view: m.identityView() }))
      .catch((e) => sendResponse({ ok: false, error: String(e) }));
    return true; // async
  }
  return false;
});
