# OIDC language hints and independent authorization tabs

Clients can add `ui_locales=es-CR%20es%20en` to a standard authorization-code
request. Darkhorse selects the first supported primary language: this example
selects Spanish. A list containing only unsupported languages leaves presentation
to the ordinary fallback. This follows the ordered-hint behavior in
[OIDC Core](https://openid.net/specs/openid-connect-core-1_0.html#AuthRequest).

## Input and precedence

The decoded value may contain at most 256 ASCII bytes, 16 space-separated tags,
and 64 bytes per tag. Exactly one ASCII space separates entries; empty values,
leading/trailing/repeated spaces, malformed syntax and duplicate `ui_locales`
parameters yield the existing local `invalid_request` response. Such parsing
failures never redirect to an unvalidated callback. Every entry is checked, even
after a supported language has been found. Repeated language tags are harmless:
the first supported choice wins. Missing hints and syntactically acceptable
unsupported-only lists create no override. The enclosing request retains its
8,192-byte/32-parameter limits.

Only the resolved `en`/`es` value or `NULL` is stored. Regional details and arbitrary
client input are not retained. Migration 34 adds this nullable column without
changing existing requests, credentials or authority. The existing immutable
request trigger also protects the language column.

The shared [presentation precedence](localization.md#resolution-policy) applies:
explicit choice, saved account preference and an anonymous browser preference
precede the transaction hint. Browser negotiation and deployment defaults follow
it. A hint does not write an account or anonymous preference. Language controls
remain an explicit user choice and retain their documented persistence behavior.
Names, callback URLs, state, nonce, scopes, permission keys and token claims remain
literal protocol/application values.

## Transaction selection

Each pending flow has a distinct HttpOnly cookie named
`__Host-darkhorse-authorization-<reference>`. The value is an independently random
256-bit secret. The public reference is its purpose-separated SHA-256 digest;
knowing that reference alone cannot resume the flow.

```mermaid
sequenceDiagram
    participant A as Application
    participant B as Browser tab
    participant R as Rust provider
    participant P as PostgreSQL primary
    A->>R: Authorization request + ui_locales
    R->>P: Store immutable request, proof digest and resolved hint
    R-->>B: Per-flow HttpOnly cookie + /authorization?request=reference
    B->>R: Inspect explicit reference + matching cookie
    R->>P: Recheck current request and session authority
    R-->>B: Exact client, scopes and presentation hint
    B->>R: Explicit decision with matching page and reviewed reference
    R->>P: Recheck authority and complete existing transaction
    R-->>B: Callback result; expire only this flow's cookie
```

Both internal inspection and decision URLs require exactly
`?request=<64 lowercase hexadecimal characters>`. The decision body also retains
`request_id`; it must equal the URL selector. The provider verifies the selected
cookie's proof against that reference before reading a transaction. A different
flow's cookie, missing/duplicate proof, ambiguous query or stale body cannot
substitute another tab's request. No bearer secret enters a URL or browser storage.
The frontend rejects mismatched projections and discards late results after a page
or selector change. Its language hint is scoped to the mounted authorization page.

Cookies retain `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/` and a five-minute
lifetime. Completion expires only the selected proof. Failed terminal issuance
requires restarting from the client; the portal does not automatically retry a
mutation. Beginning a new flow rejects eight already-present flow cookies.
Concurrent starts can exceed that admission observation; parsing still enforces
an aggregate 4 KiB Cookie-header bound, and the existing database capacity/expiry
controls remain authoritative. This is not a separate persistent browser identity.

The internal portal protocol changes with this deployment. Coordinate the new
static build and server; existing old-format pending browser flows must restart
from their client. Browser authentication sessions and issued credentials are
unchanged. Apply the additive migration with the existing operator workflow.

## Discovery and validation

Discovery publishes `ui_locales_supported: ["en", "es"]`. It does not publish
`claims_locales_supported` or translate UserInfo/ID-token claims. These are
separate metadata fields in
[OIDC Discovery](https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderMetadata).

Source-defined tests cover list limits, unsupported fallback, literal protocol
fields, cookie substitution, two independent handler flows and exact cookie
expiry. PostgreSQL tests cover immutable hint storage and committed revocation;
the full browser suite adds two client tabs, reload, explicit choice, cancellation
and unsupported hints. Transport fakes and static presentation checks do not
replace the real PostgreSQL/HTTPS runs. Whole-project coverage, interoperability
and release qualification remain separate gates.

## Processing and storage cost

The resolver runs once when parsing an authorization request. Inspection and
decisions reuse the existing transaction reads; the two-byte resolved language
adds no SQL statement, cache lookup or external request. The nullable PostgreSQL
text column adds tuple representation/alignment overhead as well as the selected
tag. Actual row-size and workload measurements remain part of PostgreSQL
qualification; two payload bytes are not a claim about physical storage size.

`ui_locales=es-CR%20es%20en` adds 27 query bytes including its separating `&`.
The per-flow cookie name adds 65 bytes over the former single cookie name, and the
public `?request=` selector adds 73 bytes to each internal URL. These increases
remain inside the existing request bounds. Multiple pending cookies increase
request-header traffic until completion or expiry.

Run `cargo run --release --locked --offline -p darkhorse-adapters --example
oidc-language-benchmark` for the absent, supported, unsupported, maximum-entry and
malformed-hint computations. This in-process comparison excludes network,
authentication, database work and throughput qualification. The
[static-build measurement](measurements/oidc-language-2026-09-26.json) records
145,423 summed gzip JavaScript bytes: 36,270 above the unchanged 109,153-byte
baseline, inside the 36 KiB localization allowance. Its browser timings use
loopback fixtures and establish presentation cost only.

The [recorded resolver run](measurements/oidc-language-processing-2026-09-26.json)
uses Rust 1.97.1, the locked dependency graph, release optimization and an Apple M5
on macOS arm64. Five rotated-order samples of 100,000 calls, each preceded by
1,000 warm-up calls, give a median 167 ns for `es-CR es en` and 1.31 µs for the
16-entry fixture. Absent hints take approximately 2 ns, dominated by measurement
and call overhead. These observations do not include the authorization path.
