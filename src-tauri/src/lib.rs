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
mod protocol;
mod window;

use protocol::{AppState, MessageRow, PublishOptions, Status, TopicNode, UpdateCheckResult};

#[tauri::command]
async fn clear_topic(topic: String, state: tauri::State<'_, AppState>) -> Result<u64, String> {
  log::debug!("clear_topic({topic})");
  controller::clear_topic(&state.session, &topic).map_err(convert_error)
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
async fn open_config_file(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
  log::debug!("open_config_file");
  controller::open_config_file(&app, &state.session).map_err(convert_error)
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

/// Starts the GUI. Called by `main.rs`, which adds the Windows subsystem attribute.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  env_logger::init();

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4)
    .enable_all()
    .build()
    .expect("the tokio runtime is built");
  tauri::async_runtime::set(runtime.handle().clone());

  // A config that cannot be read is not fatal: the session opens on defaults, the
  // status bar says so, and nothing is written until the user saves the Settings tab.
  // A history that cannot be opened is, because there is nothing to show without it
  // and nowhere to put what arrives.
  let toaster = Arc::new(notification::TauriToaster::new());
  let session = match Session::open(None, SessionApp::Gui, toaster.clone()) {
    Ok(session) => session,
    Err(error) => panic!("the shared session could not be opened: {error}"),
  };
  if let Some(error) = session.load_error() {
    log::error!("the config could not be read: {error}");
  }

  // Before the builder, because the builder creates the window and a window that is moved
  // after it exists is drawn twice. See `window::place`.
  let mut context = tauri::generate_context!();
  window::place(&mut context, &session);

  tauri::Builder::default()
    .manage(AppState { session, toaster })
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_notification::init())
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
      open_config_file,
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
