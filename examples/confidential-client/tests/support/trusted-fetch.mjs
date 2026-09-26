import { Agent, fetch } from "undici";

/** A CA-pinned Fetch adapter for local HTTPS integration tests. */
export function trustedFetch(authority) {
  if (
    !(typeof authority === "string" || Buffer.isBuffer(authority)) ||
    authority.length === 0
  ) {
    throw new TypeError("A trusted certificate authority is required.");
  }
  const dispatcher = new Agent({ connect: { ca: authority } });
  return {
    fetch: (input, init) => fetch(input, { ...init, dispatcher }),
    close: () => dispatcher.close(),
  };
}
