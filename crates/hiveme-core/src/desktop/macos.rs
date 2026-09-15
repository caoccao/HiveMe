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

//! macOS notifications belong to HiveMe's application identity. Run the app's hmg
//! in notification-only mode, preserving its signed bundle and icon. Raw development
//! binaries use a cached bundle with that same identity, never a second notification
//! application. This never opens or changes the user's config or message history.

use std::ffi::c_void;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON: &[u8] = include_bytes!("../../../../src-tauri/icons/icon.icns");
const APP_IDENTIFIER: &str = "com.caoccao.hiveme";
const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.caoccao.hiveme</string>
<key>CFBundleName</key><string>HiveMe</string>
<key>CFBundleDisplayName</key><string>HiveMe</string>
<key>CFBundleExecutable</key><string>hmg</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>"#;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
  fn CFURLCreateFromFileSystemRepresentation(
    allocator: *const c_void,
    bytes: *const u8,
    length: isize,
    is_directory: u8,
  ) -> *const c_void;
  fn CFRelease(value: *const c_void);
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
  fn LSRegisterURL(url: *const c_void, update: u8) -> i32;
}

fn register_bundle(bundle: &Path) -> Result<(), String> {
  let path = bundle.as_os_str().as_bytes();
  // SAFETY: Core Foundation copies this valid byte slice. A null allocator selects
  // the default allocator; the returned URL follows the Create ownership rule.
  let url = unsafe { CFURLCreateFromFileSystemRepresentation(std::ptr::null(), path.as_ptr(), path.len() as isize, 1) };
  if url.is_null() {
    return Err("the notification bundle URL could not be created".to_owned());
  }
  // SAFETY: url is a live CFURL. Force a rescan even when the bundle's timestamp
  // has not changed: replacing files inside it does not update its directory mtime.
  let status = unsafe { LSRegisterURL(url, 1) };
  // SAFETY: release the one reference returned by the Create function above.
  unsafe { CFRelease(url) };
  if status != 0 {
    return Err(format!("notification host registration failed: macOS error {status}"));
  }
  Ok(())
}

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
  let binary = select_host(directory, source)?;
  // Directly spawning Contents/MacOS/hmg does not refresh Launch Services. Its
  // registered icon can otherwise remain generic despite a correct on-disk bundle.
  // Register cache hits too, because the OS may still hold older bundle metadata.
  register_bundle(binary.ancestors().nth(3).expect("the host is inside an app bundle"))?;
  Ok(binary)
}

fn select_host(directory: &Path, source: &Path) -> Result<PathBuf, String> {
  let source = source.canonicalize().map_err(|e| e.to_string())?;
  if is_app_executable(&source) {
    // Reuse the real app, including its icon resources and signing identity. Both
    // hmg and the hmc shipped beside it launch this executable with the host flag.
    return Ok(source);
  }
  prepare_bundle(directory, &source)
}

fn is_app_executable(source: &Path) -> bool {
  let Some(bundle) = source.ancestors().nth(3) else {
    return false;
  };
  if bundle.extension().is_none_or(|extension| extension != "app") || source != bundle.join("Contents/MacOS/hmg") {
    return false;
  }
  let Ok(info) = plist::Value::from_file(bundle.join("Contents/Info.plist")) else {
    return false;
  };
  let Some(info) = info.as_dictionary() else {
    return false;
  };
  let string = |key| info.get(key).and_then(plist::Value::as_string);
  string("CFBundleIdentifier") == Some(APP_IDENTIFIER)
    && string("CFBundleExecutable") == Some("hmg")
    && string("CFBundleIconFile").is_some_and(|icon| bundle.join("Contents/Resources").join(icon).is_file())
}

