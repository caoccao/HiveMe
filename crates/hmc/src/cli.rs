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
//!
//! The words of the help come from the `help.*` keys of the catalogs, so that `hmc`
//! explains itself in the language of the config it will read. The doc comments below
//! are the English of those keys, and a test keeps the two the same.

use std::ffi::OsString;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Arg, ArgAction, Command, CommandFactory, FromArgMatches, Parser};
use hiveme_core::i18n::{Locale, t, t_with};

use crate::failure::{Failure, Result};

/// The levels `--level` accepts, which are the ones `hiveme_core::Level` knows.
fn known_levels() -> impl Iterator<Item = &'static str> {
  static LEVELS: [hiveme_core::Level; 4] = hiveme_core::Level::known();
  LEVELS.iter().map(hiveme_core::Level::as_str)
}

/// Each argument of [`Cli`] and the `help.*` key that describes it.
const ARGUMENT_HELP: [(&str, &str); 12] = [
  ("message", "help.message"),
  ("init", "help.init"),
  ("tui", "help.tui"),
  ("topic", "help.topic"),
  ("json", "help.json"),
  ("title", "help.title"),
  ("level", "help.level"),
  ("qos", "help.qos"),
  ("retain", "help.retain"),
  ("no_retain", "help.noRetain"),
  ("config", "help.config"),
  ("verbose", "help.verbose"),
];

/// Send a message to the MQTT broker.
#[derive(Debug, Parser)]
#[command(
  name = "hmc",
  version,
  about,
  long_about = None,
  after_help = "Run hmc with no arguments on a terminal to open the terminal UI."
)]
pub struct Cli {
  /// Message body. Read from stdin when omitted.
  pub message: Option<String>,

  /// Initialize the shared config from a setup string, then exit
  #[arg(long, value_name = "JSON", conflicts_with_all = ["message", "topic", "json", "title", "level", "qos", "retain", "no_retain"])]
  pub init: Option<String>,

  /// Open the terminal UI, even when stdin is not a terminal
  #[arg(long, conflicts_with_all = ["message", "init", "topic", "json", "title", "level", "qos", "retain", "no_retain"])]
  pub tui: bool,

  /// Topic relative to hiveme; leading slashes are ignored [default: hiveme]
  #[arg(short = 't', long, value_name = "TOPIC")]
  pub topic: Option<String>,

  /// Publish MESSAGE (or stdin) as a raw JSON payload without the envelope
  #[arg(long, conflicts_with_all = ["title", "level"])]
  pub json: bool,

  /// Optional title for the message
  #[arg(long, value_name = "TITLE")]
  pub title: Option<String>,

  /// info | success | warn | error [default: info; independent of topic]
  #[arg(short = 'l', long, value_name = "LEVEL", value_parser = PossibleValuesParser::new(known_levels()).map(|level| level.to_ascii_lowercase()), ignore_case = true, hide_possible_values = true)]
  pub level: Option<String>,

  /// 0 | 1 | 2 [default: publish.qos]
  #[arg(short = 'q', long, value_name = "QOS", value_parser = clap::value_parser!(u8).range(0..=2))]
  pub qos: Option<u8>,

  /// Set the retain flag
  #[arg(short = 'r', long)]
  pub retain: bool,

  /// Clear the retain flag
  #[arg(long, conflicts_with = "retain")]
  pub no_retain: bool,

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
  pub fn retained(&self, default: bool) -> bool {
    !self.no_retain && (self.retain || default)
  }

  /// Whether this run opens the terminal UI rather than publishing.
  ///
  /// `--tui` always does. Otherwise the command line has to be empty of everything a
  /// publish or an init reads, and stdin a terminal: a body piped in is still published,
  /// so `echo hi | hmc` keeps working. `--config` and `--verbose` apply to both modes.
  pub fn is_interactive(&self, stdin_is_terminal: bool) -> bool {
    self.tui
      || (stdin_is_terminal
        && self.message.is_none()
        && self.init.is_none()
        && self.topic.is_none()
        && !self.json
        && self.title.is_none()
        && self.level.is_none()
        && self.qos.is_none()
        && !self.retain
        && !self.no_retain)
  }

