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

//! The Finder icons of the built binaries on macOS.
//!
//! Windows keeps an icon inside the executable, which `crates/hmc/build.rs` puts there,
//! and a Linux icon belongs to a desktop entry. macOS has neither: an icon belongs to an
//! application bundle, so `HiveMe.app` wears one and the two binaries under
//! `target/release` are drawn with the generic `exec` placeholder, `hmg` included.
//!
//! macOS does let a plain file carry an icon of its own, which is what this module
//! gives them. `NSWorkspace` writes the icon into the file's resource fork and raises
//! the Finder flag that says to prefer it, so `hmc` and `hmg` show the hive cell in
//! Finder without either of them becoming a bundle.
//!
//! A build replaces the binary and therefore drops the icon, so this runs after the
//! release builds rather than before them. See `docs/development.md`.

use std::path::{Path, PathBuf};

/// The binaries that wear an icon, each with the icon it wears.
///
/// `hmg` borrows the bundle's icon rather than keeping a second copy of it: the two are
/// the same drawing, and one of them going stale would be hard to see.
const ICONS: [(&str, &str); 2] = [
  ("target/release/hmc", "crates/hmc/icons/hmc.icns"),
  ("target/release/hmg", "src-tauri/icons/icon.icns"),
];

/// What [`stamp`] did to one binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
  /// The binary now carries the icon.
  Stamped { binary: PathBuf, icon: PathBuf },
  /// The binary has not been built, so there was nothing to give an icon to.
  NotBuilt { binary: PathBuf },
}

/// Gives every built binary its Finder icon.
///
/// A binary that is not built yet is reported rather than treated as a failure, so that
/// the command is worth running after building only one of the two. A missing icon file
/// is a failure: it belongs to the repository, and the drawing is the point.
pub fn stamp(root: &Path) -> Result<Vec<Outcome>, String> {
  let mut outcomes = Vec::with_capacity(ICONS.len());
  for (binary, icon) in ICONS {
    let binary = root.join(binary);
    let icon = root.join(icon);
    if !icon.is_file() {
      return Err(format!("{} is missing", icon.display()));
    }
    if !binary.is_file() {
      outcomes.push(Outcome::NotBuilt { binary });
      continue;
    }
    set_icon(&binary, &icon)?;
    outcomes.push(Outcome::Stamped { binary, icon });
  }
  Ok(outcomes)
}

/// Writes `icon` into the resource fork of `file` and flags it as the one to draw.
///
/// `setIcon:forFile:options:` answers false and says nothing more, so the error names
/// the two paths and leaves the reason to whoever reads them.
#[cfg(target_os = "macos")]
fn set_icon(file: &Path, icon: &Path) -> Result<(), String> {
  use objc2::AllocAnyThread;
  use objc2_app_kit::{NSImage, NSWorkspace, NSWorkspaceIconCreationOptions};
  use objc2_foundation::NSString;

  let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(&icon.to_string_lossy()))
    .ok_or_else(|| format!("{} is not an image macOS can read", icon.display()))?;
  let accepted = NSWorkspace::sharedWorkspace().setIcon_forFile_options(
    Some(&image),
    &NSString::from_str(&file.to_string_lossy()),
    NSWorkspaceIconCreationOptions::empty(),
  );
  if accepted {
    Ok(())
  } else {
    Err(format!(
      "macOS refused to give {} the icon {}",
      file.display(),
      icon.display()
    ))
  }
}

/// Nowhere else has a per file icon, and [`stamp`] is not reached on those platforms.
#[cfg(not(target_os = "macos"))]
fn set_icon(_file: &Path, _icon: &Path) -> Result<(), String> {
  unreachable!("a Finder icon is a macOS idea")
}

/// Whether this platform draws a file's own icon at all.
pub const fn is_supported() -> bool {
  cfg!(target_os = "macos")
}
