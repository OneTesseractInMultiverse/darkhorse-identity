# Personal API keys

An authenticated user can create, list, inspect, and revoke personal credentials at `/security/keys`. These credentials represent that user. They are not independent service identities, OAuth clients, browser sessions, or OIDC identity tokens.

## Permission contract

Each key belongs to one application and contains 1–16 explicit resource grants. Every grant has an immutable ceiling of 1–256 capability identifiers. Users may select an exact subset or all current eligible permissions for each selected resource. All snapshots the current grant; it never includes future capabilities or resources.

Eligible permissions come from the user's assigned application roles, intersected with the resource's exposed, nonretired capabilities. This initial delegation policy permits those resource capabilities to be delegated. Platform-administrator membership and application ownership grant no implicit resource access. OAuth scopes are not attached to personal keys. Separate policy for nondelegable capabilities remains a future extension.

An authenticated resource server receives the intersection of current user permissions and the key's original ceiling. Removing a role grant or resource exposure takes effect on the next check after the transaction commits. Restoring a permission within the ceiling can restore access for a still-valid key. Key revocation and principal credential-epoch changes are terminal: account reactivation cannot revive an old key. Ordinary browser sign-out does not revoke personal keys.

## Issuance policy and limits

The migration establishes a deployment-wide policy in PostgreSQL:

| Setting                     | Initial value                                                |
| --------------------------- | ------------------------------------------------------------ |
| Default expiration          | 30 days                                                      |
| Maximum selected expiration | 365 days                                                     |
| Allow no expiration         | Yes                                                          |
| Maximum live keys per user  | 100                                                          |
| Creation budget per user    | 10 keys per ten minutes, including subsequently revoked keys |

Expiration is calculated from the primary database clock at issuance. The optional no-expiration choice remains explicit in the form. The persisted policy supports defaults and maximums from 1–3650 days and can prohibit new nonexpiring keys. Policy changes advance the common revision and affect future issuance; they do not silently change already issued lifetimes. A public policy-management workflow is not implemented yet. Operators must not use application credentials for ad hoc schema or policy administration.

Creation and revocation require a live original browser session and authentication within five minutes. Listing requires a live session. Management never accepts an API key or a caller-supplied owner identity as the actor. CSRF and exact-origin checks protect mutations. Requests are size bounded, concurrent handlers are limited, and database operations use the existing deadlines.

## Transactions and credential storage

Creation checks authority before generating secret material. The final transaction acquires the exclusive primary security fence and repeats session, policy revision, issuance-budget, and capability checks. It then commits the credential, metadata, verifier, resource ceilings, and actor-bound audit record atomically. A database constraint rejects incomplete issuance. Original grants are sealed after the issuance audit and cannot be changed or appended later.

Keys contain 256 bits from the operating system's random generator, encoded as `dk_` followed by 64 lowercase hexadecimal characters. Their public credential ID is independently generated. PostgreSQL stores a domain-separated SHA-256 verifier with an explicit scheme version; the raw secret never crosses the storage port. The initial key response reveals the secret once, after commit. Read APIs contain neither secrets nor verifiers. The browser clears the displayed secret on acknowledgement, navigation, hiding, and component teardown. It does not persist it in web storage or copy it automatically.

If a response is lost, issuance may already have committed. Clients must refresh the key list, identify and revoke the unwanted key, and create a replacement. They must not automatically retry creation. Revocation is owner-scoped and idempotent; audit failure rolls it back with the credential change.

## HTTP interfaces

| Endpoint                         | Purpose                                                        |
| -------------------------------- | -------------------------------------------------------------- |
| `GET /api/security/keys`         | Owner-only metadata, 25 records per page                       |
| `GET /api/security/keys/options` | Current eligible resources, capabilities, policy, and revision |
| `POST /api/security/keys`        | Create a key with an explicit revision and resource selections |
| `POST /api/security/keys/revoke` | Revoke a key by its public `key_id`                            |

Both GET endpoints accept one optional canonical UUID `after` cursor. They use stable ascending identifier order, not creation-time order. Permission pages must be refreshed if their revisions differ. Mutation payloads reject unknown fields. Creation accepts at most 256 KiB; both management route groups allow eight concurrent requests per process. Metadata responses use `Cache-Control: no-store`.

Example creation body, using actual identifiers from the options response:

```json
{
  "name": "Reporting worker",
  "application_id": "00000000-0000-0000-0000-000000000001",
  "policy_revision": "42",
  "expiration": { "kind": "days", "days": 30 },
  "grants": [
    {
      "resource_id": "00000000-0000-0000-0000-000000000002",
      "selection": {
        "kind": "subset",
        "capabilities": ["00000000-0000-0000-0000-000000000003"]
      }
    }
  ]
}
```

Expiration may also be `{ "kind": "default" }` or `{ "kind": "never" }`; a resource selection may be `{ "kind": "all" }`. Revisions are decimal strings to preserve 64-bit precision. Success returns HTTP 201 with `key` metadata and a `secret`. Revocation success returns HTTP 204.

## Resource-server integration

Enable the existing provider configuration and register a [resource introspection credential](resource-introspection.md). A protected service receives the personal key as a bearer credential and sends it in the form body to `/introspect`, authenticating with its own `rs_<resource UUID>` Basic credential over verified TLS. Never send the personal key as the introspection client's Basic secret or in a URL.

An active response contains `active`, `token_type`, `credential_type: personal_key`, `iss`, `sub`, one resource `aud`, `iat`, and the effective `capabilities`. `exp` is present only for expiring keys. It contains no OAuth `client_id`, scopes, or user profile. Another resource, an OAuth client credential, a revoked key, or a key with no current capabilities cannot obtain an active result. A resource must check its audience and the capability required by the operation.

Every introspection reads the primary under the shared security fence and rechecks the resource credential lifetime after policy reads. Dependency failure returns an unavailable error; callers must deny access. Do not cache positive authorization decisions when using the strict post-commit revocation contract. Issuance and introspection share the existing pure authorization engine; no second role hierarchy or Redis authority copy is introduced.

## Usage data, retention, and qualification

The initial lifecycle does not collect per-key request counts or last-used timestamps. Introspection performs no synchronous usage or audit writes. This avoids a write bottleneck on frequently used keys and avoids presenting missing telemetry as “never used.” Metadata and immutable lifecycle audit records are retained; a bounded retention and erasure workflow is still required before production qualification.

The proposed usage extension is optional, approximate, and separate from authority: a bounded in-process queue, minute-level Redis aggregation in the cache deployment, finite TTL, and batched durable summaries. It must drop telemetry on overload or cache failure, never include raw secrets, never decide authorization, and define retention and loss semantics before a “last used” field appears. Implementation and load evidence remain tracked in [issue #17](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/17).

`make test-personal-keys` runs isolated policy, service, HTTP, and component tests. `make test-postgres` exercises real storage, rollback, and concurrent authority changes. `make test-browser` exercises verified HTTPS, creation/revocation, authenticated introspection, response loss, and responsive UI. These are functional evidence, not a production capacity, multi-host failover, complete accessibility, or full coverage claim. See [engineering rules](engineering.md) and the open release-qualification issues.
