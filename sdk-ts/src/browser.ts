/**
 * `@dregg/sdk/browser` — the FULL browser acting + reading surface.
 *
 * The `sdk-browser-ed25519-webcrypto` follow-up is DONE
 * (`docs/design-frontiers/WEB-FORWARD.md §8 S5`): `internal/ed25519` now backs
 * `Identity` with `@noble/ed25519` (the audited reference impl, byte-identical to
 * the old `node:crypto` path — the golden key-derivation vector pins it), so the
 * SDK no longer statically imports `node:crypto`. The `profiles` store (the only
 * remaining `fs`/`path` user) is NOT re-exported here; everything else — the
 * two-noun front door `Identity → .turn() → .sign() → .submit() → Receipt`, the
 * `NodeClient` (pure `fetch` + SSE), the `AgentRuntime`, the `Receipt` noun, and
 * the organ clients — bundles for the browser unchanged.
 *
 * So a `.turn()` from a tab against the devnet is now a REAL signed turn:
 *
 * ```ts
 * import { Identity, AgentRuntime } from "@dregg/sdk/browser";
 * const id = Identity.generate();                       // WebCrypto getRandomValues
 * const rt = new AgentRuntime(id, "https://devnet.dregg.fg-goose.online");
 * const signed = await rt.turn().transfer(targetHex, 100n).sign();
 * console.log(signed.explain());                        // anti-blind-signing reading
 * const receipt = await signed.submit();                // a real ed25519-signed turn
 * ```
 *
 * Authorization stays INESCAPABLE: there is no `Unchecked` constructor on this
 * surface (it lives behind `@dregg/sdk/raw`, the sealed escape hatch); the
 * authorization field is private to the `.sign()` flow and is always a real
 * Ed25519 signature by the time anything is submitted (#166).
 *
 * `BrowserNodeClient` remains as a fetch-only, signing-free client (the operands
 * the organ clients duck-type); it is byte-for-byte the same wire behaviour as
 * `NodeClient.getJson` / `.postJson` / `.sseStream`. STARK verification + the in-
 * tab world still come from the wasm path (`pkg/dregg_wasm`).
 */

import { TrustlineClient } from "./trustline";
import { ChannelsClient } from "./channels";

// THE FULL ACTING SURFACE (the two-noun front door) — browser-safe now that
// `internal/ed25519` is @noble-backed. Re-exported so a tab gets `Identity →
// .turn() → .sign() → .submit() → Receipt` from `@dregg/sdk/browser`.
export { Identity } from "./identity";
export { NodeClient, AgentRuntime, NodeError as NodeClientError } from "./client";
export type { NodeClientOptions } from "./client";
export { TurnBuilder, AuthorizedTurn, EmptyTurnError } from "./turns";
// The anti-blind-signing readings as values — pure, browser-safe (no key, no
// network): the SAME `Effect::hash`/`Action::hash`-tagged renderings the
// `AuthorizedTurn.explain()` flow uses, exported so a trusted-path mediator (the
// browser-extension front door) can render a per-effect plain-language approval
// of EXACTLY the term it is about to sign.
export { explainAction, explainEffect, explainTurn, renderTurn } from "./explain";
export { Receipt, TurnProof } from "./receipt";
export { NodeEvents } from "./events";
export type { ReceiptFilter } from "./events";
// The wire vocabulary as TYPES only — the value-level `Authorization::Unchecked`
// constructor stays sealed in `@dregg/sdk/raw`, never reachable from this surface.
export type { Action, AuthRequired, CapabilityRef, CellId, Effect, Turn } from "./internal/wire";
// Hex codecs — pure, browser-safe helpers a consumer needs to move 32-byte
// fields across a JSON boundary (e.g. the extension front door re-hydrating a
// page's turn description). Same functions the main `@dregg/sdk` index exports.
export { hexDecode, hexEncode } from "./internal/bytes";

// Re-export the organ clients (type-only deps on NodeClient → browser-clean).
export { TrustlineClient } from "./trustline";
export type {
  TrustlineOpened,
  TrustlineDraw,
  TrustlineRepay,
  TrustlineSettle,
  TrustlineClose,
  TrustlineStatus,
} from "./trustline";
export { ChannelsClient } from "./channels";
export type {
  MemberSpec,
  SealedEpochKey,
  ChannelStep,
  ChannelPosted,
  ChannelStatus,
  ChannelMessage,
} from "./channels";

