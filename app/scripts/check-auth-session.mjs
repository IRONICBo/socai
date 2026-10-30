// Exercise the real auth panel with a mocked Tauri transport; no account/network access.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import ts from "typescript";

const source = readFileSync(new URL("../src/panels/auth.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const session = { logged_in: true, phone: "+8613800000000", email: "", user_id: "user", device_id: "device" };
const wallet = { balance_points: 123, points_per_cny: 10, starter_points: 50, active_until: null };
const rejected = { code: "authentication_required", message: "point balance failed (401)" };

function harness() {
  const handlers = new Map();
  let signedIn = false;
  let walletCall = async () => wallet;
  let savedSession = { ...session };
  const calls = [];
  const exports = {};
  const shell = { rerender() {} };
  const invoke = async (command) => {
    calls.push(command);
    if (command === "auth_session") return { ...savedSession };
    if (command === "billing_wallet") return walletCall();
    if (command === "auth_logout") {
      savedSession = { ...session, logged_in: false };
      throw "logout failed (401 Unauthorized): invalid token";
    }
    throw Error(`Unexpected command: ${command}`);
  };
  vm.runInNewContext(compiled, {
    exports,
    require(name) {
      if (name.endsWith("/core")) return { invoke };
      if (name.endsWith("/event")) return { listen: async () => () => {} };
      if (name.endsWith(".jpg")) return { default: "google.jpg" };
      if (name.endsWith("/html")) return { esc: String };
      if (name.endsWith("/i18n")) return {
        t: (key, args) => key + (args ? JSON.stringify(args) : ""),
        setAccountSignedIn: (value) => { signedIn = value; },
      };
      throw Error(`Unexpected import: ${name}`);
    },
    console: { error() {} },
    window: { clearTimeout() {}, clearInterval() {} },
    document: { getElementById: (id) => ({ addEventListener: (event, cb) => handlers.set(`${id}:${event}`, cb) }) },
  });
  const panel = exports.authMenu;
  const bind = () => panel.bind(shell, () => {}, async () => {});
  return {
    panel, calls, bind,
    setWallet: (fn) => { walletCall = fn; },
    signedIn: () => signedIn,
    show() { panel.requireSignIn(shell); bind(); return panel.render(); },
    async click(id) {
      handlers.get(`${id}:click`)();
      await new Promise((resolve) => setImmediate(resolve));
      bind();
    },
  };
}

{
  const h = harness();
  h.setWallet(async () => { throw rejected; });
  await h.panel.loadSession();
  assert.equal(h.panel.isLoggedIn(), false);
  assert.equal(h.signedIn(), false);
  assert.match(h.show(), /auth.sessionExpired/);
  assert.match(h.panel.render(), /id="auth-sign-in-again"/);
  assert.doesNotMatch(h.panel.render(), /id="auth-upgrade"/);
  // A subsequent network error must not make a rejected session look valid.
  h.setWallet(async () => { throw { code: "wallet_unavailable" }; });
  await h.panel.refreshWallet();
  assert.equal(h.panel.isLoggedIn(), false);
  await h.click("auth-sign-in-again");
  assert.match(h.panel.render(), /id="auth-phone-form"/);
  assert.match(h.panel.render(), /138 0000 0000/);
  assert.match(h.panel.render(), /id="auth-google-login"/);
  assert.doesNotMatch(h.panel.render(), /auth.sessionExpired|logout failed/);
  assert.ok(h.calls.includes("auth_logout"));
}

for (const message of ["network timeout", "point balance failed (403 Forbidden)", "HTTP 500"]) {
  const h = harness();
  h.setWallet(async () => { throw { code: "wallet_unavailable", message }; });
  await h.panel.loadSession();
  assert.equal(h.panel.isLoggedIn(), true);
  assert.match(h.show(), /id="auth-wallet-retry"/);
  assert.doesNotMatch(h.panel.render(), /auth.sessionExpired/);
  h.setWallet(async () => wallet);
  await h.click("auth-wallet-retry");
  assert.match(h.panel.render(), /123/);
  assert.doesNotMatch(h.panel.render(), /id="auth-wallet-retry"/);
}

{
  const h = harness();
  h.setWallet(async () => { throw rejected; });
  await h.panel.loadSession();
  h.show();
  // A backend compatibility fix can restore the original credential.
  h.setWallet(async () => wallet);
  await h.panel.refreshWallet();
  assert.equal(h.panel.isLoggedIn(), true);
  assert.equal(h.signedIn(), true);
  assert.doesNotMatch(h.panel.render(), /auth.sessionExpired|id="auth-sign-in-again"/);
}

{
  const h = harness();
  await h.panel.loadSession();
  h.show();
  let failOld;
  h.setWallet(() => new Promise((_, reject) => { failOld = reject; }));
  const old = h.panel.refreshWallet();
  h.setWallet(async () => wallet);
  await h.panel.loadSession();
  failOld(rejected);
  await old;
  assert.equal(h.panel.isLoggedIn(), true);
  assert.match(h.panel.render(), /123/);

  let finishOld;
  h.setWallet(() => new Promise((resolve) => { finishOld = resolve; }));
  const pending = h.panel.refreshWallet();
  h.bind();
  await h.click("auth-logout");
  finishOld(wallet);
  await pending;
  assert.equal(h.panel.isLoggedIn(), false);
  assert.doesNotMatch(h.panel.render(), /auth-wallet-points/);
}

console.log("Auth panel: rejected session, relogin, network/403/500 retry and stale-response checks passed.");
