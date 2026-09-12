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

//! Where the config file and the history database live.
//!
//! The rules are specified in `docs/specs/config.md`. Every rule is expressed against
//! an injected [`PathEnv`] and an explicit [`Os`], so that the Windows behaviour can
//! be tested from a Linux or macOS host and vice versa.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The application name, used for the config directory and the file stem.
pub const APP_NAME: &str = "HiveMe";

/// The config file name.
pub const CONFIG_FILE_NAME: &str = "HiveMe.json";

/// The message history database file name.
pub const DATABASE_FILE_NAME: &str = "HiveMe.db";

/// The environment variable that overrides the config path.
pub const CONFIG_PATH_VARIABLE: &str = "HIVEME_CONFIG";

/// The operating system whose rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
  Linux,
  MacOs,
  Windows,
}

impl Os {
  /// The rules of the host this build runs on.
  pub fn current() -> Self {
    if cfg!(target_os = "windows") {
      Self::Windows
    } else if cfg!(target_os = "macos") {
      Self::MacOs
    } else {
      Self::Linux
    }
  }
}

/// Everything outside the process that decides where the config lives.
///
/// Collected once so that path resolution is a pure function of its inputs.
#[derive(Debug, Clone, Default)]
pub struct PathEnv {
  pub hiveme_config: Option<PathBuf>,
  pub xdg_config_home: Option<PathBuf>,
  pub home: Option<PathBuf>,
  pub appdata: Option<PathBuf>,
  pub local_appdata: Option<PathBuf>,
  pub program_files: Option<PathBuf>,
  pub program_files_x86: Option<PathBuf>,
  pub executable: Option<PathBuf>,
}

impl PathEnv {
  /// Reads the real process environment.
  pub fn from_process() -> Self {
    Self {
      hiveme_config: non_empty_var(CONFIG_PATH_VARIABLE),
      xdg_config_home: non_empty_var("XDG_CONFIG_HOME"),
      home: non_empty_var("HOME").or_else(|| non_empty_var("USERPROFILE")),
      appdata: non_empty_var("APPDATA"),
      local_appdata: non_empty_var("LOCALAPPDATA"),
      program_files: non_empty_var("ProgramFiles"),
      program_files_x86: non_empty_var("ProgramFiles(x86)"),
      executable: std::env::current_exe().ok(),
    }
  }
}

fn non_empty_var(name: &str) -> Option<PathBuf> {
  match std::env::var(name) {
    Ok(value) if !value.trim().is_empty() => Some(PathBuf::from(value)),
    _ => None,
  }
}

/// The config file path for this process, honouring `--config` when given.
pub fn config_path(explicit: Option<&Path>) -> Result<PathBuf> {
  config_path_with(explicit, &PathEnv::from_process(), Os::current())
}

/// The config file path under injected inputs.
///
/// The order is `--config`, then `HIVEME_CONFIG`, then the per-OS location.
pub fn config_path_with(explicit: Option<&Path>, env: &PathEnv, os: Os) -> Result<PathBuf> {
  if let Some(path) = explicit {
    return Ok(path.to_path_buf());
  }
  if let Some(path) = env.hiveme_config.as_ref() {
    return Ok(path.clone());
  }
  Ok(config_dir_with(env, os)?.join(CONFIG_FILE_NAME))
}

/// The directory holding the config file and the history database.
pub fn config_dir_with(env: &PathEnv, os: Os) -> Result<PathBuf> {
  match os {
    Os::Linux => {
      if let Some(xdg) = env.xdg_config_home.as_ref() {
        return Ok(xdg.join(APP_NAME));
      }
      let home = env.home.as_ref().ok_or_else(|| missing("HOME"))?;
      Ok(home.join(".config").join(APP_NAME))
    }
    Os::MacOs => {
      let home = env.home.as_ref().ok_or_else(|| missing("HOME"))?;
      Ok(home.join("Library").join("Application Support").join(APP_NAME))
    }
    Os::Windows => {
      if is_installed_windows(env) {
        let appdata = env.appdata.as_ref().ok_or_else(|| missing("APPDATA"))?;
        return Ok(appdata.join(APP_NAME));
      }
      // Portable and development builds keep the config beside the executable.
      let executable = env
        .executable
        .as_ref()
        .ok_or_else(|| Error::ConfigPath("the path of the running executable is unknown".to_owned()))?;
      Ok(parent_directory(executable))
    }
  }
}

