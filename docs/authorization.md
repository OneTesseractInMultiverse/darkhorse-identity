# Authorization contract

The `darkhorse-domain::authorization` module is a pure Rust policy engine with no external dependencies. It computes access from explicit facts. It does not authenticate passwords, tokens, or API keys, load a database, read a clock, or implement an OAuth endpoint.

## Policy vocabulary

| Definition  | Meaning                                                                                               |
| ----------- | ----------------------------------------------------------------------------------------------------- |
| Application | Administrative boundary containing protected resources and explicit role assignments.                 |
| Resource    | A protected API/audience belonging to one application; exposes a set of capabilities.                 |
| Capability  | One permission, such as reading the user directory. Its application bindings are explicit.            |
| Role        | A set of capabilities granted through an assignment within a specified application.                   |
| Scope       | A resource-specific upper bound on delegated capabilities; it cannot grant a principal new authority. |
| Client      | An OAuth client owned by an application, explicitly registered for resources and scopes.              |

Capabilities and roles may be shared definitions bound to several applications. Binding a definition does not assign it to anyone. An assignment for application A grants nothing in application B, even if the role is bound to both. Creating an application grants no existing role access to it. There are no wildcard permissions, role hierarchies, negative grants, or application-owner bypasses.

`Catalog::new` validates the complete supplied graph before returning an immutable catalog. Duplicate IDs, unknown references, roles exposing capabilities outside their application bindings, and scopes exceeding their resource's exposed capabilities are errors. Empty catalogs, unbound definitions, and empty roles are valid and confer no access. The catalog must include all definitions referenced by the supplied facts; incomplete assignments fail closed.

Persistence must never reuse identifiers or repurpose an existing capability to mean a different permission. A new meaning requires a new capability ID so that existing credential ceilings cannot silently acquire it.

## Evaluation

`effective_capabilities` accepts a catalog and an `Evaluation`: the current principal, an authenticated stored credential grant, the requested application/resource target, and current Unix time in seconds. Every grant describes exactly one target.

It checks credential subject, target, revocation, principal epoch, activation/expiry, principal status, application/resource status, and role assignments. OAuth grants additionally require a currently active client, an active owning application, and explicitly registered scopes for the requested resource. Client ownership and target application may differ when registration and principal assignments explicitly permit access.

For a personal API key:

```text
effective = current role grants ∩ resource capabilities ∩ issuance ceiling
```

For an OAuth access grant:

```text
effective = current role grants ∩ resource capabilities ∩ issuance ceiling
            ∩ capabilities of the stored, currently allowed scopes
```

Capabilities from multiple assigned roles are combined before applying the limits. Capabilities from multiple selected scopes are also combined. For example, a writer role grants `read` and `update`, but a token with only a `read` scope can exercise only `read`. A scope cannot compensate for a missing role.

`authorize` requires the requested capability to be in that computed set. An otherwise valid credential can have an empty effective set after a permission reduction; it authorizes no capability. That result is not an OAuth introspection response or a declaration that a token is active. Adapters must implement their protocol contracts separately.

Time boundaries are inclusive for `valid_from` and exclusive for `expires_at`. An expiry at or before activation is invalid. OAuth access grants require expiry. Personal keys can have no expiry; the later key-issuance use case must enforce the organization's expiration policy.

`Denial` and `CatalogError` describe internal failures. Transport adapters must map them to appropriate safe responses without exposing account or token existence. The engine accepts trusted facts, not arbitrary browser-supplied authority, and identifiers are never authentication secrets.

## Delegation and credential lifecycle

- `plan_key` snapshots all currently delegable capabilities or validates an explicit subset. Its supplied delegation limit is another upper bound and never adds authority.
- `plan_oauth` intersects current access, selected registered scopes, and the supplied consent limit. It does not establish consent itself. Protocol scopes such as `openid` and profile-claim disclosure are separate from this capability model.
- Both return an `IssuancePlan` containing subject, target, principal epoch, capability ceiling, and delegation bindings. Empty grants are rejected. They do not generate or store credentials or choose expiry.
- `attenuate` permits a nonempty subset of an existing ceiling. Refresh handling must still validate the credential, current authority, client binding, and token-family reuse before using that result.

“All access” means all eligible access at issuance. Later role or scope expansion cannot exceed the stored ceiling. A live permission reduction takes effect when the new facts are supplied. Restoring permission inside the original ceiling can restore access to an otherwise valid credential. Permanently invalidating it requires revocation or a principal epoch change.

Persistence must advance the principal's credential epoch atomically on deactivation/revoke-all and preserve that advanced value on reactivation. It must prevent epoch wraparound and never reset an individual credential's revocation flag. The pure evaluator rejects mismatched epochs; it cannot reconstruct lifecycle history from an active-status flag.

An application-bound [personal API key](personal-api-keys.md) can hold several explicit per-resource grants. Each plan is independently checked; the key lifecycle enforces the application boundary and atomic issuance/revocation. No plan implies permission for other or future resources.

## Freshness, caching, and performance

The calling use case must authenticate the credential and load coherent, current authoritative principal, credential, and policy facts. New authorization checks after a revocation or permission reduction commits must use the new state. Cached positive decisions or stale replicas cannot satisfy that requirement. This module alone does not prove database consistency or close the race between an authorization check and a later protected write.

Catalog validation happens when constructing a policy snapshot. Evaluation reuses immutable catalogs and indexed definitions; it visits supplied assignments, scopes, and capability sets without I/O. Future adapters may cache versioned computations only after fresh authoritative revision/status checks. They must bound input sizes and policy sizes, avoid rebuilding a whole organization catalog for every request, and measure the full request path before claiming throughput or latency targets.

The PostgreSQL [resource issuance adapter](resource-issuance.md) now loads a bounded
projection for the requested resource, principal assignments and registered scopes.
It calls `plan_oauth` under the primary security-state fence at consent, code
issuance and redemption. The current persistence contract restricts clients to
resources in their owning application. [Resource-server checks](resource-introspection.md)
now reuse that projection and `effective_capabilities` under the same fence.

Object-level rules, such as which particular directory record a caller may edit, remain the consuming use case's responsibility. General capabilities do not bypass those rules.

## Verification

`make test-authorization` exercises valid and malformed catalogs, successful access, denial boundaries, shared definitions, and delegation. `make test-property` exhaustively checks small sets, including 4,096 combinations of role, resource, scope, and credential ceilings, plus monotonic reduction, scope growth, subset attenuation, and cross-application isolation. All fixtures and time values are defined in test source.

`make test-mutation` checks whether altered authorization computations are detected. `make coverage-rust` reports instrumented library coverage; stable branch instrumentation and full system behavior are outside that report. Neither exhaustive small-set properties, mutations, nor coverage constitute a proof of the complete identity server.