  /// Parses the command line, printing help, the version, or a usage error in
  /// `locale` and exiting when that is what it holds.
  pub fn parse_in(locale: Locale, arguments: impl IntoIterator<Item = OsString>) -> Self {
    let matches = command(locale).get_matches_from(arguments);
    Self::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
  }

  /// The message body, from the argument or from stdin.
  ///
  /// Trailing whitespace is dropped, because the everyday way to reach this is
  /// `echo hi | hmc` and the newline `echo` adds is not part of what the user wrote.
  pub fn body(&self, locale: Locale) -> Result<Body> {
    match self.message.as_deref() {
      Some(message) => Self::finish(message, false, locale),
      None => {
        if std::io::stdin().is_terminal() {
          return Err(Failure::Usage(t(locale, "cli.noMessage")));
        }
        let mut text = String::new();
        std::io::stdin()
          .read_to_string(&mut text)
          .map_err(|source| Failure::Usage(t_with(locale, "cli.stdinUnreadable", &[("error", &source.to_string())])))?;
        Self::finish(&text, true, locale)
      }
    }
  }

  fn finish(text: &str, from_stdin: bool, locale: Locale) -> Result<Body> {
    let text = text.trim_end();
    if text.is_empty() {
      let key = if from_stdin {
        "cli.stdinEmpty"
      } else {
        "cli.messageEmpty"
      };
      return Err(Failure::Usage(t(locale, key)));
    }
    Ok(Body {
      text: text.to_owned(),
      from_stdin,
    })
  }
}

/// The command line of [`Cli`], with its help in `locale`.
///
/// clap writes the `Arguments` and `Options` headings, the `Usage` heading, and the
/// descriptions of `--help` and `--version` in English itself. So every argument is
/// given a heading of its own, which clap prints as written, the usage heading is part
/// of the template, and the two built-in flags are replaced by ones that carry
/// translated descriptions. The English rendering is byte for byte clap's default.
pub fn command(locale: Locale) -> Command {
  let arguments = t(locale, "help.arguments");
  let options = t(locale, "help.options");
  let mut command = Cli::command();
  let usage = command.get_styles().get_usage();
  let template = format!(
    "{{before-help}}{{about-with-newline}}\n{}{}:{} {{usage}}\n\n{{all-args}}{{after-help}}",
    usage.render(),
    t(locale, "help.usage"),
    usage.render_reset()
  );
  command = command
    .about(t(locale, "help.about"))
    .after_help(t(locale, "help.interactive"))
    .help_template(template)
    .disable_help_flag(true)
    .disable_version_flag(true);
  for (id, key) in ARGUMENT_HELP {
    let heading = if id == "message" { &arguments } else { &options };
    command = command.mut_arg(id, |arg| arg.help(t(locale, key)).help_heading(heading.clone()));
  }
  command
    .arg(
      Arg::new("help")
        .short('h')
        .long("help")
        .action(ArgAction::Help)
        .help(t(locale, "help.printHelp"))
        .help_heading(options.clone()),
    )
    .arg(
      Arg::new("version")
        .short('V')
        .long("version")
        .action(ArgAction::Version)
        .help(t(locale, "help.printVersion"))
        .help_heading(options),
    )
}

