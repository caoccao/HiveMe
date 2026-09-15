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

//! `hmg`, the HiveMe GUI.
//!
//! This file is the IPC surface and nothing else: one thin `#[tauri::command]` per
//! call, alphabetized, each delegating to [`controller`] and turning its error into a
//! string for the frontend. The contract is written in `docs/specs/gui.md`.

use std::sync::Arc;

use hiveme_core::session::{Session, SessionApp};

mod constants;
mod controller;
mod events;
mod notification;
mod notification_host;
mod protocol;
mod window;

use protocol::{AppState, MessageRow, PublishOptions, Status, TopicNode, UpdateCheckResult};

#[tauri::command]
async fn clear_topic(topic: String, state: tauri::State<'_, AppState>) -> Result<u64, String> {
  log::debug!("clear_topic({topic})");
  controller::clear_topic(&state.session, &topic).map_err(convert_error)
}

#[tauri::command]
async fn close_topmost_notification(window: tauri::WebviewWindow, revision: u64) -> Result<(), String> {
  notification_host::close(&window, revision).await
}

#[tauri::command]
async fn connect(state: tauri::State<'_, AppState>) -> Result<Status, String> {
  log::debug!("connect");
  controller::connect(&state.session).await.map_err(convert_error)
}

fn convert_error(error: anyhow::Error) -> String {
  error.to_string()
}

#[tauri::command]
async fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
  log::debug!("disconnect");
  controller::disconnect(&state.session).await.map_err(convert_error)
}

#[tauri::command]
async fn get_about(state: tauri::State<'_, AppState>) -> Result<protocol::About, String> {
  log::debug!("get_about");
  controller::get_about(&state.session).map_err(convert_error)
}

#[tauri::command]
async fn get_broker_init(state: tauri::State<'_, AppState>) -> Result<String, String> {
  // The setup string carries the broker password, so it is never logged.
  log::debug!("get_broker_init");
  controller::get_broker_init(&state.session).map_err(convert_error)
}

#[tauri::command]
async fn get_config(state: tauri::State<'_, AppState>) -> Result<hiveme_core::Config, String> {
  log::debug!("get_config");
  controller::get_config(&state.session).map_err(convert_error)
}

#[tauri::command]
async fn get_messages(
  topic: String,
  before: Option<i64>,
  limit: Option<u32>,
  state: tauri::State<'_, AppState>,
) -> Result<Vec<MessageRow>, String> {
  log::debug!("get_messages({topic}, {before:?}, {limit:?})");
  controller::get_messages(&state.session, &topic, before, limit.unwrap_or(0)).map_err(convert_error)
}

#[tauri::command]
async fn get_status(state: tauri::State<'_, AppState>) -> Result<Status, String> {
  log::debug!("get_status");
  controller::get_status(&state.session).map_err(convert_error)
}

#[tauri::command]
async fn get_topmost_notification(
  window: tauri::WebviewWindow,
  state: tauri::State<'_, notification_host::State>,
) -> Result<Option<protocol::TopmostSnapshot>, String> {
  notification_host::current(&window, &state)
}

#[tauri::command]
async fn get_update_result(state: tauri::State<'_, AppState>) -> Result<Option<UpdateCheckResult>, String> {
  log::debug!("get_update_result");
  Ok(controller::get_update_result(&state.session))
}

#[tauri::command]
async fn list_topics(state: tauri::State<'_, AppState>) -> Result<Vec<TopicNode>, String> {
  log::debug!("list_topics");
  controller::list_topics(&state.session).map_err(convert_error)
}

#[tauri::command]
async fn mark_read(topic: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
  log::debug!("mark_read({topic})");
  controller::mark_read(&state.session, &topic).map_err(convert_error)
}

#[tauri::command]
async fn publish(
  topic: String,
  body: String,
  options: Option<PublishOptions>,
  state: tauri::State<'_, AppState>,
) -> Result<MessageRow, String> {
  log::debug!("publish({topic}, {} byte(s))", body.len());
  controller::publish(&state.session, &topic, &body, options.unwrap_or_default())
    .await
    .map_err(convert_error)
}

#[tauri::command]
async fn ready_topmost_notification(window: tauri::WebviewWindow, revision: u64) -> Result<(), String> {
  notification_host::ready(&window, revision).await
}

