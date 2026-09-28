import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("web demo uses same-origin authenticated API with bounded read-only runs", async () => {
  const [page, runtime, vercel] = await Promise.all([
    read("src/pages/demo.astro"),
    read("src/scripts/demo.ts"),
    read("vercel.json"),
  ]);

  assert.match(page, /PUBLIC_SOCAI_API_BASE_URL \|\| "\/api\/server"/u);
  assert.match(page, /每天（UTC）最多运行 5 次/u);
  assert.match(page, /data-live-panel/u);
  assert.match(page, /data-email-form data-provider="email"/u);
  assert.match(page, /<div class="live-frame" inert>/u);
  assert.match(page, /tabindex="-1"/u);
  assert.match(page, /sandbox="allow-scripts allow-same-origin"/u);
  assert.match(page, /referrerpolicy="no-referrer"/u);
  assert.match(page, /frame-src 'self' https:\/\/live\.browser-use\.com/u);
  assert.match(runtime, /credentials: "include"/u);
  assert.match(runtime, /"X-CSRF-Token"/u);
  assert.match(runtime, /parsed\.hostname === "live\.browser-use\.com"/u);
  assert.match(runtime, /clearAccountState/u);
  assert.match(runtime, /request\("\/v1\/web\/runs"\)/u);
  assert.match(runtime, /Math\.min\(15000, 1000 \* 2 \*\* pollFailures\)/u);
  assert.match(runtime, /\["localhost", "127\.0\.0\.1", "::1"\]/u);
  assert.doesNotMatch(`${page}\n${runtime}`, /BROWSER_USE_API_KEY/u);
  assert.match(vercel, /"source": "\/api\/server\/:path\*"/u);
  assert.match(vercel, /"destination": "https:\/\/api\.socai\.work\/:path\*"/u);
});

test("book a demo posts validated data to the persisted backend endpoint", async () => {
  const [page, runtime, header, footer] = await Promise.all([
    read("src/pages/book-demo.astro"),
    read("src/scripts/book-demo.ts"),
    read("src/components/SiteHeader.astro"),
    read("src/components/SiteFooter.astro"),
  ]);

  assert.match(page, /name="use_case"/u);
  assert.match(page, /name="website"/u);
  assert.match(runtime, /\/v1\/web\/demo-requests/u);
  for (const navigation of [header, footer]) {
    assert.match(navigation, /href="\/demo"/u);
    assert.match(navigation, /href="\/book-demo"/u);
    assert.match(navigation, /href="\/platforms"/u);
  }
});

test("platform page separates structured connectors from public browser research", async () => {
  const page = await read("src/pages/platforms.astro");
  for (const platform of ["X", "Instagram", "LinkedIn", "TikTok", "Douyin", "Xiaohongshu"]) {
    assert.match(page, new RegExp(`\\["${platform}"`, "u"));
  }
  assert.match(page, /不等同于已承诺稳定结构的 socai Connector/u);
  assert.match(page, /isolated public browser without a signed-in profile/u);
});