/// The `--config` path on a command line clap has not parsed yet.
///
/// The language of the help and of a usage error is the one in the config, so the path
/// has to be known before clap is asked to print either. This reads the arguments the
/// way clap does: `--config PATH`, `--config=PATH`, `-c PATH`, `-cPATH`, `-c=PATH`, and
/// `-c` at the end of a cluster such as `-vc PATH`, skipping the values of the other
/// options and stopping at `--`. A command line clap will refuse anyway may give the
/// wrong answer, which costs only the language of the refusal.
pub fn config_argument(arguments: &[OsString]) -> Option<PathBuf> {
  let command = Cli::command();
  let takes_value = |arg: &Arg| arg.get_action().takes_values();
  let long_values: Vec<&str> = command
    .get_arguments()
    .filter(|arg| takes_value(arg))
    .filter_map(Arg::get_long)
    .collect();
  let short_values: Vec<char> = command
    .get_arguments()
    .filter(|arg| takes_value(arg))
    .filter_map(Arg::get_short)
    .collect();

  let mut found = None;
  let mut iterator = arguments.iter().skip(1);
  while let Some(argument) = iterator.next() {
    let Some(text) = argument.to_str() else {
      continue;
    };
    if text == "--" {
      break;
    }
    if let Some(long) = text.strip_prefix("--") {
      match long.split_once('=') {
        Some(("config", value)) => found = Some(PathBuf::from(value)),
        Some(_) => {}
        None if long == "config" => found = iterator.next().map(PathBuf::from),
        None if long_values.contains(&long) => {
          iterator.next();
        }
        None => {}
      }
    } else if let Some(cluster) = text.strip_prefix('-').filter(|cluster| !cluster.is_empty()) {
      for (index, short) in cluster.char_indices() {
        if !short_values.contains(&short) {
          continue;
        }
        let rest = &cluster[index + short.len_utf8()..];
        let value = if rest.is_empty() {
          iterator.next().cloned()
        } else {
          Some(OsString::from(rest.strip_prefix('=').unwrap_or(rest)))
        };
        if short == 'c' {
          found = value.map(PathBuf::from);
        }
        break;
      }
    }
  }
  found
}

#[cfg(test)]
mod tests {
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

  fn rendered_help(locale: Locale) -> String {
    command(locale).render_help().to_string().replace("\r\n", "\n")
  }

  fn parse(arguments: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(arguments)
  }

  fn arguments(line: &[&str]) -> Vec<OsString> {
    std::iter::once("hmc")
      .chain(line.iter().copied())
      .map(OsString::from)
      .collect()
  }

  #[test]
  fn cli_help_matches_spec() {
    let expected = help_from_spec();
    let actual = rendered_help(Locale::EnUs);
    assert_eq!(
      actual.trim_end(),
      expected.trim_end(),
      "\n--- docs/specs/cli.md ---\n{expected}\n--- hmc --help ---\n{actual}\n\
       Copy the rendered help into the `text hiveme:help` block of docs/specs/cli.md."
    );
  }

  #[test]
  fn the_doc_comments_are_the_english_catalog() {
    // clap's own rendering of the derive, headings and built-in flags included, is the
    // English one, so neither the catalog nor the doc comments can drift alone.
    assert_eq!(
      Cli::command().render_help().to_string(),
      command(Locale::EnUs).render_help().to_string()
    );
  }

  #[test]
  fn the_help_is_translated_into_every_language() {
    for locale in Locale::ALL {
      let help = rendered_help(locale);
      assert!(help.starts_with(&t(locale, "help.about")), "{locale}:\n{help}");
      assert!(
        help.trim_end().ends_with(&t(locale, "help.interactive")),
        "{locale}:
{help}"
      );
      for key in ["help.usage", "help.arguments", "help.options"] {
        let heading = format!("{}:", t(locale, key));
        assert!(help.contains(&heading), "{locale} has no {heading}:\n{help}");
      }
      for (_, key) in ARGUMENT_HELP {
        assert!(help.contains(&t(locale, key)), "{locale} lacks {key}:\n{help}");
      }
      assert!(help.contains(&t(locale, "help.printHelp")), "{locale}:\n{help}");
      assert!(help.contains(&t(locale, "help.printVersion")), "{locale}:\n{help}");
      if locale != Locale::EnUs {
        for english in ["Usage:", "Print help", "Message body"] {
          assert!(!help.contains(english), "{locale} still says {english}:\n{help}");
        }
      }
    }
  }

  #[test]
  fn the_help_still_names_every_option() {
    for locale in Locale::ALL {
      let help = rendered_help(locale);
      for flag in [
        "--init",
        "--tui",
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
        assert!(help.contains(flag), "{flag} is missing from the {locale} help:\n{help}");
      }
    }
  }

  #[test]
  fn the_definition_is_internally_consistent() {
    Cli::command().debug_assert();
    for locale in Locale::ALL {
      command(locale).debug_assert();
    }
  }

