// SPDX-License-Identifier: MIT

//! Application model, view, and update logic.

use crate::clipboard::{self, ClipboardEvent};
use crate::config::{self, Config, DATA_DIR_FALLBACK, DATA_DIR_NAME, HISTORY_FILE_NAME};
use crate::fl;
use crate::history::{EntryKind, HistoryStore};
use cosmic::app::context_drawer;use cosmic::iced::alignment::{Horizontal, Vertical};
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
    search_query: String,
    /// Index of the item currently in the system clipboard.
    active_index: Option<usize>,
    /// Show confirm-clear dialog.
    show_confirm_clear: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ClipboardChanged(ClipboardEvent),
    SelectItem(usize),
    PinItem(usize),
    DeleteItem(usize),
    RequestClearAll,
    ConfirmClearAll,
    CancelClearAll,
    TogglePrivateMode,
    SearchChanged(String),
    SearchClear,
    LaunchUrl(String),
    ToggleContextPage(ContextPage),
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
            active_index: None,
            show_confirm_clear: false,
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

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        // Private mode toggle in the header
        let icon_name = if self.config.private_mode {
            "security-high-symbolic"
        } else {
            "security-low-symbolic"
        };
        vec![
            widget::tooltip(
                widget::button::icon(widget::icon::from_name(icon_name))
                    .on_press(Message::TogglePrivateMode),
                widget::text(fl!("private-mode")),
                widget::tooltip::Position::Bottom,
            )
            .into(),
        ]
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

    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        if !self.show_confirm_clear {
            return None;
        }

        let dialog = widget::dialog()
            .title(fl!("confirm-clear-title"))
            .body(fl!("confirm-clear-body"))
            .primary_action(
                widget::button::destructive(fl!("confirm-clear-confirm"))
                    .on_press(Message::ConfirmClearAll),
            )
            .secondary_action(
                widget::button::text(fl!("confirm-clear-cancel"))
                    .on_press(Message::CancelClearAll),
            );

        Some(dialog.into())
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let spacing = cosmic::theme::spacing();

        // ── Search bar ────────────────────────────────────────────────────────
        let search = widget::search_input(fl!("search-placeholder"), &self.search_query)
            .on_input(Message::SearchChanged)
            .on_clear(Message::SearchClear)
            .width(Length::Fill);

        let toolbar = widget::row::with_capacity(2)
            .push(search)
            .push(
                widget::button::destructive(fl!("clear-all"))
                    .on_press(Message::RequestClearAll),
            )
            .spacing(spacing.space_s)
            .padding([spacing.space_xs, spacing.space_s]);

        // ── Filter entries ────────────────────────────────────────────────────
        let query = self.search_query.to_lowercase();
        let filtered: Vec<(usize, _)> = self
            .history
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                query.is_empty() || e.kind.dedup_key().to_lowercase().contains(&query)
            })
            .collect();

        // ── Empty state ───────────────────────────────────────────────────────
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
            // Split into pinned / unpinned for rendering
            let pinned: Vec<_> = filtered.iter().filter(|(_, e)| e.pinned).collect();
            let unpinned: Vec<_> = filtered.iter().filter(|(_, e)| !e.pinned).collect();

            let mut col = widget::list_column().spacing(0);

            // ── Pinned section ────────────────────────────────────────────────
            if !pinned.is_empty() {
                col = col.add(
                    widget::text::caption(fl!("pinned"))
                        .width(Length::Fill)
                        .apply(|t| {
                            widget::container(t)
                                .padding([spacing.space_xxs, spacing.space_s])
                        }),
                );
                for (i, entry) in &pinned {
                    col = col.add(self.history_row(*i, entry));
                }
            }

            // ── History section ───────────────────────────────────────────────
            if !unpinned.is_empty() {
                col = col.add(
                    widget::text::caption(fl!("history"))
                        .width(Length::Fill)
                        .apply(|t| {
                            widget::container(t)
                                .padding([spacing.space_xxs, spacing.space_s])
                        }),
                );
                for (i, entry) in &unpinned {
                    col = col.add(self.history_row(*i, entry));
                }
            }

            widget::scrollable(col)
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

        // Don't poll in private mode
        if self.config.private_mode {
            return config_watch;
        }

        let clipboard_watch =
            clipboard::watch(self.config.poll_interval_ms).map(Message::ClipboardChanged);
        Subscription::batch([config_watch, clipboard_watch])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::ClipboardChanged(event) => {
                match event {
                    ClipboardEvent::Text(text) => {
                        self.history.push_text(text.clone());
                        self.active_index =
                            self.history.entries().iter().position(|e| {
                                matches!(&e.kind, EntryKind::Text { content } if content == &text)
                            });
                    }
                    ClipboardEvent::Image { rgba, width, height } => {
                        if let Some(_path) = self.history.push_image(&rgba, width, height) {
                            self.active_index = Some(
                                self.history
                                    .entries()
                                    .iter()
                                    .position(|e| e.kind.is_image())
                                    .unwrap_or(0),
                            );
                        }
                    }
                    ClipboardEvent::Cleared => {
                        self.active_index = None;
                    }
                }
            }

            Message::SelectItem(i) => {
                let entry = if self.config.move_to_top_on_select {
                    self.history.promote(i)
                } else {
                    self.history.entries().get(i)
                };
                if let Some(entry) = entry {
                    match &entry.kind {
                        EntryKind::Text { content } => {
                            let content = content.clone();
                            if let Err(e) = clipboard::set_text(&content) {
                                error!("failed to set clipboard text: {e}");
                            } else {
                                self.active_index = self.history.entries().iter().position(|e| {
                                    matches!(&e.kind, EntryKind::Text { content: c } if c == &content)
                                });
                            }
                        }
                        EntryKind::Image { path, width, height } => {
                            let (path, w, h) = (path.clone(), *width, *height);
                            // Decode PNG back to raw RGBA pixels
                            match image::open(&path) {
                                Ok(img) => {
                                    let rgba = img.to_rgba8().into_raw();
                                    if let Err(e) = clipboard::set_image(&rgba, w, h) {
                                        error!("failed to set clipboard image: {e}");
                                    } else {
                                        self.active_index = Some(i);
                                    }
                                }
                                Err(e) => error!("failed to decode image for clipboard: {e}"),
                            }
                        }
                    }
                }
            }

            Message::PinItem(i) => {
                self.history.toggle_pin(i);
                // Recompute active index after reorder
                let active_key = self
                    .active_index
                    .and_then(|idx| self.history.entries().get(idx))
                    .map(|e| e.kind.dedup_key());
                if let Some(key) = active_key {
                    self.active_index = self
                        .history
                        .entries()
                        .iter()
                        .position(|e| e.kind.dedup_key() == key);
                }
            }

            Message::DeleteItem(i) => {
                if self.active_index == Some(i) {
                    self.active_index = None;
                }
                self.history.remove(i);
            }

            Message::RequestClearAll => {
                self.show_confirm_clear = true;
            }

            Message::ConfirmClearAll => {
                self.show_confirm_clear = false;
                self.history.clear_unpinned();
                self.active_index = None;
            }

            Message::CancelClearAll => {
                self.show_confirm_clear = false;
            }

            Message::TogglePrivateMode => {
                self.config.private_mode = !self.config.private_mode;
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

    /// Build a single history row element.
    fn history_row<'a>(
        &'a self,
        index: usize,
        entry: &'a crate::history::ClipEntry,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let is_active = self.active_index == Some(index);

        // ── Active indicator ──────────────────────────────────────────────────
        let indicator: Element<_> = if is_active {
            widget::icon(widget::icon::from_name("media-record-symbolic").handle())
                .size(8)
                .into()
        } else {
            widget::container(widget::Space::new())
                .width(8)
                .into()
        };

        // ── Content area ──────────────────────────────────────────────────────
        let content_area: Element<_> = match &entry.kind {
            EntryKind::Text { .. } => {
                widget::text::body(entry.preview(self.config.preview_chars))
                    .width(Length::Fill)
                    .into()
            }
            EntryKind::Image { path, .. } => {
                widget::column::with_capacity(2)
                    .push(
                        widget::image(widget::image::Handle::from_path(path))
                            .width(Length::Fixed(120.0))
                            .height(Length::Fixed(68.0))
                            .content_fit(cosmic::iced::ContentFit::Cover),
                    )
                    .push(widget::text::caption(entry.preview(self.config.preview_chars)))
                    .spacing(spacing.space_xxxs)
                    .width(Length::Fill)
                    .into()
            }
        };

        // ── Action buttons ────────────────────────────────────────────────────
        let pin_icon = "view-pin-symbolic";
        let pin_tooltip = if entry.pinned { fl!("action-unpin") } else { fl!("action-pin") };

        let actions = widget::row::with_capacity(2)
            .push(widget::tooltip(
                widget::button::icon(widget::icon::from_name(pin_icon))
                    .on_press(Message::PinItem(index)),
                widget::text(pin_tooltip),
                widget::tooltip::Position::Bottom,
            ))
            .push(widget::tooltip(
                widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                    .on_press(Message::DeleteItem(index)),
                widget::text(fl!("action-delete")),
                widget::tooltip::Position::Bottom,
            ))
            .spacing(0);

        let row = widget::row::with_capacity(3)
            .push(indicator)
            .push(content_area)
            .push(actions)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(spacing.space_xs)
            .padding([spacing.space_xs, spacing.space_s]);

        widget::button::custom(row)
            .on_press(Message::SelectItem(index))
            .width(Length::Fill)
            .class(cosmic::theme::Button::MenuItem)
            .into()
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
