export async function supervise(specs, start, signal) {
  const children = [];
  let onAbort;
  const interrupted = new Promise((resolve) => {
    onAbort = resolve;
  });
  signal.addEventListener("abort", onAbort, { once: true });
  try {
    if (signal.aborted) return;
    for (const spec of specs) children.push(start(spec));
    await Promise.race([interrupted, ...children.map(unexpectedExit)]);
  } finally {
    signal.removeEventListener("abort", onAbort);
    await Promise.all(children.map((child) => child.stop()));
  }
}

async function unexpectedExit(child) {
  await child.done;
  throw new Error("A development process exited. See its output above.");
}