/** Options for [`BrowserNodeClient`] — mirrors the main `NodeClientOptions`. */
export interface BrowserNodeClientOptions {
  /** The operator/devnet key for operator-gated routes (trustline/channels). */
  devnetKey?: string;
  /** Per-request timeout in ms (default 15000). */
  timeoutMs?: number;
}

/** A non-2xx node response — mirrors the main `NodeError`. */
export class NodeError extends Error {
  readonly status?: number;
  constructor(message: string, status?: number) {
    super(message);
    this.name = "NodeError";
    this.status = status;
  }
}

interface NodeIdentity {
  public_key: string;
  [k: string]: unknown;
}

/**
 * The browser-safe node client. Implements exactly the surface the organ
 * clients call — `getJson` / `postJson` / `sseStream` — plus a liveness probe
 * (`operatorPublicKeyHex`), all over `fetch`. No signing, no key material.
 */
export class BrowserNodeClient {
  readonly baseUrl: string;
  private readonly opts: BrowserNodeClientOptions;

  constructor(baseUrl: string, opts: BrowserNodeClientOptions = {}) {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
    this.opts = opts;
  }

  private headers(extra: Record<string, string> = {}): Record<string, string> {
    const h: Record<string, string> = { ...extra };
    if (this.opts.devnetKey) {
      h["X-Devnet-Key"] = this.opts.devnetKey;
      h["Authorization"] = `Bearer ${this.opts.devnetKey}`;
    }
    return h;
  }

  private async request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const resp = await fetch(this.baseUrl + path, {
      signal: AbortSignal.timeout(this.opts.timeoutMs ?? 15000),
      ...init,
      headers: this.headers((init.headers as Record<string, string>) ?? {}),
    });
    if (!resp.ok) {
      const body = await resp.text().catch(() => "");
      throw new NodeError(`HTTP ${resp.status} from ${path}: ${body.slice(0, 300)}`, resp.status);
    }
    return (await resp.json()) as T;
  }

  /** `GET {path}` → parsed JSON (the organ clients' generic read). */
  getJson<T>(path: string): Promise<T> {
    return this.request<T>(path);
  }

  /** `POST {path}` with a JSON body → parsed JSON (the organ clients' write). */
  postJson<T>(path: string, body: unknown): Promise<T> {
    return this.request<T>(path, {
      method: "POST",
      body: JSON.stringify(body),
      headers: { "Content-Type": "application/json" },
    });
  }

  /** A node SSE route as a one-shot async iterable of parsed `data:` JSON. */
  async *sseStream<T>(path: string): AsyncIterable<T> {
    const resp = await fetch(this.baseUrl + path, {
      headers: this.headers({ Accept: "text/event-stream" }),
    });
    if (!resp.ok || !resp.body) {
      const txt = resp.body ? await resp.text().catch(() => "") : "";
      throw new NodeError(`HTTP ${resp.status} from ${path}: ${txt.slice(0, 300)}`, resp.status);
    }
    const reader = resp.body.getReader();
    const decoder = new TextDecoder();
    let buf = "";
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let idx: number;
      while ((idx = buf.indexOf("\n\n")) >= 0) {
        const frame = buf.slice(0, idx);
        buf = buf.slice(idx + 2);
        for (const line of frame.split("\n")) {
          if (line.startsWith("data:")) {
            const data = line.slice(5).trim();
            if (data) yield JSON.parse(data) as T;
          }
        }
      }
    }
  }

  /**
   * The node operator's Ed25519 public key (hex). `GET /api/node/identity`,
   * falling back to `/status` for older node builds. Doubles as a liveness
   * probe. Mirrors the main `NodeClient.operatorPublicKeyHex`.
   */
  async operatorPublicKeyHex(): Promise<string> {
    try {
      return (await this.request<NodeIdentity>("/api/node/identity")).public_key;
    } catch (e) {
      if (e instanceof NodeError && e.status === 404) {
        return (await this.request<{ public_key: string }>("/status")).public_key;
      }
      throw e;
    }
  }

  /** The **trustline** organ (`docs/ORGANS.md` §1). Operator-gated. */
  trustline(): TrustlineClient {
    // The organ client only duck-types getJson/postJson; this is faithful.
    return new TrustlineClient(this as unknown as never);
  }

  /** The **channels** organ (`docs/ORGANS.md` §4). Operator-gated. */
  channels(): ChannelsClient {
    return new ChannelsClient(this as unknown as never);
  }
}
