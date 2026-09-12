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

//! The reconnect delay of the `hmg` connection.
//!
//! Exponential with equal jitter: the delay doubles until `broker.reconnect.maxDelayMs`
//! and then half of it is randomised. The jitter matters because several HiveMe
//! installations share a cluster and a cluster restart would otherwise bring them all
//! back at the same instant.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::Reconnect;

/// Produces the delay before each reconnect attempt.
#[derive(Debug, Clone)]
pub struct Backoff {
  initial: Duration,
  max: Duration,
  attempt: u32,
  jitter: Jitter,
}

impl Backoff {
  /// A backoff over the delays in `reconnect`.
  ///
  /// An initial delay above the maximum is clamped rather than rejected, because
  /// [`Config::validate`](crate::config::Config::validate) already reports the pair and
  /// a connection should not also fail over it.
  pub fn from_config(reconnect: &Reconnect) -> Self {
    Self::new(
      Duration::from_millis(reconnect.initial_delay_ms),
      Duration::from_millis(reconnect.max_delay_ms),
    )
  }

  /// A backoff seeded from the clock.
  pub fn new(initial: Duration, max: Duration) -> Self {
    Self::with_seed(initial, max, seed_from_clock())
  }

  /// A backoff with a fixed jitter seed, so that a test can predict the sequence.
  pub fn with_seed(initial: Duration, max: Duration, seed: u64) -> Self {
    let initial = initial.max(Duration::from_millis(1));
    Self {
      initial,
      max: max.max(initial),
      attempt: 0,
      jitter: Jitter::new(seed),
    }
  }

  /// How many delays have been handed out since the last [`Backoff::reset`].
  pub fn attempt(&self) -> u32 {
    self.attempt
  }

  /// Forgets the attempts, which a successful connection does.
  pub fn reset(&mut self) {
    self.attempt = 0;
  }

  /// The delay before the next attempt, half of it randomised.
  pub fn next_delay(&mut self) -> Duration {
    let ceiling = self.ceiling();
    self.attempt = self.attempt.saturating_add(1);
    let half = ceiling / 2;
    let spread = ceiling.saturating_sub(half).as_millis() as u64;
    half + Duration::from_millis(self.jitter.below(spread + 1))
  }

  /// The undithered delay of the next attempt: `initial * 2^attempt`, capped.
  fn ceiling(&self) -> Duration {
    let factor = 1u64.checked_shl(self.attempt.min(63)).unwrap_or(u64::MAX);
    self
      .initial
      .checked_mul(factor.min(u32::MAX as u64) as u32)
      .unwrap_or(self.max)
      .min(self.max)
  }
}

/// The seed for a fresh [`Backoff`].
///
/// The clock is enough: the point is that two installations differ, not that the
/// sequence is unpredictable to an attacker.
fn seed_from_clock() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|since| since.as_nanos() as u64)
    .unwrap_or(0x9e37_79b9_7f4a_7c15)
    | 1
}

/// A xorshift64\* generator, so that jitter costs no dependency.
#[derive(Debug, Clone)]
struct Jitter {
  state: u64,
}

impl Jitter {
  fn new(seed: u64) -> Self {
    Self {
      // The generator is stuck at zero, so a zero seed becomes the golden ratio.
      state: if seed == 0 { 0x9e37_79b9_7f4a_7c15 } else { seed },
    }
  }

  fn next_u64(&mut self) -> u64 {
    let mut state = self.state;
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    self.state = state;
    state.wrapping_mul(0x2545_f491_4f6c_dd1d)
  }

  /// A value in `0..bound`, or zero when `bound` is zero.
  fn below(&mut self, bound: u64) -> u64 {
    if bound == 0 { 0 } else { self.next_u64() % bound }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_delay_doubles_until_the_maximum() {
    let mut backoff = Backoff::with_seed(Duration::from_millis(1_000), Duration::from_millis(30_000), 7);
    let ceilings = [1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000];
    for ceiling in ceilings {
      let delay = backoff.next_delay().as_millis() as u64;
      assert!(
        (ceiling / 2..=ceiling).contains(&delay),
        "delay {delay} is outside {}..={ceiling}",
        ceiling / 2
      );
    }
    assert_eq!(backoff.attempt(), ceilings.len() as u32);
  }

  #[test]
  fn a_reset_starts_the_sequence_again() {
    let mut backoff = Backoff::with_seed(Duration::from_millis(1_000), Duration::from_millis(30_000), 7);
    for _ in 0..5 {
      backoff.next_delay();
    }
    backoff.reset();
    assert_eq!(backoff.attempt(), 0);
    assert!(backoff.next_delay() <= Duration::from_millis(1_000));
  }

  #[test]
  fn the_jitter_moves_the_delay() {
    let mut backoff = Backoff::with_seed(Duration::from_millis(1_000), Duration::from_millis(30_000), 7);
    let mut delays = std::collections::HashSet::new();
    for _ in 0..16 {
      delays.insert(backoff.next_delay());
      backoff.reset();
    }
    assert!(delays.len() > 1, "every delay was identical, the jitter does nothing");
  }

  #[test]
  fn two_backoffs_with_different_seeds_diverge() {
    let mut one = Backoff::with_seed(Duration::from_millis(1_000), Duration::from_millis(30_000), 1);
    let mut other = Backoff::with_seed(Duration::from_millis(1_000), Duration::from_millis(30_000), 2);
    let ones: Vec<Duration> = (0..8).map(|_| one.next_delay()).collect();
    let others: Vec<Duration> = (0..8).map(|_| other.next_delay()).collect();
    assert_ne!(ones, others);
  }

  #[test]
  fn the_config_delays_are_honoured() {
    let reconnect = Reconnect {
      initial_delay_ms: 250,
      max_delay_ms: 1_000,
    };
    let mut backoff = Backoff::from_config(&reconnect);
    for _ in 0..10 {
      let delay = backoff.next_delay();
      assert!(
        delay >= Duration::from_millis(125),
        "{delay:?} is below half the initial"
      );
      assert!(delay <= Duration::from_millis(1_000), "{delay:?} is above the maximum");
    }
  }

  #[test]
  fn an_initial_delay_above_the_maximum_is_clamped() {
    let mut backoff = Backoff::from_config(&Reconnect {
      initial_delay_ms: 5_000,
      max_delay_ms: 1_000,
    });
    assert!(backoff.next_delay() <= Duration::from_millis(5_000));
  }

  #[test]
  fn a_zero_delay_still_produces_something() {
    let mut backoff = Backoff::from_config(&Reconnect {
      initial_delay_ms: 0,
      max_delay_ms: 0,
    });
    assert!(backoff.next_delay() <= Duration::from_millis(1));
  }

  #[test]
  fn a_zero_seed_does_not_freeze_the_generator() {
    let mut jitter = Jitter::new(0);
    assert_ne!(jitter.next_u64(), jitter.next_u64());
  }
}
