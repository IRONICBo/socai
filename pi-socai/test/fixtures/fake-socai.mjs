#!/usr/bin/env node

const mode = process.env.FAKE_SOCAI_MODE ?? "ok";

if (mode === "fail") {
  process.stderr.write("browser login is required\n");
  process.exit(23);
}

if (mode === "invalid-json") {
  process.stdout.write("not json\n");
  process.exit(0);
}

if (mode === "hang") {
  process.on("SIGTERM", () => process.exit(143));
  setInterval(() => {}, 1_000);
} else {
  process.stdout.write(JSON.stringify({
    ok: true,
    argv: process.argv.slice(2),
    run_dir: "/tmp/fake-socai-run",
  }));
}
