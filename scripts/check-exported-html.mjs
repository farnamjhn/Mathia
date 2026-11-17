#!/usr/bin/env node
// Compiles a few fixtures and validates the JavaScript in the exported HTML:
// every inline script must parse, use LF line endings, and be allowed by the
// page's Content-Security-Policy hash.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const FIXTURES = [
  { input: "crates/math-core/tests/fixtures/algebra_en.md", flags: [] },
  { input: "crates/math-core/tests/fixtures/analysis_fa.md", flags: ["--ai"] },
  { input: "crates/math-core/tests/fixtures/latex_en.md", flags: [] },
  { input: "crates/math-core/tests/fixtures/svg_figure.html", flags: ["--ai"] },
  { input: "sample-data/0-25.md", flags: [] },
];

const outDir = mkdtempSync(join(tmpdir(), "mathia-html-check-"));
let failures = 0;
let scripts = 0;

function fail(message) {
  failures += 1;
  console.error(`  FAIL ${message}`);
}

try {
  for (const { input, flags } of FIXTURES) {
    const name = input.split("/").at(-1).replace(/\.[^.]+$/, "");
    const out = join(outDir, `${name}${flags.length ? "-ai" : ""}.html`);
    execFileSync(
      "cargo",
      ["run", "-q", "-p", "mathia", "--", join(ROOT, input), "-o", out, ...flags],
      { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] },
    );

    const html = readFileSync(out, "utf8");
    const csp = html.match(/Content-Security-Policy" content="([^"]*)"/)?.[1] ?? "";
    const found = [...html.matchAll(/<script([^>]*)>([\s\S]*?)<\/script>/g)];
    console.log(`${name}: ${found.length} inline scripts`);
    if (found.length === 0) {
      fail(`${name}: no inline scripts found`);
    }

    for (const [index, [, attrs, body]] of found.entries()) {
      scripts += 1;
      const id = attrs.match(/id="([^"]+)"/)?.[1] ?? `script-${index}`;
      if (body.includes("\r")) {
        fail(`${name} ${id}: contains CR line endings`);
      }

      const jsPath = join(outDir, `${name}-${id}.js`);
      writeFileSync(jsPath, body);
      try {
        execFileSync(process.execPath, ["--check", jsPath], {
          stdio: ["ignore", "ignore", "pipe"],
        });
      } catch (error) {
        fail(`${name} ${id}: does not parse\n${error.stderr}`);
        continue;
      }

      const hash = createHash("sha256").update(body).digest("base64");
      if (!csp.includes(`'sha256-${hash}'`)) {
        fail(`${name} ${id}: CSP does not allow its hash`);
      }
    }
  }
} finally {
  rmSync(outDir, { recursive: true, force: true });
}

if (failures > 0) {
  console.error(`\n${failures} failure(s) across ${scripts} inline scripts`);
  process.exit(1);
}
console.log(`\nOK: ${scripts} inline scripts parse and match their CSP hashes`);