fn prepare_bundle(directory: &Path, source: &Path) -> Result<PathBuf, String> {
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
  if binary.is_file()
    && fs::read_to_string(directory.join("bundle-version")).ok().as_deref() == Some(&stamp)
    && fs::read_to_string(contents.join("Info.plist")).ok().as_deref() == Some(INFO_PLIST)
    && fs::read(contents.join("Resources/icon.icns")).ok().as_deref() == Some(ICON)
  {
    return Ok(binary);
  }
  fs::create_dir_all(contents.join("MacOS")).map_err(|e| e.to_string())?;
  copy_executable(source, &binary).map_err(|e| e.to_string())?;
  fs::create_dir_all(contents.join("Resources")).map_err(|e| e.to_string())?;
  fs::write(contents.join("Resources/icon.icns"), ICON).map_err(|e| e.to_string())?;
  fs::write(contents.join("Info.plist"), INFO_PLIST).map_err(|e| e.to_string())?;
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
  fn registration_refreshes_the_icon_even_when_the_bundle_directory_has_not_changed() {
    const LSREGISTER: &str =
      "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    struct Registration(PathBuf);
    impl Drop for Registration {
      fn drop(&mut self) {
        // Only remove this test's unique registration, including after a failure.
        let _ = Command::new(LSREGISTER).arg("-u").arg(&self.0).output();
      }
    }
    let directory = tempfile::tempdir().unwrap();
    let binary = prepare_bundle(directory.path(), Path::new("/usr/bin/true")).unwrap();
    let bundle = binary.ancestors().nth(3).unwrap();
    let _registration = Registration(bundle.to_owned());
    // Never register a test fixture under the real app's identity.
    let identifier = format!("com.caoccao.hiveme.icon-test-{}", uuid::Uuid::now_v7());
    let current = INFO_PLIST.replace(APP_IDENTIFIER, &identifier);
    let old = current.replace("<key>CFBundleIconFile</key><string>icon.icns</string>\n", "");
    let plist = bundle.join("Contents/Info.plist");
    fs::write(&plist, old).unwrap();
    let modified = fs::metadata(bundle).unwrap().modified().unwrap();
    let registered = || {
      let result = Command::new(LSREGISTER).arg("-dump").output().unwrap();
      assert!(result.status.success());
      String::from_utf8(result.stdout)
        .unwrap()
        .split("--------------------------------------------------------------------------------")
        .find(|entry| {
          entry.lines().any(|line| {
            line
              .strip_prefix("identifier:")
              .is_some_and(|value| value.trim() == identifier)
          })
        })
        .expect("the test bundle is registered")
        .to_owned()
    };
    register_bundle(bundle).unwrap();
    assert!(!registered().contains("CFBundleIconFile"));
    fs::write(&plist, current).unwrap();
    assert_eq!(fs::metadata(bundle).unwrap().modified().unwrap(), modified);
    register_bundle(bundle).unwrap();
    let entry = registered();
    assert!(entry.contains("CFBundleIconFile = \"icon.icns\""), "{entry}");
    assert!(entry.contains("Contents/Resources/icon.icns"), "{entry}");
  }

  #[test]
  fn development_notifications_have_the_same_identity_and_icon_as_the_gui() {
    let configuration: serde_json::Value =
      serde_json::from_str(include_str!("../../../../src-tauri/tauri.conf.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let binary = select_host(directory.path(), Path::new("/usr/bin/true")).unwrap();
    let contents = binary.parent().unwrap().parent().unwrap();
    let info = plist::Value::from_file(contents.join("Info.plist")).unwrap();
    let info = info.as_dictionary().unwrap();
    assert_eq!(
      info["CFBundleIdentifier"].as_string(),
      configuration["identifier"].as_str()
    );
    assert_eq!(info["CFBundleIdentifier"].as_string(), Some(APP_IDENTIFIER));
    assert_eq!(fs::read(contents.join("Resources/icon.icns")).unwrap(), ICON);
    assert!(is_app_executable(&binary));
  }

  #[test]
  fn packaged_notifications_reuse_the_actual_app_without_copying_or_resigning_it() {
    let directory = tempfile::tempdir().unwrap();
    let prepared = prepare_bundle(directory.path(), Path::new("/usr/bin/true")).unwrap();
    let bundle = directory.path().join("HiveMe.app");
    fs::rename(prepared.ancestors().nth(3).unwrap(), &bundle).unwrap();
    let binary = bundle.join("Contents/MacOS/hmg");
    let original = fs::read(&binary).unwrap();
    let modified = fs::metadata(&binary).unwrap().modified().unwrap();
    let cache = directory.path().join("unused-cache");
    assert_eq!(select_host(&cache, &binary).unwrap(), binary.canonicalize().unwrap());
    // A symlink to the packaged hmg must not create another app identity either.
    let link = directory.path().join("hmg");
    std::os::unix::fs::symlink(&binary, &link).unwrap();
    assert_eq!(select_host(&cache, &link).unwrap(), binary.canonicalize().unwrap());
    assert_eq!(fs::read(&binary).unwrap(), original);
    assert_eq!(fs::metadata(&binary).unwrap().modified().unwrap(), modified);
    assert!(!cache.exists());
    assert!(
      Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&bundle)
        .status()
        .unwrap()
        .success()
    );
    // An unrelated application or incomplete bundle is never selected as HiveMe.
    fs::write(
      bundle.join("Contents/Info.plist"),
      INFO_PLIST.replace(APP_IDENTIFIER, "com.example.other"),
    )
    .unwrap();
    assert!(!is_app_executable(&binary));
    fs::write(bundle.join("Contents/Info.plist"), INFO_PLIST).unwrap();
    fs::remove_file(bundle.join("Contents/Resources/icon.icns")).unwrap();
    assert!(!is_app_executable(&binary));
  }

  #[test]
  fn signed_bundle_declares_and_contains_the_app_icon_and_repairs_incomplete_cache_entries() {
    let directory = tempfile::tempdir().unwrap();
    // A small real Mach-O executable exercises signing without launching an app.
    let source = Path::new("/usr/bin/true");
    let binary = prepare_bundle(directory.path(), source).unwrap();
    let contents = binary.parent().unwrap().parent().unwrap();
    let icon = contents.join("Resources/icon.icns");
    let plist = contents.join("Info.plist");
    let declared_icon = Command::new("/usr/bin/plutil")
      .args(["-extract", "CFBundleIconFile", "raw", "-o", "-"])
      .arg(&plist)
      .output()
      .unwrap();
    assert!(declared_icon.status.success());
    assert_eq!(String::from_utf8(declared_icon.stdout).unwrap().trim(), "icon.icns");
    assert_eq!(fs::read(&icon).unwrap(), ICON);
    for missing in [&icon, &plist] {
      fs::remove_file(missing).unwrap();
      assert_eq!(prepare_bundle(directory.path(), source).unwrap(), binary);
      assert_eq!(fs::read(&icon).unwrap(), ICON);
      let signature = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(contents.parent().unwrap())
        .output()
        .unwrap();
      assert!(
        signature.status.success(),
        "{}",
        String::from_utf8_lossy(&signature.stderr)
      );
    }
    let modified = fs::metadata(&binary).unwrap().modified().unwrap();
    assert_eq!(prepare_bundle(directory.path(), source).unwrap(), binary);
    assert_eq!(fs::metadata(&binary).unwrap().modified().unwrap(), modified);
  }

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