  #[test]
  fn a_translated_command_line_parses_as_the_english_one_does() {
    let line = arguments(&["-t", "error", "-l", "WARN", "-q", "2", "-r", "-c", "x.json", "boom"]);
    let matches = command(Locale::De).try_get_matches_from(&line).unwrap();
    let cli = Cli::from_arg_matches(&matches).unwrap();
    assert_eq!(cli.topic.as_deref(), Some("error"));
    assert_eq!(cli.level.as_deref(), Some("warn"));
    assert_eq!(cli.qos, Some(2));
    assert!(cli.retain);
    assert_eq!(cli.message.as_deref(), Some("boom"));

    for (flag, kind) in [
      ("--help", clap::error::ErrorKind::DisplayHelp),
      ("-h", clap::error::ErrorKind::DisplayHelp),
      ("--version", clap::error::ErrorKind::DisplayVersion),
      ("-V", clap::error::ErrorKind::DisplayVersion),
    ] {
      let error = command(Locale::Ja)
        .try_get_matches_from(arguments(&[flag]))
        .unwrap_err();
      assert_eq!(error.kind(), kind, "{flag}");
    }
    let error = command(Locale::Fr)
      .try_get_matches_from(arguments(&["--init", "{}", "hello"]))
      .unwrap_err();
    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
  }

  #[test]
  fn the_config_path_is_found_before_parsing() {
    let path = Some(PathBuf::from("x.json"));
    for line in [
      vec!["--config", "x.json"],
      vec!["--config=x.json"],
      vec!["-c", "x.json"],
      vec!["-cx.json"],
      vec!["-c=x.json"],
      vec!["-vc", "x.json"],
      vec!["--help", "-c", "x.json"],
      vec!["-t", "error", "-c", "x.json", "hello"],
      vec!["--title", "-c", "--config", "x.json"],
      vec!["-t-c", "--config", "x.json"],
      vec!["-c", "first.json", "-c", "x.json"],
    ] {
      assert_eq!(config_argument(&arguments(&line)), path, "{line:?}");
    }
    for line in [
      vec![],
      vec!["hello"],
      vec!["--", "-c", "x.json"],
      vec!["-t", "-c"],
      vec!["--topic", "--config"],
      vec!["--init", "-c"],
      vec!["--configuration", "x.json"],
    ] {
      assert_eq!(config_argument(&arguments(&line)), None, "{line:?}");
    }
    // The program name is never an argument, whatever it looks like.
    assert_eq!(
      config_argument(&[OsString::from("-cx.json"), OsString::from("hello")]),
      None
    );
  }

  #[test]
  fn a_message_argument_is_taken_as_written() {
    let cli = Cli::parse_from(["hmc", "the build finished"]);
    let body = cli.body(Locale::EnUs).unwrap();
    assert_eq!(body.text, "the build finished");
    assert!(!body.from_stdin);
  }

  #[test]
  fn a_trailing_newline_is_not_part_of_the_body() {
    let finish = |text, from_stdin| Cli::finish(text, from_stdin, Locale::EnUs).unwrap().text;
    assert_eq!(finish("the build finished\n", true), "the build finished");
    assert_eq!(finish("two\nlines\n", true), "two\nlines");
    // Leading whitespace is the user's, so it stays.
    assert_eq!(finish("  indented  ", false), "  indented");
  }

  #[test]
  fn an_empty_body_is_a_usage_error() {
    let failure = Cli::finish("   \n", true, Locale::EnUs).unwrap_err();
    assert_eq!(failure.code(), 2);
    assert_eq!(failure.line(), "hmc: usage: nothing to publish: stdin was empty");
    assert_eq!(Cli::finish("", false, Locale::EnUs).unwrap_err().code(), 2);
  }

  #[test]
  fn a_usage_error_hmc_writes_is_in_the_language_but_its_prefix_is_not() {
    let failure = Cli::finish("", false, Locale::De).unwrap_err();
    assert_eq!(failure.line(), "hmc: usage: nichts zu senden: die Nachricht ist leer");
    let failure = Cli::finish("", true, Locale::ZhCn).unwrap_err();
    assert_eq!(failure.line(), "hmc: usage: 没有可发布的内容：stdin 为空");
  }

