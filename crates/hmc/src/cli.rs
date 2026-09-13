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

//! The command line of `hmc`.
//!
//! The help this renders is copied into `docs/specs/cli.md`, and
//! [`tests::cli_help_matches_spec`] fails when the two drift, so the specification is
//! the help text rather than a description of it.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use clap::Parser;
use clap::builder::{PossibleValuesParser, TypedValueParser};

use crate::failure::{Failure, Result};

/// The levels `--level` accepts, which are the ones `hiveme_core::Level` knows.
const LEVELS: [&str; 5] = ["debug", "info", "success", "warn", "error"];

/// Send a message to the MQTT broker.
#[derive(Debug, Parser)]
#[command(name = "hmc", version, about, long_about = None)]
pub struct Cli {
  /// Message body. Read from stdin when omitted.
  pub message: Option<String>,

  /// Initialize the shared config from a setup string, then exit
  #[arg(long, value_name = "JSON", conflicts_with_all = ["message", "topic", "json", "title", "level", "qos", "retain"])]
  pub init: Option<String>,

  /// Topic relative to hiveme; leading slashes are ignored [default: hiveme]
  #[arg(short = 't', long, value_name = "TOPIC")]
  pub topic: Option<String>,

  /// Publish MESSAGE (or stdin) as a raw JSON payload without the envelope
  #[arg(long, conflicts_with_all = ["title", "level"])]
  pub json: bool,

  /// Optional title for the message
  #[arg(long, value_name = "TITLE")]
  pub title: Option<String>,

  /// debug | info | success | warn | error [default: info; independent of topic]
  #[arg(short = 'l', long, value_name = "LEVEL", value_parser = PossibleValuesParser::new(LEVELS).map(|level| level.to_ascii_lowercase()), ignore_case = true, hide_possible_values = true)]
  pub level: Option<String>,

  /// 0 | 1 | 2 [default: publish.qos]
  #[arg(short = 'q', long, value_name = "QOS", value_parser = clap::value_parser!(u8).range(0..=2))]
  pub qos: Option<u8>,

  /// Set the retain flag
  #[arg(short = 'r', long)]
  pub retain: bool,

  /// Config file path
  #[arg(short = 'c', long, value_name = "PATH")]
  pub config: Option<PathBuf>,

  /// Log connection details to stderr
  #[arg(short = 'v', long)]
  pub verbose: bool,
}

/// The body to publish, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
  pub text: String,
  pub from_stdin: bool,
}

impl Cli {
  /// The message body, from the argument or from stdin.
  ///
  /// Trailing whitespace is dropped, because the everyday way to reach this is
  /// `echo hi | hmc` and the newline `echo` adds is not part of what the user wrote.
  pub fn body(&self) -> Result<Body> {
    match self.message.as_deref() {
      Some(message) => Self::finish(message, false),
      None => {
        if std::io::stdin().is_terminal() {
          return Err(Failure::Usage(
            "no message: pass one as an argument or pipe it in, see hmc --help".to_owned(),
          ));
        }
        let mut text = String::new();
        std::io::stdin()
          .read_to_string(&mut text)
          .map_err(|source| Failure::Usage(format!("cannot read the message from stdin: {source}")))?;
        Self::finish(&text, true)
      }
    }
  }

  fn finish(text: &str, from_stdin: bool) -> Result<Body> {
    let text = text.trim_end();
    if text.is_empty() {
      let source = if from_stdin {
        "stdin was empty"
      } else {
        "the message is empty"
      };
      return Err(Failure::Usage(format!("nothing to publish: {source}")));
    }
    Ok(Body {
      text: text.to_owned(),
      from_stdin,
    })
  }
}

#[cfg(test)]
mod tests {
  use clap::CommandFactory;

  use super::*;

  /// The fenced block of `docs/specs/cli.md` that holds the help text.
  const HELP_BLOCK: &str = "```text hiveme:help";

