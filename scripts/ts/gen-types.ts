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

// Generates src/generated/<name>.ts from schemas/<name>.schema.json, so that the
// frontend and the Rust core agree on the config and message shapes.
// See docs/specs/app.md, "Spec sync".
//
//   deno task -c scripts/ts/deno.json gen-types
//   pnpm gen:types
//
// The schemas themselves come from `cargo xtask schema`, which reads the `schemars`
// annotations on the hiveme-core types. Never edit either output by hand.

// The dependency is pinned in scripts/ts/deno.json, which also sets
// nodeModulesDir to "none" so that the repository scripts never reach into the
// frontend's node_modules.
import { compile } from "json-schema-to-typescript";

const rootDirUrl = new URL("../../", import.meta.url);
const schemasDirUrl = new URL("schemas/", rootDirUrl);
const outDirUrl = new URL("src/generated/", rootDirUrl);

const header = (await Deno.readTextFile(new URL("scripts/license-header.txt", rootDirUrl))).trimEnd();

async function isDirectory(url: URL): Promise<boolean> {
  try {
    return (await Deno.stat(url)).isDirectory;
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) {
      return false;
    }
    throw error;
  }
}

if (!(await isDirectory(schemasDirUrl))) {
  console.info(`gen-types: ${schemasDirUrl.pathname} does not exist, nothing to generate.`);
  Deno.exit(0);
}

const schemaNames: string[] = [];
for await (const entry of Deno.readDir(schemasDirUrl)) {
  if (entry.isFile && entry.name.endsWith(".schema.json")) {
    schemaNames.push(entry.name);
  }
}
schemaNames.sort();

if (schemaNames.length === 0) {
  console.info("gen-types: no schemas yet (steps 1.1 and 1.2), nothing to generate.");
  Deno.exit(0);
}

await Deno.mkdir(outDirUrl, { recursive: true });

for (const schemaName of schemaNames) {
  const name = schemaName.replace(/\.schema\.json$/, "");
  const schema = JSON.parse(await Deno.readTextFile(new URL(schemaName, schemasDirUrl)));
  const body = await compile(schema, name, {
    bannerComment: "",
    additionalProperties: false,
    cwd: schemasDirUrl.pathname,
    style: { singleQuote: true },
  });
  const generated = [
    header,
    "",
    `// Generated from schemas/${schemaName} by scripts/ts/gen-types.ts.`,
    "// Do not edit; run `pnpm gen:types` instead.",
    "",
    body,
  ].join("\n");
  await Deno.writeTextFile(new URL(`${name}.ts`, outDirUrl), generated);
  console.info(`gen-types: wrote src/generated/${name}.ts`);
}
