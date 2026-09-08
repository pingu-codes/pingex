//! A command's Home is selected once, before its handler starts. A later
//! navigation or default change cannot redirect work already in flight.

use crate::{AppState, HomeContext};
use serde::Deserialize;
use specta::Type;
use std::sync::Arc;
use tauri::{
    ipc::{CommandArg, CommandItem, InvokeError},
    Manager,
};

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HomeRoute {
    pub home_key: String,
}

#[derive(Clone)]
pub(crate) struct HomeWindow {
    window: tauri::WebviewWindow,
    context: Arc<HomeContext>,
}

impl std::ops::Deref for HomeWindow {
    type Target = tauri::WebviewWindow;
    fn deref(&self) -> &Self::Target {
        &self.window
    }
}

impl specta::function::FunctionArg for HomeWindow {
    fn to_datatype(types: &mut specta::Types) -> Option<specta::datatype::DataType> {
        Some(<Option<HomeRoute>>::definition(types))
    }
}

impl<'de> CommandArg<'de, tauri::Wry> for HomeWindow {
    fn from_command(command: CommandItem<'de, tauri::Wry>) -> Result<Self, InvokeError> {
        let window = tauri::WebviewWindow::from_command(CommandItem {
            plugin: command.plugin,
            name: command.name,
            key: command.key,
            message: command.message,
            acl: command.acl,
        })?;
        let route = Option::<HomeRoute>::from_command(command)?;
        let context = window
            .state::<AppState>()
            .context_for_route(
                window.label(),
                route.as_ref().map(|route| route.home_key.as_str()),
            )
            .map_err(InvokeError::from)?;
        Ok(Self { window, context })
    }
}

pub(crate) trait ContextWindow {
    fn context(&self, state: &AppState) -> Arc<HomeContext>;
}

impl ContextWindow for HomeWindow {
    fn context(&self, _state: &AppState) -> Arc<HomeContext> {
        self.context.clone()
    }
}

impl ContextWindow for tauri::WebviewWindow {
    fn context(&self, state: &AppState) -> Arc<HomeContext> {
        state.ctx_for_label(self.label())
    }
}
