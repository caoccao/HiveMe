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

// Gives `hmc.exe` the icon and the version information Windows shows for it, in
// Explorer, in the taskbar, and on the properties sheet. `hmg` gets the same from
// `tauri_build`, so this is the CLI's half of the pair.
//
// Only Windows carries an icon inside the executable. On Linux and macOS an icon
// belongs to a desktop entry or an application bundle, and a command line tool has
// neither, so there is nothing to do and this script does nothing.

fn main() {
  #[cfg(windows)]
  windows::embed_resources();
}

#[cfg(windows)]
mod windows {
  pub fn embed_resources() {
    println!("cargo:rerun-if-changed=icons/hmc.ico");

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("icons/hmc.ico");
    // The version comes from the package. The rest is spelled out, because the package
    // is called `hmc` and what a user reads on the properties sheet should name the
    // product it belongs to and say what the executable does.
    resource.set("ProductName", "HiveMe");
    resource.set("FileDescription", env!("CARGO_PKG_DESCRIPTION"));
    resource.set("InternalName", "hmc");
    resource.set("OriginalFilename", "hmc.exe");
    resource.set("CompanyName", env!("CARGO_PKG_AUTHORS"));
    resource.set("LegalCopyright", "Copyright (c) 2026 caoccao.com Sam Cao. Apache-2.0.");

    if let Err(error) = resource.compile() {
      // A missing resource compiler is a reason for the executable to have no icon,
      // not a reason for the build to fail.
      println!("cargo:warning=hmc.exe was built without its icon: {error}");
    }
  }
}
