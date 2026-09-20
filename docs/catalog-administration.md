# Application and access-catalog administration

The management console provides separate **Applications**, **Clients**,
**Resources**, **Scopes**, **Roles**, and **Capabilities** pages. It uses the
Rust-owned authenticated portal; apply migration 0018 and run `make dev-login`.
All reads and writes require a live platform administrator. Application owners
are contacts; ownership grants no administration rights. Delegated application
administrators and separate management permissions remain unimplemented.

## Workflows

Create an application with an active user as its owner. Its detail dialog links
to the application's clients, resources, scopes, roles, and capabilities. Child
catalogs require an application selection. Tables support literal name-prefix
search, status where applicable, and cursor pagination. Forms and confirmations
remain separate from the table.

Client forms configure exact HTTPS callback URLs, explicit resource and scope
allowlists, refresh-token opt-in, and active status. Updates use the freshly read
client revision. The client ID remains stable. Credential controls rotate or
retire secrets separately; overlap is limited to 300 seconds.

A newly issued secret appears once in a dedicated dialog. Store it in the
client's secret manager before acknowledging it. Dismissal, navigation, page
hiding, or leaving the tab clears the reveal. The console does not write secrets
to URLs, browser storage, logs, or the clipboard. JavaScript does not guarantee
memory erasure; this is a bounded display lifetime, not a memory-zeroization
claim. A lost response can follow a successful commit. Refresh, locate the
client, and rotate after inspecting its current state; never retry creation or
rotation automatically. Detail and list endpoints never return a raw secret.

Capabilities have permanent permission keys and meanings. Keys use lowercase
ASCII letters, digits, `.`, `_`, `:`, and `-`, beginning with a letter or digit;
meanings hold up to 1,000 characters. Retirement is permanent and affects new
authorization checks. Role labels, resource identities, audiences, and scope
names are immutable; a role's grants can change.

The authorization graph is explicit:

- Bind shared capabilities and roles to individual applications. No binding
  covers future applications automatically.
- Grant capabilities to roles. Every capability must be bound in every
  application where the role is bound.
- Expose application capabilities through a resource.
- Include exposed resource capabilities in a scope's bounds. Scopes restrict
  requested authority; they do not grant role membership.
- Assign an application role to a user through **User directory → Access**.

Each binding edit changes one edge and requires confirmation. Remove dependent
scope, resource, role, or principal bindings before removing their prerequisite.
Rejected edits do not cascade or partially change access. Retired capabilities
cannot receive new grants through these commands. Existing token and consent
ceilings do not expand when a role acquires new capabilities.

## API and architecture

| Endpoint                                                    | Behavior                                                          |
| ----------------------------------------------------------- | ----------------------------------------------------------------- |
| `GET /api/admin/catalog/{kind}`                             | Bounded catalog page                                              |
| `GET /api/admin/catalog/{kind}/{id}`                        | Capability, role, resource, or scope with explicit bindings       |
| `POST /api/admin/catalog`                                   | One typed catalog command and expected global policy revision     |
| `GET /api/admin/console/applications/{id}`                  | Application detail                                                |
| `GET /api/admin/console/applications/{id}/clients/{client}` | Client detail and credential metadata                             |
| `POST /api/admin/console/registration`                      | Existing registration commands with string revisions in responses |

Kinds are `applications`, `clients`, `resources`, `scopes`, `roles`, and
`capabilities`. List parameters are `search`, `status`, `limit`, `after`, and
`application_id`. Clients, resources, and scopes require `application_id`.
Capabilities and roles accept it as an optional explicit-binding filter.
Status accepts `active` or `inactive` only for applications, clients, and
capabilities; inactive capabilities are retired. Resource details require
`application_id`; scope details additionally require `resource_id`.

Catalog writes supply a decimal-string `policy_revision` and a `change` object.
The operations are `create_capability`, `retire_capability`, `create_role`,
`capability_binding`, `role_binding`, `role_capability`, `resource_capability`,
and `scope_capability`. A binding command names both endpoints and its requested
boolean state. Existing [registration commands](registration.md) are reused
without duplicating client authentication or secret policy. Their inputs accept
canonical decimal-string revisions and legacy JSON integers. Legacy registration
responses retain numeric revisions; console responses use strings so JavaScript
preserves the full database counter range.

The application catalog port has no HTTP or SQL dependencies. Domain computations
validate definitions, grant eligibility, binding prerequisites, and growth limits.
The PostgreSQL adapter owns transaction boundaries; Axum owns transport parsing,
CSRF checks, and serialization. The port can be reused by a future operator CLI.

## Consistency and limits

Reads take the shared primary security fence. Changes take it exclusively and
check current session, administrator membership, recent authentication, and the
expected policy revision. Mutations require a sign-in within five minutes;
reads do not extend the session's idle timestamp. The committing transaction
rechecks authority after reference reads and locks. Registration also rechecks
recent authentication after owner/reference waits.

Every effective catalog change commits an immutable actor/session/target audit
record with its policy revision. Existing graph before/after auditing remains in
the same transaction. Failed auditing, stale revisions, missing prerequisites,
or constraint failures roll everything back. No-op binding commands create no
revision or audit record. Administrative changes share the existing global fence;
no positive authorization result is cached in Redis or the browser.

Lists return 25 records by default and at most 100. Keysets follow immutable UUID
order; they are live pages, not a snapshot export. Names are literal prefixes,
including `%`, `_`, and backslashes. Query values are bound parameters; restqs
handles the allowlisted status and limit filters. Query strings are limited to
2 KiB, bodies to 32 KiB, and each HTTP route group to 16 concurrent requests
with the existing request deadline.

Details support at most 1,000 application bindings and 256 capabilities per
role/resource/scope. Expansion past those bounds is rejected. A pre-existing
larger graph fails explicitly and requires operator review; it is never silently
truncated into an authorization decision. The separate user-role assignment
picker retains its existing 100-application/128-role limits. Resource-server
introspection credential administration remains available through its existing
API; it has no console form in this slice. An audit browser, delegated
administration, larger graph pagination, and production load qualification remain
tracked work.

## Verification

`make test-catalog` runs isolated domain, service, transport, and component
contracts. `make test-postgres` exercises real catalog pagination, explicit
bindings, audit rollback, concurrent edits, upgrades, and immediate introspection
reductions with immutable issued ceilings. `make test-browser` exercises the
static console through verified HTTPS, including registration, owner selection,
resource/scope/capability/role bindings, client updates, secret rotation and
retirement, lost responses, stale policies, denied reads, and narrow layouts.
These suites do not establish complete accessibility or production qualification.
The authored-code coverage target remains unchanged; see [engineering](engineering.md).
