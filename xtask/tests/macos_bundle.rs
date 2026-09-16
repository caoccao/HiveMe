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

//! Exercises bundle signing with an unsigned helper, as produced on Intel Macs.
#![cfg(target_os = "macos")]

use std::fs;
use std::path::Path;
use std::process::Command;

use xtask::repo_root;

fn codesign(path: &Path, arguments: &[&str]) -> std::process::Output {
  Command::new("/usr/bin/codesign")
    .args(arguments)
    .arg(path)
    .output()
    .expect("codesign runs")
}

#[test]
fn the_bundle_hook_signs_an_unsigned_hmc_before_the_app_is_signed() {
  let scratch = tempfile::tempdir().unwrap();
  let root = scratch.path();
  let release = root.join("target/release");
  let bundle = root.join("HiveMe.app");
  let executables = bundle.join("Contents/MacOS");
  fs::create_dir_all(&release).unwrap();
  fs::create_dir_all(&executables).unwrap();
  fs::write(
    bundle.join("Contents/Info.plist"),
    r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.caoccao.hiveme.signing-test</string>
<key>CFBundleExecutable</key><string>hmg</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>"#,
  )
  .unwrap();
  let helper = release.join("hmc");
  let main = executables.join("hmg");
  fs::copy("/usr/bin/true", &helper).unwrap();
  fs::copy("/usr/bin/true", &main).unwrap();
  assert!(codesign(&helper, &["--remove-signature"]).status.success());
  assert!(!codesign(&helper, &["--verify"]).status.success());

  // Tauri's custom macOS files are copied without being added to its signing list.
  // An unsigned nested executable makes signing the main executable fail.
  let bundled_helper = executables.join("hmc");
  fs::copy(&helper, &bundled_helper).unwrap();
  let unsigned = codesign(&main, &["--force", "--sign", "-"]);
  assert!(!unsigned.status.success());
  assert!(String::from_utf8_lossy(&unsigned.stderr).contains("hmc"));

  let config: serde_json::Value =
    serde_json::from_slice(&fs::read(repo_root().join("src-tauri/tauri.macos.conf.json")).unwrap()).unwrap();
  let hook = config["build"]["beforeBundleCommand"].as_str().unwrap();
  let signed = Command::new("/bin/sh")
    .args(["-c", hook])
    .current_dir(root)
    .output()
    .unwrap();
  assert!(signed.status.success(), "{}", String::from_utf8_lossy(&signed.stderr));
  fs::copy(&helper, &bundled_helper).unwrap();
  for target in [&main, &bundle] {
    let signed = codesign(target, &["--force", "--sign", "-"]);
    assert!(signed.status.success(), "{}", String::from_utf8_lossy(&signed.stderr));
  }
  let verified = codesign(&bundle, &["--verify", "--deep", "--strict"]);
  assert!(
    verified.status.success(),
    "{}",
    String::from_utf8_lossy(&verified.stderr)
  );
}
