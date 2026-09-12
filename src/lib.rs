//! Laboratory acquisition and control application building blocks.
//!
//! Start with `application_runtime` for composition, `acquisition` for transport routing,
//! `process_control` for pure controller algorithms, and `output_control` for actuator ownership.
//! The GUI and optional standalone emulator share instrument/protocol types but own separate runtimes.

mod acquisition;
pub mod app;
mod app_log;
pub mod application_definition;
mod application_event;
pub mod application_paths;
mod application_runtime;
mod components;
pub mod connection;
pub mod control_panel;
mod data;
pub mod device_emulator_handle;
pub mod instrument;
mod lua_api;
pub mod lua_application_definition;
mod lua_application_script;
mod lua_execution;
pub mod lua_runtime;
mod lua_virtual_instrument_model;
mod lua_worker;
mod output_control;
pub mod presentation;
pub mod process_control;
mod process_recorder;
pub mod protocol;
mod scenario;
pub mod serial_connection;
pub mod signal_processing;
mod user_command;
mod utils;
mod worker;
