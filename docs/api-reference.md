# API reference source inventory

The checked-in
[`route-registration-v1.json`](api/route-registration-v1.json) is a deterministic
inventory of syntactically registered Axum routes and static-service mounts. It is
generated from the Rust adapters and server source without executing the server.
The parser fails closed when it encounters route-registration syntax it does not
understand.

The source walk is bounded to 1,024 Rust files, 4,096 directory entries, 32
directory levels, 1 MiB per source file, 32 MiB total source, and 512 route
registrations. Exceeding a bound stops generation with a generic diagnostic.

This inventory is an engineering drift check, not an OpenAPI document or a list of
supported third-party APIs. It can contain private browser/console operations,
operational routes, static mounts, and feature-enabled/disabled alternatives. It
does not infer authentication, authorization, availability, request/response
schemas, validation bounds, or caller stability. An entry does not authorize an
integration. Until the integrated, classified API reference is published, use the
existing [provider](provider.md), [registration](registration.md),
[token-check](token-checks.md), [resource-introspection](resource-introspection.md),
[personal-key](personal-api-keys.md), and feature-specific guides for their
respective contracts.

After changing an Axum registration, regenerate and review the snapshot:

```sh
make api-inventory-generate
git diff -- docs/api/route-registration-v1.json
make api-inventory-check
```

`make check` runs `api-inventory-check`, so a route change cannot silently leave
this source inventory stale. Do not use this file to generate client code or to
decide whether an endpoint is safe or supported. Issue #43 tracks the complete
versioned OpenAPI contract and management-console reader that will add these
classifications and schemas.