  fn spec() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/cli.md");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
  }

  /// The contents of the `text hiveme:help` block, with line endings normalized.
  fn help_from_spec() -> String {
    let text = spec().replace("\r\n", "\n");
    let start = text
      .find(HELP_BLOCK)
      .unwrap_or_else(|| panic!("docs/specs/cli.md has no {HELP_BLOCK} block"));
    let body = &text[start + HELP_BLOCK.len()..];
    let body = body.strip_prefix('\n').expect("the fence is followed by a newline");
    let end = body.find("\n```").expect("the help block is closed");
    body[..end].to_owned()
  }

  fn rendered_help() -> String {
    Cli::command().render_help().to_string().replace("\r\n", "\n")
  }

  #[test]
  fn cli_help_matches_spec() {
    let expected = help_from_spec();
    let actual = rendered_help();
    assert_eq!(
      actual.trim_end(),
      expected.trim_end(),
      "\n--- docs/specs/cli.md ---\n{expected}\n--- hmc --help ---\n{actual}\n\
       Copy the rendered help into the `text hiveme:help` block of docs/specs/cli.md."
    );
  }

  #[test]
  fn the_help_still_names_every_option() {
    let help = rendered_help();
    for flag in [
      "--topic",
      "--json",
      "--title",
      "--level",
      "--qos",
      "--retain",
      "--config",
      "--verbose",
      "--help",
      "--version",
    ] {
      assert!(help.contains(flag), "{flag} is missing from the help:\n{help}");
    }
  }

  #[test]
  fn the_definition_is_internally_consistent() {
    Cli::command().debug_assert();
  }

  #[test]
  fn a_message_argument_is_taken_as_written() {
    let cli = Cli::parse_from(["hmc", "the build finished"]);
    let body = cli.body().unwrap();
    assert_eq!(body.text, "the build finished");
    assert!(!body.from_stdin);
  }

  #[test]
  fn a_trailing_newline_is_not_part_of_the_body() {
    assert_eq!(
      Cli::finish("the build finished\n", true).unwrap().text,
      "the build finished"
    );
    assert_eq!(Cli::finish("two\nlines\n", true).unwrap().text, "two\nlines");
    // Leading whitespace is the user's, so it stays.
    assert_eq!(Cli::finish("  indented  ", false).unwrap().text, "  indented");
  }

  #[test]
  fn an_empty_body_is_a_usage_error() {
    let failure = Cli::finish("   \n", true).unwrap_err();
    assert_eq!(failure.code(), 2);
    assert!(failure.to_string().contains("stdin was empty"), "{failure}");
    assert_eq!(Cli::finish("", false).unwrap_err().code(), 2);
  }

  #[test]
  fn publishing_has_no_absolute_topic_mode() {
    for flag in ["-T", "--absolute-topic"] {
      let error = Cli::try_parse_from(["hmc", flag, "-t", "/ci", "hello"]).unwrap_err();
      assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }
  }

  #[test]
  fn json_has_no_room_for_a_title_or_a_level() {
    for arguments in [
      vec!["hmc", "--json", "--title", "CI", "{}"],
      vec!["hmc", "--json", "--level", "warn", "{}"],
    ] {
      let error = Cli::try_parse_from(&arguments).unwrap_err();
      assert_eq!(
        error.kind(),
        clap::error::ErrorKind::ArgumentConflict,
        "{arguments:?} should conflict"
      );
    }
  }

  #[test]
  fn a_level_the_message_format_does_not_define_is_refused() {
    let error = Cli::try_parse_from(["hmc", "-l", "critical", "boom"]).unwrap_err();
    assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue);
    for level in LEVELS {
      for input in [
        level.to_owned(),
        level.to_uppercase(),
        format!("{}{}", level[..1].to_uppercase(), &level[1..]),
      ] {
        let cli = Cli::try_parse_from(["hmc", "-l", &input, "boom"]).unwrap();
        assert_eq!(cli.level.as_deref(), Some(level), "{input}");
      }
    }
  }

  #[test]
  fn a_quality_of_service_mqtt_does_not_define_is_refused() {
    assert!(Cli::try_parse_from(["hmc", "-q", "3", "boom"]).is_err());
    for qos in ["0", "1", "2"] {
      assert!(Cli::try_parse_from(["hmc", "-q", qos, "boom"]).is_ok(), "{qos}");
    }
  }

  #[test]
  fn init_is_a_mode_of_its_own_and_not_a_publish() {
    // Anything that shapes a message makes no sense while writing a config, so clap
    // says so rather than letting the option be silently ignored.
    for conflicting in [
      vec!["hmc", "--init", "{}", "hello"],
      vec!["hmc", "--init", "{}", "-t", "error"],
      vec!["hmc", "--init", "{}", "--json"],
      vec!["hmc", "--init", "{}", "--title", "CI"],
      vec!["hmc", "--init", "{}", "-l", "warn"],
      vec!["hmc", "--init", "{}", "-q", "1"],
      vec!["hmc", "--init", "{}", "-r"],
    ] {
      let error = Cli::try_parse_from(&conflicting).unwrap_err();
      assert_eq!(
        error.kind(),
        clap::error::ErrorKind::ArgumentConflict,
        "{conflicting:?} should conflict"
      );
    }
  }

  #[test]
  fn init_still_takes_a_config_path_and_verbosity() {
    let cli = Cli::parse_from(["hmc", "--init", "{}", "-c", "x.json", "-v"]);
    assert_eq!(cli.init.as_deref(), Some("{}"));
    assert_eq!(cli.config.as_deref(), Some(std::path::Path::new("x.json")));
    assert!(cli.verbose);
  }

  #[test]
  fn the_short_flags_are_the_documented_ones() {
    let cli = Cli::parse_from([
      "hmc", "-t", "error", "-l", "warn", "-q", "2", "-r", "-v", "-c", "x.json", "boom",
    ]);
    assert_eq!(cli.topic.as_deref(), Some("error"));
    assert_eq!(cli.level.as_deref(), Some("warn"));
    assert_eq!(cli.qos, Some(2));
    assert!(cli.retain);
    assert!(cli.verbose);
    assert_eq!(cli.config.as_deref(), Some(std::path::Path::new("x.json")));
    assert_eq!(cli.message.as_deref(), Some("boom"));
  }
}
