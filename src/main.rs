// SPDX-License-Identifier: MIT

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

mod app;
mod clipboard;
mod config;
mod history;
mod i18n;

use config::{DEFAULT_LOG_LEVEL, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH};

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_LOG_LEVEL)),
        )
        .init();

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    let settings = cosmic::app::Settings::default().size_limits(
        cosmic::iced::Limits::NONE
            .min_width(WINDOW_MIN_WIDTH)
            .min_height(WINDOW_MIN_HEIGHT),
    );

    cosmic::app::run::<app::AppModel>(settings, ())
}