/// The history database path, which sits beside the config file.
pub fn database_path(config_path: &Path) -> PathBuf {
  config_path
    .parent()
    .map(Path::to_path_buf)
    .unwrap_or_else(|| PathBuf::from("."))
    .join(DATABASE_FILE_NAME)
}

/// The directory holding `path`, splitting on either separator.
///
/// `Path::parent` only understands the host's separator, so a Windows path would lose
/// its directory when this rule is exercised from Linux or macOS. Doing it by hand
/// keeps the Windows behaviour testable everywhere.
fn parent_directory(path: &Path) -> PathBuf {
  let text = path.to_string_lossy();
  match text.rfind(['/', '\\']) {
    Some(0) => PathBuf::from(&text[..1]),
    Some(index) => PathBuf::from(&text[..index]),
    None => PathBuf::from("."),
  }
}

/// On Windows, an executable under one of the install roots means an installed app.
fn is_installed_windows(env: &PathEnv) -> bool {
  let Some(executable) = env.executable.as_ref() else {
    return false;
  };
  [&env.local_appdata, &env.program_files, &env.program_files_x86]
    .into_iter()
    .flatten()
    .any(|root| starts_with_ignoring_case(executable, root))
}

/// Windows paths are case insensitive, so the comparison has to be too.
fn starts_with_ignoring_case(path: &Path, prefix: &Path) -> bool {
  let path = path.to_string_lossy().to_lowercase().replace('\\', "/");
  let prefix = prefix.to_string_lossy().to_lowercase().replace('\\', "/");
  let prefix = prefix.trim_end_matches('/');
  if prefix.is_empty() {
    return false;
  }
  path == prefix || path.starts_with(&format!("{prefix}/"))
}

