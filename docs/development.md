# Development

## Commands and prerequisites

`make help` lists every implemented target without provisioning anything. `make doctor` reports tool versions. `make deps-install` fetches locked Cargo and pnpm dependencies; it does not install system tools. `make check` runs formatting, lint, types, architecture checks, and all isolated tests. `make ci` also builds the release binary and static console.

`make test-unit-rust TEST_FILTER=configuration` selects Rust tests. `make test-unit-web TEST_FILTER=health` selects a frontend test file. `make test-unit-watch` watches frontend tests. `make dev-api` reloads Rust after source changes; a compile error stops the stack with an error so it cannot look healthy while running stale code.

`make test-authorization` runs the pure authorization suite. `make test-property` selects its deterministic exhaustive set properties; both are also included in the ordinary unit suite. For mutation testing, install the optional tool with `cargo install cargo-mutants --version 27.1.0 --locked`, then run `make test-mutation`. It tests authorization source mutations in temporary copies, with locked offline Cargo commands and an unmodified baseline first. Reports go under `target/mutation/mutants.out`; use `MUTATION_JOBS=1` to reduce concurrency. A missed mutation fails the command and needs investigation; it must not be hidden by a coverage percentage. The default unit targets do not require this tool.

Commands may override `NODE`, `PNPM`, and `CADDY`. The development supervisor also invokes `pnpm` from PATH. Paths containing spaces must be quoted by the shell. No machine-specific executable path is stored in the repository.

## Run the password portal

With dependencies installed, first-time local setup is:

```sh
make db-setup redis-setup login-setup https-setup
make db-up redis-up db-migrate
make bootstrap
make limiter-fence
make limiter-status
# Wait the full recovery interval shown by status (at least 904 seconds).
make limiter-activate
make dev-login
```

`make bootstrap` interactively asks for your administrator's profile and password.
It only initializes an empty deployment; it does not reset an existing account.
Use your own credentials and keep them out of command arguments and shell history.
The setup commands preserve existing secrets. Do not delete a bound login key to
reset login limits.

Open **https://localhost:8443**. The portal currently provides password sign-in and
self-service session management. Email verification and invitation acceptance
require the separate [email configuration](email-verification.md). The full
management console with user/application directory pages is tracked in
[issue #15](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/15)
and is not available yet.

For subsequent starts, run `make db-up redis-up`, inspect `make limiter-status`,
then run `make dev-login` while the limiter is active. A changed Redis generation
requires the [recovery procedure](redis.md); do not repeat fencing for ordinary
application restarts. Stop the application before applying pending migrations with
`make db-migrate`. Ctrl-C stops the portal processes; `make db-down redis-down`
stops its dependency containers while preserving their data.

If your browser does not yet trust the development certificate, follow the local
HTTPS instructions below; on macOS the explicit command is `make https-trust`.
To enable OAuth/OIDC as well as login, follow [provider setup](provider.md#configuration-and-development)
and use `make dev-provider` instead. Plain `make dev` provides the foundation
preview with authentication disabled unless explicitly configured.

## Local HTTPS

The canonical host origin is `https://localhost:8443`. Caddy listens on IPv4/IPv6 loopback; Rust uses `127.0.0.1:3001`; Vite uses `127.0.0.1:5173`. Vite only provides frontend development assets and hot reload. Rust owns application behavior. Production frontend output is static and has no Node server.

`make https-setup` validates `config/Caddyfile` and creates a project-local CA in `.local/pki`, with restrictive permissions and implicit trust installation disabled. `make dev` requires that setup. The CA survives normal restarts and `make clean`. Do not delete it while its certificate remains trusted.

On macOS, `make https-trust` prints the certificate fingerprint and installs this root into the current user's login keychain/trust settings for TLS. It may trigger an OS permission prompt. `make https-untrust` removes its trust setting; it does not delete the CA material or necessarily remove the certificate from Keychain. Neither command runs during tests, setup, or ordinary startup. Firefox and other independent trust stores may need their own explicit import.

On Linux, import only the public root certificate into your selected browser's certificate store, or use explicit per-client trust. System trust automation is not yet qualified there. Never copy `.local/pki` private keys to clients or containers.

For terminal verification without modifying host trust:

```sh
make https-check
curl --cacert .local/pki/pki/authorities/local/root.crt https://localhost:8443/health/live
```

These checks validate the chain and hostname; they do not bypass TLS verification. They prove explicit client trust, not that a browser or OS store has been configured. The certificate directory is excluded from Git, container contexts, and release inputs.

The local origin is presently qualified for host processes. Inside a container, `localhost` names that container. Container client DNS/routing and public-root distribution must be implemented and tested with the Compose slice before container OIDC clients are supported; an internal URL must never become a second issuer.

## Process lifecycle and troubleshooting

`make dev` runs in the foreground with combined logs. Ctrl-C or termination stops only process groups started by that invocation. Missing tools, occupied ports, early child exits, and readiness timeouts produce errors and stop owned peers. No command kills a process merely because it occupies a desired port. The proxy routes reserved API/OIDC paths to Rust before forwarding frontend paths. Rust currently returns errors for unimplemented protocol routes.

If a certificate warning appears, verify the displayed fingerprint and configure the intended client trust store. If port 3001, 5173, or 8443 is occupied, stop its owner or use another workspace session after stopping the first. If Caddy is missing, install the documented version or pass `CADDY=/absolute/path/caddy`.

The Rust service reads these optional variables with validated defaults:

| Variable                  | Default                  |
| ------------------------- | ------------------------ |
| `DARKHORSE_HTTP_HOST`     | `127.0.0.1`              |
| `DARKHORSE_HTTP_PORT`     | `3001`                   |
| `DARKHORSE_PUBLIC_ORIGIN` | `https://localhost:8443` |
| `DARKHORSE_STATIC_DIR`    | `apps/console/build`     |

Public origin must be HTTPS with no credentials, path, query, or fragment. The development topology uses fixed ports; changing server settings alone does not reconfigure the proxy. The enabled provider binds this origin as its issuer; discovery requires an active signing key. Configuration errors never print supplied values. The HTTP preview needs no credentials. Explicit database operator commands use separate settings documented in [persistence](persistence.md).

`make build` produces `target/release/darkhorse-server` and `apps/console/build`. Run the binary from the repository root, or set an absolute static directory. `/health/live` reports process liveness only; it does not claim database or authentication readiness.

`make db-setup`, `make db-up`, `make db-migrate`, `make bootstrap`, and `make db-down` manage the local PostgreSQL workflow. `make test-postgres` uses disposable infrastructure instead. `make docker-build` and `make docker-smoke` build and verify the first real image. See [persistence](persistence.md) for protected input, state preservation and the separate deployment qualifications.

For independent Redis cache/limiter development instances, use `make redis-setup`, `make redis-up`, `make redis-status`, and `make redis-down`. `make test-redis` owns isolated Redis/PostgreSQL/TLS fault-test infrastructure and requires OpenSSL. Use `make redis-acl-update` for existing local ACL files and `make limiter-fence`, `make limiter-activate`, and `make limiter-status` for the explicit recovery workflow. See [Redis](redis.md) for credentials, loopback ports, resource limits, and the distinction between diagnostics and admission enforcement.