  #[test]
  fn publishing_has_no_absolute_topic_mode() {
    for flag in ["-T", "--absolute-topic"] {
      let error = parse(&["hmc", flag, "-t", "/ci", "hello"]).unwrap_err();
      assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }
  }

  #[test]
  fn json_has_no_room_for_a_title_or_a_level() {
    for arguments in [
      vec!["hmc", "--json", "--title", "CI", "{}"],
      vec!["hmc", "--json", "--level", "warn", "{}"],
    ] {
      let error = parse(&arguments).unwrap_err();
      assert_eq!(
        error.kind(),
        clap::error::ErrorKind::ArgumentConflict,
        "{arguments:?} should conflict"
      );
    }
  }

  #[test]
  fn a_level_the_message_format_does_not_define_is_refused() {
    for input in ["critical", "debug", "DEBUG", "DeBuG"] {
      let error = parse(&["hmc", "-l", input, "boom"]).unwrap_err();
      assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue, "{input}");
    }
    for level in known_levels() {
      for input in [
        level.to_owned(),
        level.to_uppercase(),
        format!("{}{}", level[..1].to_uppercase(), &level[1..]),
      ] {
        let cli = parse(&["hmc", "-l", &input, "boom"]).unwrap();
        assert_eq!(cli.level.as_deref(), Some(level), "{input}");
      }
    }
  }

  #[test]
  fn a_quality_of_service_mqtt_does_not_define_is_refused() {
    assert!(parse(&["hmc", "-q", "3", "boom"]).is_err());
    for qos in ["0", "1", "2"] {
      assert!(parse(&["hmc", "-q", qos, "boom"]).is_ok(), "{qos}");
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
      let error = parse(&conflicting).unwrap_err();
      assert_eq!(
        error.kind(),
        clap::error::ErrorKind::ArgumentConflict,
        "{conflicting:?} should conflict"
      );
    }
  }

  #[test]
  fn tui_is_a_mode_of_its_own_and_refuses_what_it_would_ignore() {
    for conflicting in [
      vec!["hmc", "--tui", "hello"],
      vec!["hmc", "--tui", "--init", "{}"],
      vec!["hmc", "--tui", "-t", "error"],
      vec!["hmc", "--tui", "--json"],
      vec!["hmc", "--tui", "--title", "CI"],
      vec!["hmc", "--tui", "-l", "warn"],
      vec!["hmc", "--tui", "-q", "1"],
      vec!["hmc", "--tui", "-r"],
    ] {
      let error = parse(&conflicting).unwrap_err();
      assert_eq!(
        error.kind(),
        clap::error::ErrorKind::ArgumentConflict,
        "{conflicting:?} should conflict"
      );
    }
    let cli = Cli::parse_from(["hmc", "--tui", "-c", "x.json", "-v"]);
    assert!(cli.tui && cli.verbose);
    assert_eq!(cli.config.as_deref(), Some(std::path::Path::new("x.json")));
  }

  #[test]
  fn the_terminal_ui_opens_only_for_an_empty_command_line_on_a_terminal() {
    assert!(Cli::parse_from(["hmc"]).is_interactive(true));
    assert!(Cli::parse_from(["hmc", "-c", "x.json", "-v"]).is_interactive(true));
    assert!(
      !Cli::parse_from(["hmc"]).is_interactive(false),
      "a piped body is published"
    );
    assert!(Cli::parse_from(["hmc", "--tui"]).is_interactive(false));
    for publishing in [
      vec!["hmc", "hello"],
      vec!["hmc", "--init", "{}"],
      vec!["hmc", "-t", "ci"],
      vec!["hmc", "--json"],
      vec!["hmc", "--title", "CI"],
      vec!["hmc", "-l", "warn"],
      vec!["hmc", "-q", "1"],
      vec!["hmc", "-r"],
    ] {
      assert!(!Cli::parse_from(&publishing).is_interactive(true), "{publishing:?}");
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
