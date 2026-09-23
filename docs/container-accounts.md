# Container account commands

The host-side launchers run the existing Rust `operator account` commands inside
the selected deployment: `show`, `deactivate`, `reactivate` and `revoke-all`.
They require a fresh platform-administrator password on every invocation. Container
access alone does not identify or authenticate an application actor. Read the
[account authority and audit contract](operator-accounts.md) first.

These targets are for protected stdin and JSON output. They never allocate a TTY,
start HTTP, migrate, repair the limiter or retry a command. Node runs only the
host launcher; the packaged Rust binary performs authentication and all account
logic. Use the repository's pinned Node toolchain, Docker Compose v2 or kubectl.

## Input and selection

Deliver a JSON object containing `email`, `password` and, for mutations, `reason`
through your secret manager or an owner-only file outside the repository. Protect
the containing directory, use file mode `0600`, avoid symlinks/untrusted writers,
and remove the input according to your secret-handling policy afterward. The
launcher passes stdin directly to the transport; Rust validates the 16 KiB limit
and input schema. It does not create or inspect a credential file. Never put a
password in Make variables, environment variables, arguments, shell history or
reason text. Do not use shell tracing or a terminal/session recorder with secret
input. Output can contain profile data and must also be protected.

Only nonsecret command selectors are accepted:

| Setting             | Meaning                                                         |
| ------------------- | --------------------------------------------------------------- |
| `ACCOUNT_OPERATION` | `show` (default), `deactivate`, `reactivate`, `revoke-all`      |
| `ACCOUNT_ID`        | Required nonzero, hyphenated principal UUID                     |
| `ACCOUNT_REVISION`  | Required current revision for changes; omit for `show`          |
| `ACCOUNT_CONFIRM`   | `yes` explicitly confirms a reviewed mutation; defaults to `no` |

Read the target first and review its current revision, intended operation and
reason. Do not automatically confirm or fetch a newer revision to retry a failed
mutation. `--yes` is confirmation, not authorization. A protected input file is
represented below by `/private/path/account-input.json`; the commands do not
create it. Replace the UUID/revision placeholders with observed values.

### Running Compose application

Use an existing initialized [qualification stack](compose.md):

```sh
make stack-account-exec STACK=trial ACCOUNT_ID=<principal-uuid> < /private/path/account-input.json
make stack-account-exec STACK=trial ACCOUNT_OPERATION=revoke-all ACCOUNT_ID=<principal-uuid> ACCOUNT_REVISION=<observed-revision> ACCOUNT_CONFIRM=yes < /private/path/account-input.json
```

