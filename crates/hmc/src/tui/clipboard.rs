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

//! Copying text: the native clipboard through `arboard`, then the OSC 52 escape
//! sequence, which asks the terminal itself to set the clipboard. OSC 52 is what works
//! over SSH, where no native clipboard is reachable, and in most terminals that run
//! locally as well.

use std::io::{self, Write};
use std::sync::Mutex;

/// The native clipboard, kept open for the life of the process. On X11 and Wayland the
/// copied text is served by whoever owns the clipboard, so closing it right after a copy
/// could lose the text before anyone pasted it.
static NATIVE: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Puts `text` on the clipboard.
pub fn copy_text(text: &str) -> Result<(), String> {
  let native = {
    let mut clipboard = NATIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if clipboard.is_none() {
      *clipboard = arboard::Clipboard::new().ok();
    }
    match clipboard.as_mut() {
      Some(clipboard) => clipboard.set_text(text).map_err(|error| error.to_string()),
      None => Err("no native clipboard".to_owned()),
    }
  };
  match native {
    Ok(()) => Ok(()),
    Err(reason) => {
      log::debug!("the native clipboard did not take the text ({reason}), asking the terminal");
      let mut stdout = io::stdout();
      stdout
        .write_all(osc52(text).as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|error| error.to_string())
    }
  }
}

/// The escape sequence that asks a terminal to put `text` on the system clipboard.
pub fn osc52(text: &str) -> String {
  format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Standard base64 with padding, which is what OSC 52 carries.
fn base64(bytes: &[u8]) -> String {
  const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
  for chunk in bytes.chunks(3) {
    let value = (u32::from(chunk[0]) << 16)
      | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
      | u32::from(*chunk.get(2).unwrap_or(&0));
    for index in 0..4 {
      if index <= chunk.len() {
        out.push(char::from(ALPHABET[(value >> (18 - 6 * index) & 0x3f) as usize]));
      } else {
        out.push('=');
      }
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_terminal_is_handed_the_text_in_base64() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64("Disk full ✓".as_bytes()), "RGlzayBmdWxsIOKckw==");
    assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
  }
}
