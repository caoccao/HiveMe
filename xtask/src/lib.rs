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

//! Repository automation for HiveMe.
//!
//! The binary in `main.rs` and the tests in `tests/` both call into here, so that
//! `cargo xtask check-spec` and `cargo test` cannot disagree about what passes.
//!
//! See `docs/specs/app.md`, "Spec sync".

pub mod icon;
pub mod schema;
pub mod spec;

use std::path::{Path, PathBuf};

/// The repository root, derived from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .expect("xtask lives one level below the repository root")
    .to_path_buf()
}

/// The path of `path` relative to `root`, for messages a human reads.
pub fn relative(root: &Path, path: &Path) -> String {
  path.strip_prefix(root).unwrap_or(path).display().to_string()
}
