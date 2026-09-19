import assert from "node:assert/strict";
import { manage } from "./reference-client.mjs";
import {
  registerResource as register,
  authorizeResource as authorize,
} from "./resource-fixture.mjs";

function permitted(response, app, capability) {
  return (
    response.status === 200 &&
    response.body.active === true &&
    response.body.aud === app.audience &&
    response.body.capabilities.includes(capability)
  );
}
export async function verifyResourceChecks(options) {
  const { page, origin, ca, call, runSql, principal } = options;
  const first = await register(options, "Inventory");
  const second = await register(options, "Billing");
  const tokens = [
    await authorize(options, first),
    await authorize(options, second),
  ];
  const check = (
    app,
    token,
    endpoint = "introspect",
    secret = app.credential.secret,
    headers = {},
  ) =>
    manage(
      origin,
      ca,
      endpoint,
      app.credential.introspection_client_id,
      secret,
      token,
      "access_token",
      headers,
    );
  for (const [app, own, other] of [
    [first, tokens[0], tokens[1]],
    [second, tokens[1], tokens[0]],
  ]) {
    const active = await check(app, own.access_token);
    assert.equal(active.status, 200);
    assert.equal(active.headers["cache-control"], "no-store");
    assert.deepEqual(active.body, {
      active: true,
      aud: app.audience,
      capabilities: [app.read, app.write].sort(),
      client_id: app.client,
      sub: principal,
      iss: origin,
      scope: "openid operate",
      token_type: "Bearer",
      iat: active.body.iat,
      exp: active.body.exp,
    });
    assert.equal(active.body.exp - active.body.iat, 300);
    assert.equal(permitted(active, app, app.write), true);
    for (const token of [
      other.access_token,
      own.id_token,
      own.code,
      "unknown",
      "eyJ0eXAiOiJsb2dvdXQrand0In0.e30.AA",
    ])
      assert.deepEqual((await check(app, token)).body, { active: false });
    assert.equal(
      (await check(app, "unknown", "introspect", "00".repeat(32))).status,
      401,
    );
    assert.equal((await check(app, own.access_token, "revoke")).status, 401);
    assert.deepEqual(
      (
        await manage(
          origin,
          ca,
          "introspect",
          app.client,
          app.secret,
          own.access_token,
        )
      ).body,
      { active: false },
    );
    for (const headers of [{ origin }, { cookie: "unrelated=1" }])
      assert.equal(
        (
          await check(
            app,
            own.access_token,
            "introspect",
            app.credential.secret,
            headers,
          )
        ).status,
        403,
      );
  }
  const timings = [];
  for (let n = 0; n < 10; n++) {
    const response = await check(first, tokens[0].access_token);
    assert.equal(permitted(response, first, first.write), true);
    timings.push(response.elapsedMs);
  }
  timings.sort((a, b) => a - b);
  console.log(
    `Resource introspection development sample: 10 sequential HTTPS/Basic/primary-state/policy checks, milliseconds p50=${timings[4].toFixed(2)}, p95=${timings[9].toFixed(2)}. Existing login limiter and token HTTP admission remain enabled; no positive cache or distributed introspection limiter. This is not a capacity benchmark.`,
  );
  await runSql(
    `DELETE FROM role_capabilities WHERE role_id='${first.role}' AND capability_id='${first.write}'`,
  );
  const reduced = await check(first, tokens[0].access_token);
  assert.equal(permitted(reduced, first, first.write), false);
  assert.equal(permitted(reduced, first, first.read), true);
  assert.equal(
    permitted(
      await check(second, tokens[1].access_token),
      second,
      second.write,
    ),
    true,
  );
  const rotated = await call(page, "/api/admin/resource-introspection", {
    operation: "rotate",
    ...first.identity,
    revision: 0,
    overlap_seconds: 0,
  });
  assert.equal(rotated.status, 200);
  assert.equal((await check(first, tokens[0].access_token)).status, 401);
  first.credential = rotated.body;
  assert.equal(
    permitted(await check(first, tokens[0].access_token), first, first.read),
    true,
  );
  assert.equal(
    (
      await manage(
        origin,
        ca,
        "revoke",
        first.client,
        first.secret,
        tokens[0].access_token,
      )
    ).status,
    200,
  );
  assert.deepEqual((await check(first, tokens[0].access_token)).body, {
    active: false,
  });
  const disabled = await call(page, "/api/admin/resource-introspection", {
    operation: "set_active",
    ...second.identity,
    revision: 0,
    active: false,
  });
  assert.equal(disabled.status, 200);
  assert.equal(disabled.body.secret, undefined);
  assert.equal((await check(second, tokens[1].access_token)).status, 401);
  console.log(
    "Two-resource HTTPS SSO, dedicated credentials, live capability checks, audience isolation, rotation, disablement and committed revocation passed.",
  );
}
