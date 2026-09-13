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
//! `hmc` publishes one message to the broker and exits. The reference is
//! `docs/specs/cli.md`; this file is only the entry point, so that what a script sees
//! (stdout, stderr, and the exit code) is decided in one place.
//!
//! There is no `windows_subsystem` attribute on purpose: `hmc` is a console
//! application on every platform and must never open a window.

mod cli;
mod failure;
mod run;

use std::ffi::OsString;
use std::process::ExitCode;

use hiveme_core::i18n::{Locale, t_with};

use crate::cli::Cli;
use crate::failure::Failure;

fn main() -> ExitCode {
  // The help is written in the language of the config, so the config is looked at
  // before clap is, and before logging is set up, which makes that look silent.
  let arguments: Vec<OsString> = std::env::args_os().collect();
  let locale = run::configured_locale(cli::config_argument(&arguments).as_deref());

  // clap prints its own usage errors and exits 2, which is the code the specification
  // gives them, so a parse failure never reaches the mapping below.
  let cli = Cli::parse_in(locale, arguments);
  init_logging(cli.verbose);

  match runtime(locale).and_then(|runtime| runtime.block_on(run::run(cli, locale))) {
    Ok(()) => ExitCode::SUCCESS,
    Err(failure) => {
      eprintln!("{}", failure.line());
      ExitCode::from(failure.code())
    }
  }
}

/// The tokio runtime the MQTT client runs on.
///
/// A single thread is enough: one publish, one acknowledgement, and the event loop that
/// carries them. It also starts faster, which matters for something a script runs in a
/// loop.
fn runtime(locale: Locale) -> Result<tokio::runtime::Runtime, Failure> {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(|source| {
      Failure::Unexpected(t_with(
        locale,
        "cli.runtimeUnavailable",
        &[("error", &source.to_string())],
      ))
    })
}

/// Sends the `log` output to stderr, never to stdout.
///
/// Without `--verbose` only warnings are logged; publish confirmation goes to stdout.
/// An unencrypted broker URL still raises a warning. `RUST_LOG` overrides verbosity.
fn init_logging(verbose: bool) {
  let default = if verbose { "debug" } else { "warn" };
  env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default))
    .target(env_logger::Target::Stderr)
    .format_timestamp(None)
    .format_target(false)
    .init();
}