/// Starts the GUI. Called by `main.rs`, which adds the Windows subsystem attribute.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  env_logger::init();
  let mut context = tauri::generate_context!();
  if notification_host::is_host_launch() {
    notification_host::run(context);
    return;
  }

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4)
    .enable_all()
    .build()
    .expect("the tokio runtime is built");
  tauri::async_runtime::set(runtime.handle().clone());

  // A config that cannot be read is not fatal: the session opens on defaults, the
  // status bar says so, and nothing is written until the user saves the Settings tab.
  // Corrupt or incompatible history is recreated by the store. Storage failures
  // that remain, such as a directory that is not writable, need a visible error.
  let toaster = Arc::new(notification::TauriToaster::new());
  let session = match Session::open(None, SessionApp::Gui, toaster.clone()) {
    Ok(session) => session,
    Err(error) => {
      let path = hiveme_core::config::config_path(None).map(|path| hiveme_core::config::database_path(&path));
      let path = path
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "HiveMe.db".to_owned());
      show_startup_error(startup_error_message(&path, &error.to_string()), context);
      return;
    }
  };
  if let Some(error) = session.load_error() {
    log::error!("the config could not be read: {error}");
  }

  // Before the builder, because the builder creates the window and a window that is moved
  // after it exists is drawn twice. See `window::place`.
  window::place(&mut context, &session);

  tauri::Builder::default()
    .manage(AppState { session })
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .setup(window::setup)
    .on_window_event(window::on_window_event)
    .invoke_handler(tauri::generate_handler![
      clear_topic,
      connect,
      disconnect,
      get_about,
      get_broker_init,
      get_config,
      get_messages,
      get_status,
      get_update_result,
      list_topics,
      mark_read,
      publish,
      set_config,
      set_notifications_paused,
      skip_version
    ])
    .build(context)
    .expect("the application builds")
    .run(window::on_run_event);
}

#[tauri::command]
async fn set_config(
  config: hiveme_core::Config,
  state: tauri::State<'_, AppState>,
) -> Result<hiveme_core::Config, String> {
  // The config carries the broker password, so the value itself is never logged.
  log::debug!("set_config");
  controller::set_config(&state.session, config)
    .await
    .map_err(convert_error)
}

#[tauri::command]
async fn set_notifications_paused(paused: bool, state: tauri::State<'_, AppState>) -> Result<Status, String> {
  log::debug!("set_notifications_paused({paused})");
  controller::set_notifications_paused(&state.session, paused).map_err(convert_error)
}

#[tauri::command]
async fn skip_version(version: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
  log::debug!("skip_version({version})");
  controller::skip_version(&state.session, &version).map_err(convert_error)
}

/// A startup failure remains visible even in Windows builds without a console.
fn show_startup_error(message: String, mut context: tauri::Context<tauri::Wry>) {
  use tauri_plugin_dialog::DialogExt;
  context.config_mut().app.windows.clear();
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .setup(move |app| {
      // Keep the native event loop alive until the asynchronous dialog is dismissed.
      // This hidden host loads no application code or session commands.
      tauri::WebviewWindowBuilder::new(
        app,
        "startup-error",
        tauri::WebviewUrl::External("about:blank".parse()?),
      )
      .title("HiveMe")
      .visible(false)
      .build()?;
      let handle = app.handle().clone();
      app
        .dialog()
        .message(&message)
        .title("HiveMe")
        .kind(tauri_plugin_dialog::MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
      Ok(())
    })
    .build(context)
    .expect("the startup error dialog application is built")
    .run_return(|_, _| {});
  // Native modal loops may return their own platform code; startup still failed.
  std::process::exit(1);
}

fn startup_error_message(path: &str, error: &str) -> String {
  format!("HiveMe could not open its history at {path}.\n\n{error}")
}

#[cfg(test)]
mod review_tests {
  #[test]
  fn startup_error_names_the_history_and_underlying_reason() {
    let message = super::startup_error_message("/tmp/isolated/HiveMe.db", "file is not a database");
    assert!(message.contains("/tmp/isolated/HiveMe.db"));
    assert!(message.contains("file is not a database"));
  }

  #[test]
  fn webview_policy_keeps_ipc_and_styles_without_remote_scripts_or_clipboard_reads() {
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let csp = config["app"]["security"]["csp"].as_str().expect("CSP is enabled");
    let directives: Vec<_> = csp.split(';').map(str::trim).collect();
    assert!(directives.contains(&"default-src 'self'"));
    assert!(directives.contains(&"connect-src 'self' ipc: http://ipc.localhost"));
    assert!(directives.contains(&"style-src 'self' 'unsafe-inline'"));
    assert!(!csp.contains("unsafe-eval"));
    let capability: serde_json::Value = serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
    let permissions = capability["permissions"].as_array().unwrap();
    assert!(permissions.contains(&serde_json::json!("clipboard-manager:allow-write-text")));
    assert!(!permissions.contains(&serde_json::json!("clipboard-manager:allow-read-text")));
  }
}
