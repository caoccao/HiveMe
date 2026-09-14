/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

// Bumps the HiveMe version everywhere it is written down.
//
//   deno task -c scripts/ts/deno.json version
//
// Edit PRODUCTION_VERSION and SNAPSHOT_VERSION at the bottom, then run the task.
// Files that do not exist yet, such as src-tauri before step 4.1, are reported and
// skipped.

import * as path from "@std/path";

export class ChangeVersion {
  private productionVersion: string;
  private rootDirPath: string;
  private snapshotVersion: string;

  constructor(productionVersion: string, snapshotVersion: string, rootDirPath?: string) {
    this.productionVersion = productionVersion;
    this.snapshotVersion = snapshotVersion;
    this.rootDirPath = rootDirPath ?? path.join(path.dirname(path.fromFileUrl(import.meta.url)), "../../");
  }

  private _change(filePath: string, patterns: Array<RegExp>, isSnapshot = true) {
    const sourceFilePath = path.join(this.rootDirPath, filePath);
    try {
      Deno.statSync(sourceFilePath);
    } catch (error) {
      if (!(error instanceof Deno.errors.NotFound)) throw error;
      console.warn(`%c  ${sourceFilePath} is not found, skipped.`, "color: yellow");
      return;
    }
    console.info(`Processing ${sourceFilePath}.`);
    const newVersion = isSnapshot ? this.snapshotVersion : this.productionVersion;
    const positionGroups: Array<{ start: number; end: number }> = [];
    const currentContent = Deno.readTextFileSync(sourceFilePath);
    patterns.map((pattern) => {
      [...currentContent.matchAll(pattern)].map((match) => {
        const matchedString = match[0];
        const currentVersion = match.groups!["version"];
        if (newVersion === currentVersion) {
          console.warn(`%c  Ignored ${matchedString}`, "color: yellow");
        } else {
          console.info(`  ${matchedString} => ${newVersion}`);
          const start = match.index! + matchedString.indexOf(currentVersion);
          const end = start + currentVersion.length;
          positionGroups.push({ start: start, end: end });
        }
      });
    });
    if (positionGroups.length > 0) {
      let newContent = "";
      let lastEnd = 0;
      positionGroups.sort((pg1, pg2) => pg1.start - pg2.start);
      positionGroups.map((positionGroup) => {
        if (positionGroup.start > lastEnd) {
          newContent += currentContent.substring(lastEnd, positionGroup.start);
        }
        newContent += newVersion;
        lastEnd = positionGroup.end;
      });
      if (lastEnd < currentContent.length) {
        newContent += currentContent.substring(lastEnd, currentContent.length);
      }
      if (newContent !== currentContent) {
        Deno.writeTextFileSync(sourceFilePath, newContent);
      }
    }
  }

  change() {
    this._change("package.json", [
      /^  "version":\s*"(?<version>\d+\.\d+\.\d+)"/gim,
    ]);
    // The workspace version is inherited by every crate through version.workspace.
    this._change("Cargo.toml", [
      /^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"/gim,
    ]);
    this._change("src-tauri/tauri.conf.json", [
      /^  "version":\s*"(?<version>\d+\.\d+\.\d+)"/gim,
    ]);
    this._change(".github/workflows/linux_build.yml", [
      /^\s*HIVEME_VERSION:\s*(?<version>\d+\.\d+\.\d+)/gim,
    ]);
    this._change(".github/workflows/macos_build.yml", [
      /^\s*HIVEME_VERSION:\s*(?<version>\d+\.\d+\.\d+)/gim,
    ]);
    this._change(".github/workflows/windows_build.yml", [
      /^\s*HIVEME_VERSION:\s*(?<version>\d+\.\d+\.\d+)/gim,
    ]);
  }
}

if (import.meta.main) {
  const changeVersion = new ChangeVersion("0.1.0", "0.1.0");
  changeVersion.change();
}
