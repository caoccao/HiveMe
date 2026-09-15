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

//! Private local IPC for the one notification window shared by both applications.
//! The endpoint contains no messages. An OS file lock prevents duplicate hosts;
//! a private random token authenticates each bounded JSON request on loopback.

use crate::session::TopmostNotification;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_FRAME: u64 = 1024 * 1024;
const START_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "content", rename_all = "camelCase")]
pub enum Request {
  System { title: String, body: String },
  Topmost(TopmostNotification),
}

#[derive(Serialize, Deserialize)]
struct Envelope {
  token: String,
  request: Request,
}

#[derive(Serialize, Deserialize)]
struct Endpoint {
  port: u16,
  token: String,
}

/// Tests can choose an isolated cache without opening the real config or history.
pub fn directory() -> Result<PathBuf, String> {
  if let Some(path) = std::env::var_os("HIVEME_NOTIFICATION_DIR") {
    return Ok(PathBuf::from(path));
  }
  // Directly opening the cached bundle keeps its endpoint in that same cache.
  if let Ok(executable) = std::env::current_exe()
    && let Some(bundle) = executable.ancestors().nth(3)
    && bundle
      .file_name()
      .is_some_and(|name| name == "HiveMe Notifications.app")
    && let Some(directory) = bundle.parent()
  {
    return Ok(directory.to_owned());
  }
  directories::BaseDirs::new()
    .map(|dirs| dirs.cache_dir().join("HiveMe/notifications"))
    .ok_or_else(|| "the notification cache directory is unavailable".to_owned())
}

fn private_directory(path: &Path) -> std::io::Result<()> {
  fs::create_dir_all(path)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
  }
  Ok(())
}

fn lock_file(path: &Path) -> std::io::Result<File> {
  let mut options = OpenOptions::new();
  options.create(true).truncate(false).read(true).write(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  options.open(path)
}

fn connect(directory: &Path) -> Result<(TcpStream, String), String> {
  let endpoint: Endpoint =
    serde_json::from_slice(&fs::read(directory.join("endpoint.json")).map_err(|e| e.to_string())?)
      .map_err(|e| e.to_string())?;
  let address = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
  let stream = TcpStream::connect_timeout(&address, Duration::from_millis(300)).map_err(|e| e.to_string())?;
  stream
    .set_read_timeout(Some(Duration::from_secs(30)))
    .map_err(|e| e.to_string())?;
  stream
    .set_write_timeout(Some(Duration::from_secs(5)))
    .map_err(|e| e.to_string())?;
  Ok((stream, endpoint.token))
}

fn executable() -> Result<PathBuf, String> {
  let current = std::env::current_exe().map_err(|e| e.to_string())?;
  let name = if cfg!(windows) { "hmg.exe" } else { "hmg" };
  let sibling = current.with_file_name(name);
  if sibling.is_file() {
    return Ok(sibling);
  }
  if let Some(paths) = std::env::var_os("PATH") {
    for directory in std::env::split_paths(&paths) {
      let path = directory.join(name);
      if path.is_file() {
        return Ok(path);
      }
    }
  }
  Err("topmost notifications require hmg beside hmc or on PATH".to_owned())
}

pub fn send(request: &Request) -> Result<(), String> {
  let directory = directory()?;
  send_with_launcher(&directory, request, || {
    let executable = executable()?;
    #[cfg(target_os = "macos")]
    let executable = super::macos::bundled_host(&directory, &executable)?;
    let mut command = Command::new(executable);
    command
      .arg("--notification-host")
      .env("HIVEME_NOTIFICATION_DIR", &directory)
      .stdin(Stdio::null())
      .stdout(Stdio::null());
    // Keep delivery diagnostics out of the terminal UI.
    let log = OpenOptions::new()
      .create(true)
      .append(true)
      .open(directory.join("host.log"))
      .map_err(|e| e.to_string())?;
    command.stderr(log);
    #[cfg(windows)]
    {
      use std::os::windows::process::CommandExt;
      command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
      let _ = child.wait();
    });
    Ok(())
  })
}

fn send_with_launcher(
  directory: &Path,
  request: &Request,
  launch: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
  private_directory(directory).map_err(|e| e.to_string())?;
  let (mut stream, token) = match connect(directory) {
    Ok(connection) => connection,
    Err(_) => {
      let launch_lock = lock_file(&directory.join("launch.lock")).map_err(|e| e.to_string())?;
      launch_lock.lock().map_err(|e| e.to_string())?;
      let connection = match connect(directory) {
        Ok(connection) => connection,
        Err(_) => {
          let mut launch = Some(launch);
          let start = Instant::now();
          loop {
            if let Ok(connection) = connect(directory) {
              break connection;
            }
            if launch.is_some() {
              let host_lock = lock_file(&directory.join("host.lock")).map_err(|e| e.to_string())?;
              match host_lock.try_lock() {
                Ok(()) => {
                  drop(host_lock);
                  launch.take().expect("the launcher is present")()?;
                }
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(error) => return Err(error.to_string()),
              }
            }
            if start.elapsed() >= START_TIMEOUT {
              return Err("the notification host did not start; see its host.log".to_owned());
            }
            std::thread::sleep(Duration::from_millis(50));
          }
        }
      };
      drop(launch_lock);
      connection
    }
  };
  write_frame(
    &mut stream,
    &Envelope {
      token,
      request: request.clone(),
    },
  )?;
  read_frame::<Result<(), String>>(&mut stream)?
}

