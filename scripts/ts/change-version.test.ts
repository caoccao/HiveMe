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

import { ChangeVersion } from "./change-version.ts";
import { join } from "@std/path";

Deno.test("the Deno version task updates present files and skips missing files", () => {
  const root = Deno.makeTempDirSync({ prefix: "hiveme-version-test-" });
  try {
    Deno.writeTextFileSync(join(root, "package.json"), '{\n  "version": "0.1.0"\n}\n');
    Deno.writeTextFileSync(join(root, "Cargo.toml"), '[workspace.package]\nversion = "0.1.0"\n');
    new ChangeVersion("1.2.3", "1.2.3", root).change();
    const json = JSON.parse(Deno.readTextFileSync(join(root, "package.json")));
    if (json.version !== "1.2.3") throw new Error("package version was not replaced");
    if (!Deno.readTextFileSync(join(root, "Cargo.toml")).includes('version = "1.2.3"')) {
      throw new Error("workspace version was not replaced");
    }
    // Running again is idempotent, including the intentionally absent GUI/workflows.
    new ChangeVersion("1.2.3", "1.2.3", root).change();
    if (JSON.parse(Deno.readTextFileSync(join(root, "package.json"))).version !== "1.2.3") {
      throw new Error("repeated version task changed the result");
    }
  } finally {
    Deno.removeSync(root, { recursive: true });
  }
});