fn missing(variable: &str) -> Error {
  Error::ConfigPath(format!("the environment variable {variable} is not set"))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn env() -> PathEnv {
    PathEnv {
      home: Some(PathBuf::from("/home/sam")),
      appdata: Some(PathBuf::from(r"C:\Users\sam\AppData\Roaming")),
      local_appdata: Some(PathBuf::from(r"C:\Users\sam\AppData\Local")),
      program_files: Some(PathBuf::from(r"C:\Program Files")),
      program_files_x86: Some(PathBuf::from(r"C:\Program Files (x86)")),
      ..PathEnv::default()
    }
  }

  #[test]
  fn an_explicit_path_wins_over_everything() {
    let mut env = env();
    env.hiveme_config = Some(PathBuf::from("/from/env.json"));
    let path = config_path_with(Some(Path::new("/from/flag.json")), &env, Os::Linux).unwrap();
    assert_eq!(path, PathBuf::from("/from/flag.json"));
  }

  #[test]
  fn the_environment_variable_wins_over_the_platform_location() {
    let mut env = env();
    env.hiveme_config = Some(PathBuf::from("/from/env.json"));
    let path = config_path_with(None, &env, Os::Linux).unwrap();
    assert_eq!(path, PathBuf::from("/from/env.json"));
  }

  #[test]
  fn linux_prefers_xdg_config_home() {
    let mut env = env();
    env.xdg_config_home = Some(PathBuf::from("/home/sam/.xdg"));
    let path = config_path_with(None, &env, Os::Linux).unwrap();
    assert_eq!(path, PathBuf::from("/home/sam/.xdg/HiveMe/HiveMe.json"));
  }

  #[test]
  fn linux_falls_back_to_dot_config() {
    let path = config_path_with(None, &env(), Os::Linux).unwrap();
    assert_eq!(path, PathBuf::from("/home/sam/.config/HiveMe/HiveMe.json"));
  }

  #[test]
  fn linux_without_home_is_an_error() {
    let mut env = env();
    env.home = None;
    assert!(config_path_with(None, &env, Os::Linux).is_err());
  }

  #[test]
  fn macos_uses_application_support() {
    let path = config_path_with(None, &env(), Os::MacOs).unwrap();
    assert_eq!(
      path,
      PathBuf::from("/home/sam/Library/Application Support/HiveMe/HiveMe.json")
    );
  }

  #[test]
  fn macos_ignores_xdg_config_home() {
    let mut env = env();
    env.xdg_config_home = Some(PathBuf::from("/home/sam/.xdg"));
    let path = config_path_with(None, &env, Os::MacOs).unwrap();
    assert!(path.starts_with("/home/sam/Library"));
  }

  #[test]
  fn windows_installed_under_program_files_uses_appdata() {
    let mut env = env();
    env.executable = Some(PathBuf::from(r"C:\Program Files\HiveMe\hmg.exe"));
    let path = config_path_with(None, &env, Os::Windows).unwrap();
    assert_eq!(
      path,
      PathBuf::from(r"C:\Users\sam\AppData\Roaming").join("HiveMe/HiveMe.json")
    );
  }

  #[test]
  fn windows_installed_under_local_appdata_uses_appdata() {
    let mut env = env();
    env.executable = Some(PathBuf::from(r"C:\Users\sam\AppData\Local\HiveMe\hmg.exe"));
    let path = config_path_with(None, &env, Os::Windows).unwrap();
    assert!(path.to_string_lossy().contains("Roaming"));
  }

  #[test]
  fn windows_installed_under_program_files_x86_uses_appdata() {
    let mut env = env();
    env.executable = Some(PathBuf::from(r"C:\Program Files (x86)\HiveMe\hmg.exe"));
    let path = config_path_with(None, &env, Os::Windows).unwrap();
    assert!(path.to_string_lossy().contains("Roaming"));
  }

  #[test]
  fn windows_portable_keeps_the_config_beside_the_executable() {
    let mut env = env();
    env.executable = Some(PathBuf::from(r"D:\portable\HiveMe\hmg.exe"));
    let path = config_path_with(None, &env, Os::Windows).unwrap();
    assert_eq!(path, PathBuf::from(r"D:\portable\HiveMe").join("HiveMe.json"));
  }

  #[test]
  fn windows_install_detection_ignores_case() {
    let mut env = env();
    env.executable = Some(PathBuf::from(r"c:\program files\HiveMe\hmg.exe"));
    assert!(is_installed_windows(&env));
  }

  #[test]
  fn windows_install_detection_does_not_match_a_sibling_directory() {
    let mut env = env();
    env.executable = Some(PathBuf::from(r"C:\Program Files Custom\HiveMe\hmg.exe"));
    assert!(!is_installed_windows(&env));
  }

  #[test]
  fn a_directory_is_taken_from_either_separator() {
    assert_eq!(
      parent_directory(Path::new(r"D:\portable\HiveMe\hmg.exe")),
      PathBuf::from(r"D:\portable\HiveMe")
    );
    assert_eq!(
      parent_directory(Path::new("/usr/local/bin/hmc")),
      PathBuf::from("/usr/local/bin")
    );
    assert_eq!(parent_directory(Path::new("/hmc")), PathBuf::from("/"));
    assert_eq!(parent_directory(Path::new("hmc")), PathBuf::from("."));
  }

  #[test]
  fn the_database_sits_beside_the_config() {
    let database = database_path(Path::new("/home/sam/.config/HiveMe/HiveMe.json"));
    assert_eq!(database, PathBuf::from("/home/sam/.config/HiveMe/HiveMe.db"));
  }
}
