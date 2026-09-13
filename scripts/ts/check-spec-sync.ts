/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License")
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

// Fails when load bearing code changes without its specification, and when the two
// halves of the IPC protocol drift apart. See docs/specs/app.md, "Spec sync".
//
//   deno task -c scripts/ts/deno.json check-spec-sync [base-ref]
//
// The base ref defaults to $SPEC_SYNC_BASE, then origin/main, then HEAD~1.
// Set SPEC_SYNC_EXEMPT=1 to skip the check; CI sets it from the `spec-sync-exempt`
// pull request label.

const rootDirUrl = new URL("../../", import.meta.url);

interface GitResult {
  ok: boolean;
  stdout: string;
  stderr: string;
}

async function git(...args: string[]): Promise<GitResult> {
  const { code, stdout, stderr } = await new Deno.Command("git", {
    args,
    cwd: rootDirUrl,
    stdout: "piped",
    stderr: "piped",
  }).output();
  const decoder = new TextDecoder();
  return {
    ok: code === 0,
    stdout: decoder.decode(stdout).trim(),
    stderr: decoder.decode(stderr).trim(),
  };
}

async function refExists(ref: string): Promise<boolean> {
  return (await git("rev-parse", "--verify", "--quiet", ref)).ok;
}

/** A rule: changing code that matches `code` requires a change to `spec`. */
interface SpecRule {
  label: string;
  code: RegExp;
  spec: string;
}

const SPEC_RULES: SpecRule[] = [
  {
    label: "The config module",
    code: /^crates\/hiveme-core\/src\/config(\.rs|\/)/,
    spec: "docs/specs/config.md",
  },
  {
    label: "The message module",
    code: /^crates\/hiveme-core\/src\/message(\.rs|\/)/,
    spec: "docs/specs/message.md",
  },
  {
    label: "The MQTT client",
    code: /^crates\/hiveme-core\/src\/mqtt(\.rs|\/)/,
    spec: "docs/specs/hivemq-cloud.md",
  },
  {
    label: "The shared session",
    code: /^crates\/hiveme-core\/src\/session(\.rs|\/)/,
    spec: "docs/specs/session.md",
  },
  {
    label: "The Rust catalogs",
    code: /^crates\/hiveme-core\/src\/i18n(\.rs|\/)/,
    spec: "docs/specs/tui.md",
  },
  {
    label: "The CLI",
    // The terminal UI has a specification of its own, below.
    code: /^crates\/hmc\/src\/(?!tui\/)/,
    spec: "docs/specs/cli.md",
  },
  {
    label: "The terminal UI",
    code: /^crates\/hmc\/src\/tui\//,
    spec: "docs/specs/tui.md",
  },
  {
    label: "The GUI backend interface",
    code: /^src-tauri\/src\/(lib|protocol|controller)\.rs$/,
    spec: "docs/specs/gui.md",
  },
];

/**
 * The two halves of the IPC protocol are hand synced and must move together. The Rust
 * half is `protocol.rs` plus the shared session's types, which it re-exports.
 */
const PROTOCOL_RUST = ["src-tauri/src/protocol.rs", "crates/hiveme-core/src/session/types.rs"];
const PROTOCOL_TS = "src/lib/protocol.ts";

if (Deno.env.get("SPEC_SYNC_EXEMPT") === "1") {
  console.info("Spec sync check skipped: SPEC_SYNC_EXEMPT=1.");
  Deno.exit(0);
}

let base = Deno.args[0] ?? Deno.env.get("SPEC_SYNC_BASE") ?? "";
if (base === "") {
  if (await refExists("origin/main")) {
    base = "origin/main";
  } else if (await refExists("HEAD~1")) {
    base = "HEAD~1";
  } else {
    console.info("Spec sync check skipped: no base commit to compare against.");
    Deno.exit(0);
  }
}

if (!(await refExists(base))) {
  console.info(`Spec sync check skipped: base ref '${base}' does not exist.`);
  Deno.exit(0);
}

const mergeBaseResult = await git("merge-base", base, "HEAD");
const mergeBase = mergeBaseResult.ok ? mergeBaseResult.stdout : base;

const diff = await git("diff", "--name-only", `${mergeBase}...HEAD`);
if (!diff.ok) {
  console.error(`Spec sync check failed: git diff against ${base}: ${diff.stderr}`);
  Deno.exit(1);
}

const changed = diff.stdout.split("\n").filter((line) => line !== "");
if (changed.length === 0) {
  console.info(`Spec sync check: no changes against ${base}.`);
  Deno.exit(0);
}

const failures: string[] = [];

for (const rule of SPEC_RULES) {
  const codeChanged = changed.some((file) => rule.code.test(file));
  const specChanged = changed.includes(rule.spec);
  if (codeChanged && !specChanged) {
    failures.push(`${rule.label} changed without ${rule.spec}`);
  }
}

for (const rust of PROTOCOL_RUST) {
  if (changed.includes(rust) && !changed.includes(PROTOCOL_TS)) {
    failures.push(`${rust} changed without ${PROTOCOL_TS}`);
  }
}
if (changed.includes(PROTOCOL_TS) && !PROTOCOL_RUST.some((rust) => changed.includes(rust))) {
  failures.push(`${PROTOCOL_TS} changed without ${PROTOCOL_RUST.join(" or ")}`);
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`FAIL ${failure}`);
  }
  console.error("");
  console.error("Update the specification alongside the code, or apply the spec-sync-exempt label.");
  Deno.exit(1);
}

console.info(`Spec sync check OK against ${base}.`);
