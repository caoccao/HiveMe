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

//! macOS notification APIs require a signed bundle, including for raw development
//! binaries and hmc. Cache a small accessory bundle containing the already-built hmg.
//! This never opens or changes the user's config or message history.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_executable(source: &Path, destination: &Path) -> std::io::Result<()> {
  // fs::copy also copies the Finder icon resource fork on macOS. codesign rejects
  // that metadata inside a bundle, so copy only the executable bytes and mode.
  let temporary = destination.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
  let result = (|| {
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    std::io::copy(&mut input, &mut output)?;
    output.set_permissions(input.metadata()?.permissions())?;
    fs::rename(&temporary, destination)
  })();
  if result.is_err() {
    let _ = fs::remove_file(temporary);
  }
  result
}

pub fn bundled_host(directory: &Path, source: &Path) -> Result<PathBuf, String> {
  let bundle = directory.join("HiveMe Notifications.app");
  let contents = bundle.join("Contents");
  let binary = contents.join("MacOS/hmg");
  let metadata = fs::metadata(source).map_err(|e| e.to_string())?;
  let stamp = format!(
    "{}:{}:{:?}",
    source.display(),
    metadata.len(),
    metadata.modified().map_err(|e| e.to_string())?
  );
  if binary.is_file() && fs::read_to_string(directory.join("bundle-version")).ok().as_deref() == Some(&stamp) {
    return Ok(binary);
  }
  fs::create_dir_all(contents.join("MacOS")).map_err(|e| e.to_string())?;
  copy_executable(source, &binary).map_err(|e| e.to_string())?;
  fs::write(
    contents.join("Info.plist"),
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.caoccao.hiveme.notifications</string>
<key>CFBundleName</key><string>HiveMe</string>
<key>CFBundleDisplayName</key><string>HiveMe</string>
<key>CFBundleExecutable</key><string>hmg</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>"#,
  )
  .map_err(|e| e.to_string())?;
  let result = Command::new("/usr/bin/codesign")
    .args(["--force", "--sign", "-"])
    .arg(&bundle)
    .output()
    .map_err(|e| e.to_string())?;
  if !result.status.success() {
    return Err(format!(
      "notification host signing failed: {}",
      String::from_utf8_lossy(&result.stderr)
    ));
  }
  fs::write(directory.join("bundle-version"), stamp).map_err(|e| e.to_string())?;
  Ok(binary)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::os::unix::fs::PermissionsExt;

  #[test]
  fn copying_a_finder_decorated_binary_drops_resource_forks_but_preserves_its_bytes_and_mode() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let destination = directory.path().join("destination");
    fs::write(&source, "executable bytes").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
      Command::new("/usr/bin/xattr")
        .args(["-w", "com.apple.ResourceFork", "icon data"])
        .arg(&source)
        .status()
        .unwrap()
        .success()
    );
    copy_executable(&source, &destination).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), fs::read(&source).unwrap());
    assert_eq!(fs::metadata(&destination).unwrap().permissions().mode() & 0o777, 0o755);
    let attributes = |path: &Path| Command::new("/usr/bin/xattr").arg(path).output().unwrap().stdout;
    assert!(
      String::from_utf8(attributes(&source))
        .unwrap()
        .contains("com.apple.ResourceFork")
    );
    let copied = String::from_utf8(attributes(&destination)).unwrap();
    assert!(!copied.contains("com.apple.ResourceFork"), "{copied}");
    assert!(!copied.contains("com.apple.FinderInfo"), "{copied}");
    // Refresh an existing cached executable without inheriting its old metadata.
    copy_executable(&source, &destination).unwrap();
    assert!(
      !String::from_utf8(attributes(&destination))
        .unwrap()
        .contains("com.apple.ResourceFork")
    );
  }
}
