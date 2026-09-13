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

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

mod config;
mod constants;
mod controller;
mod mqtt;
mod notification;
mod protocol;
mod update;
mod window;

use protocol::{AppState, MessageRow, PublishOptions, Status, TopicNode, UpdateCheckResult};

#[tauri::command]
async fn clear_topic(topic: String, state: tauri::State<'_, AppState>) -> Result<u64, String> {
  log::debug!("clear_topic({topic})");
  controller::clear_topic(&state.store, &topic).map_err(convert_error)
}

#[tauri::command]
async fn connect(app: tauri::AppHandle) -> Result<Status, String> {
  log::debug!("connect");
  controller::connect(&app).await.map_err(convert_error)
}

fn convert_error(error: anyhow::Error) -> String {
  error.to_string()
}

#[tauri::command]
async fn disconnect(app: tauri::AppHandle) -> Result<(), String> {
  log::debug!("disconnect");
  controller::disconnect(&app).await.map_err(convert_error)
}

#[tauri::command]
async fn get_about() -> Result<protocol::About, String> {
  log::debug!("get_about");
  controller::get_about().map_err(convert_error)
}

#[tauri::command]
async fn get_broker_init() -> Result<String, String> {
  // The setup string carries the broker password, so it is never logged.
  log::debug!("get_broker_init");
  controller::get_broker_init().map_err(convert_error)
}

#[tauri::command]
async fn get_config() -> Result<hiveme_core::Config, String> {
  log::debug!("get_config");
  controller::get_config().map_err(convert_error)
}

#[tauri::command]
async fn get_messages(
  topic: String,
  before: Option<i64>,
  limit: Option<u32>,
  state: tauri::State<'_, AppState>,
) -> Result<Vec<MessageRow>, String> {
  log::debug!("get_messages({topic}, {before:?}, {limit:?})");
  controller::get_messages(&state.store, &topic, before, limit.unwrap_or(0)).map_err(convert_error)
}

#[tauri::command]
async fn get_status(app: tauri::AppHandle) -> Result<Status, String> {
  log::debug!("get_status");
  controller::get_status(&app).map_err(convert_error)
}

#[tauri::command]
async fn get_update_result(app: tauri::AppHandle) -> Result<Option<UpdateCheckResult>, String> {
  log::debug!("get_update_result");
  Ok(controller::get_update_result(&app))
}

#[tauri::command]
async fn list_topics(state: tauri::State<'_, AppState>) -> Result<Vec<TopicNode>, String> {
  log::debug!("list_topics");
  controller::list_topics(&state.store).map_err(convert_error)
}

#[tauri::command]
async fn mark_read(topic: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
  log::debug!("mark_read({topic})");
  controller::mark_read(&state.store, &topic).map_err(convert_error)
}

#[tauri::command]
async fn open_config_file(app: tauri::AppHandle) -> Result<(), String> {
  log::debug!("open_config_file");
  controller::open_config_file(&app).map_err(convert_error)
}

#[tauri::command]
async fn publish(
  app: tauri::AppHandle,
  topic: String,
  body: String,
  options: Option<PublishOptions>,
) -> Result<MessageRow, String> {
  log::debug!("publish({topic}, {} byte(s))", body.len());
  controller::publish(&app, &topic, &body, options.unwrap_or_default())
    .await
    .map_err(convert_error)
}

/// Starts the GUI. Called by `main.rs`, which adds the Windows subsystem attribute.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  env_logger::init();

  if let Err(error) = config::init() {
    // A config that cannot be read is not fatal: the GUI opens on defaults, says so in
    // the status bar, and writes nothing until the user saves the Settings tab.
    log::error!("the config could not be read: {error}");
  }

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4)
    .enable_all()
    .build()
    .expect("the tokio runtime is built");
  tauri::async_runtime::set(runtime.handle().clone());

  let store = match hiveme_core::Store::open(&config::database_path()) {
    Ok(store) => store,
    Err(error) => {
      // Without a history there is nothing to show and nowhere to put what arrives, so
      // this is the one startup failure the GUI cannot work around.
      panic!(
        "the message history at {} could not be opened: {error}",
        config::database_path().display()
      );
    }
  };
  let store = Arc::new(store);
  let notifier = Arc::new(notification::Notifier::new(&config::get_config()));
  let received = Arc::new(AtomicU64::new(0));
  let mqtt = Arc::new(mqtt::Mqtt::new(store.clone(), notifier.clone(), received.clone()));

  tauri::Builder::default()
    .manage(AppState {
      store,
      mqtt,
      notifier,
      update: Arc::new(Mutex::new(None)),
    })
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
    .build(tauri::generate_context!())
    .expect("the application builds")
    .run(window::on_run_event);
}

#[tauri::command]
async fn set_config(app: tauri::AppHandle, config: hiveme_core::Config) -> Result<hiveme_core::Config, String> {
  // The config carries the broker password, so the value itself is never logged.
  log::debug!("set_config");
  controller::set_config(&app, config).await.map_err(convert_error)
}

#[tauri::command]
async fn set_notifications_paused(app: tauri::AppHandle, paused: bool) -> Result<Status, String> {
  log::debug!("set_notifications_paused({paused})");
  controller::set_notifications_paused(&app, paused).map_err(convert_error)
}

#[tauri::command]
async fn skip_version(version: String) -> Result<(), String> {
  log::debug!("skip_version({version})");
  controller::skip_version(version).map_err(convert_error)
}
