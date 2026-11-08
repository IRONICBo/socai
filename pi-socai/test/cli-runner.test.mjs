import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { SocaiCliError, runSocaiCli } from "../src/cli-runner.mjs";

const fixture = fileURLToPath(new URL("./fixtures/fake-socai.mjs", import.meta.url));

function options(mode = "ok") {
  return {
    bin: process.execPath,
    prefixArgs: [fixture],
    env: { FAKE_SOCAI_MODE: mode },
  };
}

test("passes arguments without shell interpretation and parses JSON", async () => {
  const query = "奶茶; touch /tmp/should-not-exist";
  const result = await runSocaiCli(["xhs", "search", query, "--preview"], options());

  assert.deepEqual(result.argv, ["xhs", "search", query, "--preview"]);
  assert.equal(result.ok, true);
});

test("surfaces non-zero CLI exits as structured errors", async () => {
  await assert.rejects(
    runSocaiCli(["xhs", "search", "奶茶"], options("fail")),
    (error) => {
      assert.ok(error instanceof SocaiCliError);
      assert.equal(error.exitCode, 23);
      assert.match(error.message, /browser login is required/);
      return true;
    },
  );
});

test("rejects successful commands that do not return JSON", async () => {
  await assert.rejects(
    runSocaiCli(["xhs", "search", "奶茶"], options("invalid-json")),
    (error) => {
      assert.ok(error instanceof SocaiCliError);
      assert.match(error.message, /valid JSON/);
      return true;
    },
  );
});

test("propagates cancellation to the CLI process", async () => {
  const controller = new AbortController();
  const pending = runSocaiCli(["xhs", "search", "奶茶"], {
    ...options("hang"),
    signal: controller.signal,
  });

  setTimeout(() => controller.abort(new Error("cancelled by harness")), 50);
  await assert.rejects(pending, (error) => error?.name === "AbortError");
});

test("terminates a CLI call that exceeds its deadline", async () => {
  await assert.rejects(
    runSocaiCli(["xhs", "search", "奶茶"], {
      ...options("hang"),
      timeoutMs: 50,
    }),
    (error) => {
      assert.ok(error instanceof SocaiCliError);
      assert.match(error.message, /timed out after 50 ms/);
      return true;
    },
  );
});
