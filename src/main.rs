// SPDX-License-Identifier: MIT

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

mod app;
mod clipboard;
mod config;
mod history;
mod i18n;

fn main() -> cosmic::iced::Result {
    // Structured logging — respects RUST_LOG env var, defaults to info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    let settings = cosmic::app::Settings::default().size_limits(
        cosmic::iced::Limits::NONE
            .min_width(360.0)
            .min_height(480.0),
    );

    cosmic::app::run::<app::AppModel>(settings, ())
}
