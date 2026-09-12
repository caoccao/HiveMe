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

//! HiveMe CLI.
//!
//! The command line interface is specified in `docs/specs/cli.md` and implemented in
//! step 3.1 of `docs/plans/plan-initialization.md`. This entry point exists so that
//! the workspace builds and the release pipeline can be exercised from phase 0.

use std::process::ExitCode;

fn main() -> ExitCode {
  eprintln!(
    "hmc: not implemented yet: HiveMe {} is at phase 0, see docs/plans/plan-initialization.md step 3.1",
    hiveme_core::VERSION
  );
  ExitCode::from(1)
}
