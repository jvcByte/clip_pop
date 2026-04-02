// SPDX-License-Identifier: MIT

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

mod app;
mod clipboard;
mod clipboard_watcher;
mod config;
mod db;
mod i18n;

use config::{DEFAULT_LOG_LEVEL, WINDOW_HEIGHT, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH, WINDOW_WIDTH};

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_LOG_LEVEL)),
        )
        .init();

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(WINDOW_MIN_WIDTH)
                .min_height(WINDOW_MIN_HEIGHT),
        )
        .resizable(Some(1.0));

    // run_single_instance ensures only one process runs at a time.
    // When Super+V triggers a second launch, it activates the existing
    // instance via D-Bus instead of opening a new window.
    cosmic::app::run_single_instance::<app::AppModel>(settings, app::Flags)
}
