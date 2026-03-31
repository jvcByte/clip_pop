// SPDX-License-Identifier: MIT

//! Application model, view, and update logic.

use crate::clipboard;
use crate::config::{self, Config, DATA_DIR_FALLBACK, DATA_DIR_NAME, HISTORY_FILE_NAME};
use crate::fl;
use crate::history::HistoryStore;
use cosmic::app::context_drawer;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::{self, about::About, menu};
use cosmic::prelude::*;
use std::collections::HashMap;
use tracing::error;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

pub struct AppModel {
    core: cosmic::Core,
    context_page: ContextPage,
    about: About,
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    config: Config,
    history: HistoryStore,
    /// Current search query entered by the user.
    search_query: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// A new clipboard value was detected by the background watcher.
    ClipboardChanged(Option<String>),
    /// User clicked a history item to copy it.
    CopyItem(usize),
    /// User clicked the delete button on a history item.
    DeleteItem(usize),
    /// User pressed "Clear All".
    ClearAll,
    /// Search bar input changed.
    SearchChanged(String),
    /// Clear the search bar.
    SearchClear,
    /// Open a URL (used by the About page).
    LaunchUrl(String),
    /// Toggle a context drawer page.
    ToggleContextPage(ContextPage),
    /// Config was updated externally (dbus-config watch).
    UpdateConfig(Config),
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = config::APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let config = config::load(Self::APP_ID);

        let history_path = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from(DATA_DIR_FALLBACK))
            .join(DATA_DIR_NAME)
            .join(HISTORY_FILE_NAME);

        let history = HistoryStore::load(history_path, config.max_history);

        let about = About::default()
            .name(fl!("app-title"))
            .icon(widget::icon::from_svg_bytes(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .links([(fl!("repository"), REPOSITORY)])
            .license(env!("CARGO_PKG_LICENSE"));

        let mut app = AppModel {
            core,
            context_page: ContextPage::default(),
            about,
            key_binds: HashMap::new(),
            config,
            history,
            search_query: String::new(),
        };

        let command = app.update_title();
        (app, command)
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let menu_bar = menu::bar(vec![menu::Tree::with_children(
            menu::root(fl!("view")).apply(Element::from),
            menu::items(
                &self.key_binds,
                vec![menu::Item::Button(fl!("about"), None, MenuAction::About)],
            ),
        )]);
        vec![menu_bar.into()]
    }

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }
        Some(match self.context_page {
            ContextPage::About => context_drawer::about(
                &self.about,
                |url| Message::LaunchUrl(url.to_string()),
                Message::ToggleContextPage(ContextPage::About),
            ),
        })
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let spacing = cosmic::theme::spacing();

        // ── Search bar ────────────────────────────────────────────────────────
        let search = widget::search_input(fl!("search-placeholder"), &self.search_query)
            .on_input(Message::SearchChanged)
            .on_clear(Message::SearchClear)
            .width(Length::Fill);

        // ── Toolbar: search + clear all ───────────────────────────────────────
        let toolbar = widget::row::with_capacity(2)
            .push(search)
            .push(
                widget::button::destructive(fl!("clear-all"))
                    .on_press(Message::ClearAll),
            )
            .spacing(spacing.space_s)
            .padding([spacing.space_xs, spacing.space_s]);

        // ── History list ──────────────────────────────────────────────────────
        let query = self.search_query.to_lowercase();
        let filtered: Vec<(usize, _)> = self
            .history
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| query.is_empty() || e.content.to_lowercase().contains(&query))
            .collect();

        let content: Element<_> = if filtered.is_empty() {
            widget::container(
                widget::column::with_capacity(2)
                    .push(
                        widget::icon(widget::icon::from_name("edit-paste-symbolic").handle())
                            .size(48),
                    )
                    .push(widget::text::body(if self.search_query.is_empty() {
                        fl!("empty-history")
                    } else {
                        fl!("no-results")
                    }))
                    .spacing(spacing.space_m)
                    .align_x(cosmic::iced::Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
        } else {
            let list = filtered
                .iter()
                .fold(widget::list_column(), |col, (i, entry)| {
                    // First line: content preview
                    let preview = entry.preview(self.config.preview_chars);
                    // Second line: timestamp right-aligned
                    let time = entry.relative_time_i18n();

                    let meta_row = widget::row::with_capacity(2)
                        .push(
                            widget::text::caption(time)
                                .width(Length::Fill),
                        )
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("edit-delete-symbolic"),
                            )
                            .on_press(Message::DeleteItem(*i)),
                        )
                        .align_y(cosmic::iced::Alignment::Center);

                    let item = widget::column::with_capacity(2)
                        .push(widget::text::body(preview).width(Length::Fill))
                        .push(meta_row)
                        .spacing(spacing.space_xxxs)
                        .width(Length::Fill)
                        .padding([spacing.space_xxs, spacing.space_xs]);

                    col.add(
                        widget::button::custom(item)
                            .on_press(Message::CopyItem(*i))
                            .width(Length::Fill),
                    )
                });

            widget::scrollable(list)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        widget::column::with_capacity(2)
            .push(toolbar)
            .push(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let config_watch = self
            .core()
            .watch_config::<Config>(Self::APP_ID)
            .map(|update| Message::UpdateConfig(update.config));

        let clipboard_watch = clipboard::watch(self.config.poll_interval_ms)
            .map(Message::ClipboardChanged);

        Subscription::batch([config_watch, clipboard_watch])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::ClipboardChanged(Some(text)) => {
                self.history.push(text);
            }

            Message::ClipboardChanged(None) => {}

            Message::CopyItem(i) => {
                if let Some(entry) = self.history.promote(i) {
                    if let Err(e) = clipboard::set_text(&entry.content.clone()) {
                        error!("failed to set clipboard: {e}");
                    }
                }
            }

            Message::DeleteItem(i) => {
                self.history.remove(i);
            }

            Message::ClearAll => {
                self.history.clear();
            }

            Message::SearchChanged(query) => {
                self.search_query = query;
            }

            Message::SearchClear => {
                self.search_query.clear();
            }

            Message::UpdateConfig(config) => {
                let config = config.validated();
                self.history.set_max(config.max_history);
                self.config = config;
            }

            Message::ToggleContextPage(page) => {
                if self.context_page == page {
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    self.context_page = page;
                    self.core.window.show_context = true;
                }
            }

            Message::LaunchUrl(url) => {
                if let Err(e) = open::that_detached(&url) {
                    error!("failed to open url {url:?}: {e}");
                }
            }
        }

        Task::none()
    }
}

impl AppModel {
    pub fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let title = fl!("app-title");
        if let Some(id) = self.core.main_window_id() {
            self.set_window_title(title, id)
        } else {
            Task::none()
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ContextPage {
    #[default]
    About,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
        }
    }
}