fn write_frame(stream: &mut TcpStream, value: &impl Serialize) -> Result<(), String> {
  let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
  if bytes.len() as u64 >= MAX_FRAME {
    return Err("notification exceeds the 1 MiB delivery limit".to_owned());
  }
  stream
    .write_all(&bytes)
    .and_then(|_| stream.write_all(b"\n"))
    .map_err(|e| e.to_string())
}

fn read_frame<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> Result<T, String> {
  let mut bytes = Vec::new();
  BufReader::new(stream.take(MAX_FRAME))
    .read_until(b'\n', &mut bytes)
    .map_err(|e| e.to_string())?;
  if bytes.last() != Some(&b'\n') {
    return Err("incomplete or oversized notification request".to_owned());
  }
  serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

pub struct Server {
  pub listener: TcpListener,
  token: String,
  _lock: File,
}

impl Server {
  /// None means another host already owns this desktop's notification window.
  pub fn bind(directory: &Path) -> Result<Option<Self>, String> {
    private_directory(directory).map_err(|e| e.to_string())?;
    let lock = lock_file(&directory.join("host.lock")).map_err(|e| e.to_string())?;
    match lock.try_lock() {
      Ok(()) => {}
      Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
      Err(error) => return Err(error.to_string()),
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    let token = format!("{}{}", uuid::Uuid::now_v7(), uuid::Uuid::now_v7());
    let endpoint = Endpoint {
      port: listener.local_addr().map_err(|e| e.to_string())?.port(),
      token: token.clone(),
    };
    let mut file = lock_file(&directory.join("endpoint.json")).map_err(|e| e.to_string())?;
    file.set_len(0).map_err(|e| e.to_string())?;
    file
      .write_all(&serde_json::to_vec(&endpoint).map_err(|e| e.to_string())?)
      .map_err(|e| e.to_string())?;
    Ok(Some(Self {
      listener,
      token,
      _lock: lock,
    }))
  }

  pub fn receive(&self, stream: &mut TcpStream) -> Result<Request, String> {
    stream
      .set_read_timeout(Some(Duration::from_secs(3)))
      .map_err(|e| e.to_string())?;
    stream
      .set_write_timeout(Some(Duration::from_secs(3)))
      .map_err(|e| e.to_string())?;
    let envelope: Envelope = read_frame(stream)?;
    if envelope.token != self.token {
      return Err("notification host authentication failed".to_owned());
    }
    Ok(envelope.request)
  }

  pub fn reply(stream: &mut TcpStream, result: Result<(), String>) {
    if let Err(error) = write_frame(stream, &result) {
      log::warn!("notification response: {error}");
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn one_host_receives_both_clients_and_returns_real_delivery_failures() {
    let directory = tempfile::tempdir().unwrap();
    let server = Server::bind(directory.path()).unwrap().unwrap();
    assert!(Server::bind(directory.path()).unwrap().is_none());
    let worker = std::thread::spawn(move || {
      for (index, connection) in server.listener.incoming().take(2).enumerate() {
        let mut stream = connection.unwrap();
        let request = server.receive(&mut stream).unwrap();
        assert!(matches!(request, Request::System { body, .. } if body == format!("message {index}")));
        Server::reply(
          &mut stream,
          if index == 0 {
            Ok(())
          } else {
            Err("permission denied".to_owned())
          },
        );
      }
    });
    for index in 0..2 {
      let result = send_with_launcher(
        directory.path(),
        &Request::System {
          title: "test".to_owned(),
          body: format!("message {index}"),
        },
        || panic!("existing host must be reused"),
      );
      if index == 0 {
        assert!(result.is_ok());
      } else {
        assert_eq!(result.unwrap_err(), "permission denied");
      }
    }
    worker.join().unwrap();
    assert!(Server::bind(directory.path()).unwrap().is_some());
  }

  #[test]
  fn a_stale_endpoint_launches_a_host_and_delivers_the_original_message() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("endpoint.json"), "stale endpoint").unwrap();
    let mut worker = None;
    let result = send_with_launcher(
      directory.path(),
      &Request::System {
        title: "test".into(),
        body: "first message".into(),
      },
      || {
        let server = Server::bind(directory.path()).unwrap().unwrap();
        worker = Some(std::thread::spawn(move || {
          let (mut stream, _) = server.listener.accept().unwrap();
          assert!(
            matches!(server.receive(&mut stream).unwrap(), Request::System { body, .. } if body == "first message")
          );
          Server::reply(&mut stream, Ok(()));
        }));
        Ok(())
      },
    );
    result.unwrap();
    worker.unwrap().join().unwrap();
  }

  #[test]
  fn an_unauthenticated_request_never_reaches_the_notification_handler() {
    let directory = tempfile::tempdir().unwrap();
    let server = Server::bind(directory.path()).unwrap().unwrap();
    let (mut stream, _) = connect(directory.path()).unwrap();
    write_frame(
      &mut stream,
      &Envelope {
        token: "wrong".to_owned(),
        request: Request::System {
          title: "test".to_owned(),
          body: "test".to_owned(),
        },
      },
    )
    .unwrap();
    let (mut incoming, _) = server.listener.accept().unwrap();
    assert!(server.receive(&mut incoming).unwrap_err().contains("authentication"));
  }
}