The launcher selects the stack's explicit project/Compose file and runs the
absolute binary path in `api` as UID/GID `10001:10001`, with `exec -T`.
[Compose exec disables its default TTY with `-T`](https://docs.docker.com/reference/cli/docker/compose/exec/).
An absent/stopped application container is an error; this target never starts it.

### Compose with HTTP stopped

```sh
make stack-stop STACK=trial
make stack-account-run STACK=trial ACCOUNT_ID=<principal-uuid> < /private/path/account-input.json
```

The operations-profile `account` service uses the manifest's immutable application
image, a nonroot user, a read-only filesystem, dropped capabilities, bounded
temporary storage, and no published ports. Its projected secrets are the runtime
database URL, shared login-limit key, runtime cache/limiter URLs and public CA.
It receives no migration, operator, signing-wrap or limiter-recovery credential.
Its networks are limited to the database and limiter.
The cache configuration is required by the shared configuration loader; account
authentication connects to the primary database and shared limiter.

`compose run --rm --no-deps -T` creates one named container and normally removes it
on exit. It does not start dependency services; see [Compose run semantics](https://docs.docker.com/reference/cli/docker/compose/run/).
The database and active limiter must remain available, with the same login key and
reviewed schema/grants. A stopped HTTP server does not remove these dependencies.
The generated `darkhorse-account-…` container name is printed on stderr so an
operator can inspect an interrupted attachment. Do not bulk-prune containers or
repeat an uncertain mutation to determine whether it succeeded.

### Explicit Kubernetes Pod

```sh
make kube-account-exec KUBE_CONFIG=/absolute/path/identity.json KUBE_ACCESS=/absolute/path/access.yaml KUBE_CONTEXT=reviewed-context ACCOUNT_POD=<reviewed-pod-name> ACCOUNT_ID=<principal-uuid> < /private/path/account-input.json
```

`KUBE_CONFIG` is the reviewed [deployment configuration](kubernetes.md), not a
kubeconfig. `KUBE_ACCESS` is the kubeconfig; both paths must be absolute. The
context, configured namespace, exact Pod name and `api` container are explicit.
No ambient current context, deployment selector or automatic Pod replacement is
used. `kubectl exec -i` forwards stdin without a TTY. A 30-second API request
timeout and 15-second running-Pod wait bound connection setup; see
[kubectl exec](https://kubernetes.io/docs/reference/kubectl/generated/kubectl_exec/).
Pod names are trusted selectors, not a UID or image-attestation guarantee. Confirm
the cluster, namespace and workload/image before execution; namespace and
kubeconfig administrators remain trusted. Stopped-container Kubernetes account
recovery is not provided by this target.

## Exit behavior and uncertain outcomes

Invoke the launcher directly when automation needs the child's exact exit code:

```sh
STACK=trial ACCOUNT_ID=<principal-uuid> node scripts/account.mjs compose-exec < /private/path/account-input.json
```

The other modes are `compose-run` and `kube-exec`, with the same environment
selectors. Successful Rust results are JSON on stdout. Application failures are
JSON on stderr; Docker/kubectl and launcher diagnostics can also appear on stderr,
so do not parse the entire stderr stream as one JSON document.

The launcher preserves normal transport exit codes, including Rust's `0` success,
`1` operation/input failure, `2` invalid arguments, `3` missing confirmation and
`74` output failure when the transport forwards them. Docker/kubectl can also
return their own failure codes; a status alone cannot prove remote rollback.
Make reports a failed recipe using its own nonzero status (normally `2`); Make does
**not** preserve the recipe's exact code. No operation is retried by either path.

The host attachment has a 120-second deadline. Timeout returns `124`; host `SIGINT`,
`SIGTERM` and `SIGHUP` return `130`, `143` and `129`. The launcher terminates only its
owned local process group, allowing up to two seconds before forced termination.
These controls cannot cancel or roll back a remote transaction. A dropped exec
connection, killed host process, failed output pipe or lost response may leave
remote work running or already committed. Inspect the selected container/Pod,
current account revision and authoritative operation audit before any deliberate
retry. Preserve the operation correlation ID if received. For a one-shot run,
inspect the printed container name and remove an abandoned owned container only
after reconciling the outcome; automatic cleanup on disconnection is not assured.

## Authority and capacity

Restrict Docker daemon access, Kubernetes `pods/exec`, workload creation and Secret
access to trusted deployment operators. Exec can run arbitrary binaries in a
container and expose its runtime secrets; these permissions are much broader than
one account command. Kubernetes API/exec metadata exposes nonsecret command
selectors. Daemons, kubelets, credential plugins and session-recording infrastructure
can observe the stream. Protected stdin avoids arguments/manifests; it does not
hide credentials from those trusted components.

The Compose one-shot service limits its database pool to two and limiter pool to
one connection, with 512 MiB memory, two CPUs and 128 processes. Running-container
exec inherits the server configuration, creates additional pools (currently five
database and four limiter connections per command), and shares the server's
container resource limits. Kubernetes has the same per-command pools with the
current manifest. Account commands take the shared security fence, including
reads, so contention can delay other security operations. Existing database
query/lock and authentication-proof limits still apply.

Run one administrative operation at a time and reserve backend/container capacity;
there is no distributed operator-concurrency limit in these launchers. Shared
password-attempt budgets apply across CLI and HTTP. The account service's runtime
DML credential remains broad; fresh password verification does not contain a
compromised workload or database credential. Production privileged assurance,
emergency access and protected audit evidence remain in #23. Load/capacity
qualification with maximum replicas and concurrent operators remains in #10/#27.

## Validation matrix

`make test-account-launcher` uses disposable local subprocesses, without services.
`make test-compose` and `make test-kubernetes` use disposable deployments and the
public launcher entrypoint, without changing developer stacks or ambient cluster
contexts. They exercise the current image, schema and runtime grants. See their
prerequisites and remaining deployment limits in the linked guides.

| Mode / scenario                                                      | Required authority                                            | State and audit expectation                                                                           | Exit / evidence                                               |
| -------------------------------------------------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| Protected stdin transport, no TTY                                    | Host test process only                                        | Exact stdin forwarded; marker absent from arguments, environment and diagnostics                      | Process suite preserves `0/1/2/3/74/125`                      |
| Invalid selectors                                                    | None                                                          | No remote launch or target access                                                                     | `2`; pure and process suites                                  |
| Compose exec / Kubernetes exec, authenticated show                   | Deployment exec plus active administrator password            | Read with verified actor and runtime-role audit                                                       | JSON, `0`; both deployment suites                             |
| Wrong password / stale revision                                      | Exec plus supplied credentials                                | Generic authentication denial or audited conflict; no target change                                   | `1`; both suites                                              |
| Missing mutation confirmation                                        | Exec; confirmation absent                                     | No mutation                                                                                           | `3`; both suites                                              |
| Last administrator deactivation                                      | Exec plus administrator password                              | Directory invariant rejects change                                                                    | `1`; Compose suite                                            |
| Compose exec with stopped API                                        | Deployment exec                                               | No application start                                                                                  | Nonzero; Compose suite                                        |
| Compose one-shot show/deactivate/reactivate/revoke-all, HTTP stopped | Workload creation, runtime secrets and administrator password | Expected revisions and runtime-role audit; no HTTP listener; ordinary container removed               | JSON, `0`; Compose suite                                      |
| Lost limiter continuity                                              | Exec/run plus valid password                                  | Reject account access until separate recovery                                                         | `1`; both suites                                              |
| Kubernetes revoke-all across serving replicas                        | Exec plus administrator password and observed revision        | Committed revocation immediately invalidates token checks on both replicas                            | `0`; Kubernetes suite                                         |
| Local timeout / signal                                               | Host process control                                          | Stop owned child/descendant processes, preserve unrelated process; no retry; remote outcome uncertain | `124` / `143`; process suite, other signal mapping unit tests |
| Backup/migration during account run                                  | Deployment operations                                         | Reject maintenance while account service is running                                                   | Nonzero; Compose suite                                        |

Native interactive commands inside a running container can use
`docker exec --interactive --tty --user 10001:10001 <reviewed-api-container-id> /usr/local/bin/darkhorse-server operator account show <principal-uuid>`
or the corresponding Kubernetes command:

```sh
kubectl --kubeconfig /absolute/path/access.yaml --context reviewed-context --namespace <reviewed-namespace> exec -it pod/<reviewed-pod-name> --container api -- /usr/local/bin/darkhorse-server operator account show <principal-uuid>
```

They use Rust's hidden password prompt. These manual terminal paths are separate
from the protected-stdin targets; end-to-end remote PTY, disconnect and terminal
restoration qualification remains open. The native CLI's existing POSIX terminal
suite alone does not establish remote PTY behavior.

The broader account suite covers transaction/audit failures, actor reductions and
lost commit responses at the database boundary. Container-specific termination
mid-transaction, interrupted output, simultaneous operators/SSO load, incompatible
binary/schema versions, stopped-container Kubernetes recovery with temporary
credentials, independent security review and release artifact qualification remain
open. This increment does not complete #27 or establish production readiness;
full authored-code coverage remains open in #2.
