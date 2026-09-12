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

// Fails when a source file is missing the Apache-2.0 header. The template lives in
// scripts/license-header.txt.
//
//   deno task -c scripts/ts/deno.json check-license-headers
//
// Paths are handled as URLs so that the script behaves the same on Linux, macOS, and
// Windows, and it uses no third party module, so it runs offline.

const MARKER = "Apache License, Version 2.0";
const HEADER_LINES = 20;
const EXTENSIONS = [".rs", ".ts", ".tsx"];
const SKIPPED_DIRECTORY_NAMES = new Set([".git", "dist", "node_modules", "target"]);
const SKIPPED_RELATIVE_PATHS = new Set(["src/generated/", "src-tauri/gen/"]);

const rootDirUrl = new URL("../../", import.meta.url);

/** The path of `url` relative to the repository root, with forward slashes. */
function relative(url: URL): string {
  return decodeURIComponent(url.href.slice(rootDirUrl.href.length));
}

function childUrl(parentDirUrl: URL, name: string, isDirectory: boolean): URL {
  return new URL(isDirectory ? `${encodeURIComponent(name)}/` : encodeURIComponent(name), parentDirUrl);
}

async function collectSourceFiles(dirUrl: URL, found: URL[]): Promise<void> {
  const entries = [];
  for await (const entry of Deno.readDir(dirUrl)) {
    entries.push(entry);
  }
  entries.sort((left, right) => left.name.localeCompare(right.name));

  for (const entry of entries) {
    if (entry.isSymlink) {
      continue;
    }
    const entryUrl = childUrl(dirUrl, entry.name, entry.isDirectory);
    if (entry.isDirectory) {
      if (SKIPPED_DIRECTORY_NAMES.has(entry.name) || SKIPPED_RELATIVE_PATHS.has(relative(entryUrl))) {
        continue;
      }
      await collectSourceFiles(entryUrl, found);
    } else if (EXTENSIONS.some((extension) => entry.name.endsWith(extension))) {
      found.push(entryUrl);
    }
  }
}

async function hasHeader(fileUrl: URL): Promise<boolean> {
  const text = await Deno.readTextFile(fileUrl);
  return text.split("\n", HEADER_LINES).some((line) => line.includes(MARKER));
}

const sourceFiles: URL[] = [];
await collectSourceFiles(rootDirUrl, sourceFiles);

const missing: string[] = [];
for (const fileUrl of sourceFiles) {
  if (!(await hasHeader(fileUrl))) {
    missing.push(relative(fileUrl));
  }
}

if (missing.length > 0) {
  console.error("Missing the Apache-2.0 licence header:");
  for (const file of missing) {
    console.error(`  ${file}`);
  }
  console.error("");
  console.error("Prepend the template from scripts/license-header.txt.");
  Deno.exit(1);
}

console.info(`Licence headers OK, ${sourceFiles.length} file(s) checked.`);
