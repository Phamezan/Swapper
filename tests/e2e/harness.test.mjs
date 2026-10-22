import assert from "node:assert/strict";
import { test } from "node:test";
import { Cdp, allowance, poll } from "./harness.mjs";

class Socket extends EventTarget {
  sent = [];
  send(value) { this.sent.push(JSON.parse(value)); }
  reply(value) { this.dispatchEvent(new MessageEvent("message", { data: JSON.stringify(value) })); }
  close() { this.dispatchEvent(new Event("close")); }
}

test("CDP correlates concurrent replies and reports protocol errors", async () => {
  const socket = new Socket();
  const cdp = new Cdp(socket);
  const first = cdp.send("Page.enable");
  const second = cdp.send("Runtime.enable");
  socket.reply({ id: socket.sent[1].id, result: { second: true } });
  socket.reply({ id: socket.sent[0].id, error: { message: "Invalid target" } });
  await assert.rejects(first, /Page.enable: Invalid target/);
  assert.deepEqual(await second, { second: true });
  socket.close();
});

test("CDP rejects pending commands on disconnect and on deadline", async () => {
  const socket = new Socket();
  const cdp = new Cdp(socket);
  const pending = cdp.send("Page.navigate");
  socket.close();
  await assert.rejects(pending, /closed/);
  const other = new Cdp(new Socket());
  await assert.rejects(other.send("Page.enable", {}, 10), /timed out/);
  other.close();
});

test("poll retries transient readiness failures and gives a useful deadline", async () => {
  let attempts = 0;
  assert.equal(await poll("fixture ready", () => {
    if (++attempts === 1) throw new Error("not listening");
    return "ready";
  }, 200, 1), "ready");
  await assert.rejects(poll("missing selector", () => false, 10, 1), /missing selector/);
});

test("allow-list matches only the documented preview noise", () => {
  assert.ok(allowance({ kind: "resource", url: "http://127.0.0.1:1541/favicon.ico", status: 404 }));
  assert.ok(allowance({ kind: "log", url: "http://127.0.0.1:1541/favicon.ico", text: "Failed to load resource: the server responded with a status of 404 (Not Found)" }));
  assert.equal(allowance({ kind: "log", url: "http://127.0.0.1:1541/favicon.ico", text: "Failed to load resource: the server responded with a status of 500 (Internal Server Error)" }), null);
  assert.equal(allowance({ kind: "resource", url: "http://127.0.0.1:1541/src/main.tsx", status: 404 }), null);
  assert.equal(allowance({ kind: "exception", text: "TypeError: Cannot read properties of undefined" }), null);
  assert.equal(allowance({ kind: "exception", text: "Not mocked: unexpected_command" }), null);
});
