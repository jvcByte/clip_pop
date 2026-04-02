// SPDX-License-Identifier: MIT

//! Application model, view, and update logic.

use crate::clipboard::{self, ClipboardEvent, fx_hash};
use crate::config::{self, Config, DATA_DIR_NAME, DB_FILE_NAME, PRIVATE_MODE, autostart_path};
use crate::db::{Db, fuzzy_search};
use crate::fl;
use cosmic::app::context_drawer;
use cosmic::app::CosmicFlags;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::{self, about::About, menu};
use cosmic::prelude::*;
use futures::executor::block_on;
use std::collections::HashMap;
use std::sync::atomic;
use tracing::error;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

/// Flags type required by run_single_instance.
#[derive(Debug, Clone, Default)]
pub struct Flags;

impl CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;
}

pub struct AppModel {
    core: cosmic::Core,
    context_page: ContextPage,
    about: About,
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    config: Config,
    config_ctx: cosmic::cosmic_config::Config,
    db: Db,
    search_query: String,
    /// ID of the entry currently in the system clipboard.
    active_id: Option<i64>,
    /// Show confirm-clear dialog.
    show_confirm_clear: bool,
    /// Hash of content we just set — suppresses the next matching event.
    suppress_next: Option<u64>,
    /// Whether the clipboard protocol is available.
    clipboard_available: bool,
    /// Tracks minimized state for Super+V toggle.
    window_minimized: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ClipboardEvent(ClipboardEvent),
    SelectItem(usize),
    PinItem(usize),
    DeleteItem(usize),
    RequestClearAll,
    ConfirmClearAll,
    CancelClearAll,
    TogglePrivateMode,
    ToggleLaunchOnLogin,
    SearchChanged(String),
    SearchClear,
    LaunchUrl(String),
    ToggleContextPage(ContextPage),
    UpdateConfig(Config),
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
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
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {        let (config_ctx, config) = config::load(Self::APP_ID);

        // Sync atomic private mode flag
        PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);

        // Sync launch_on_login with actual autostart file state
        let autostart_exists = autostart_path().exists();
        let mut config = config;
        if config.launch_on_login != autostart_exists {
            config.launch_on_login = autostart_exists;
        }

        let db_path = dirs::data_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(DATA_DIR_NAME)
            .join(DB_FILE_NAME);

        let db = block_on(Db::open(&db_path, config.max_history))
            .unwrap_or_else(|e| {
                error!("failed to open db: {e}");
                panic!("cannot open clipboard database");
            });

        // Expire old entries on startup
        let mut db = db;
        if let Some(days) = config.entry_lifetime_days {
            if let Err(e) = block_on(db.expire_older_than(days)) {
                error!("failed to expire old entries: {e}");
            }
        }

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
            config_ctx,
            db,
            search_query: String::new(),
            active_id: None,
            show_confirm_clear: false,
            suppress_next: None,
            clipboard_available: true,
            window_minimized: false,
        };

        let command = app.update_title();
        (app, command)
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        let menu_bar = menu::bar(vec![menu::Tree::with_children(
            menu::root(fl!("view")).apply(Element::from),
            menu::items(
                &self.key_binds,
                vec![
                    menu::Item::Button(fl!("settings"), None, MenuAction::Settings),
                    menu::Item::Button(fl!("about"), None, MenuAction::About),
                ],
            ),
        )]);
        vec![menu_bar.into()]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        vec![]
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
            ContextPage::Settings => context_drawer::context_drawer(
                self.settings_view(),
                Message::ToggleContextPage(ContextPage::Settings),
            )
            .title(fl!("settings")),
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

        // ── Clipboard unavailable banner ──────────────────────────────────────
        if !self.clipboard_available {
            return widget::container(
                widget::column::with_capacity(2)
                    .push(
                        widget::icon(widget::icon::from_name("dialog-error-symbolic").handle())
                            .size(48),
                    )
                    .push(widget::text::body(fl!("clipboard-unavailable")))
                    .spacing(spacing.space_m)
                    .align_x(cosmic::iced::Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into();
        }

        // ── Search + toolbar ──────────────────────────────────────────────────
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

        // ── Filtered entries ──────────────────────────────────────────────────
        let all_entries = self.db.entries();
        let filtered = fuzzy_search(all_entries, &self.search_query);

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
            let pinned: Vec<_> = filtered.iter().filter(|(_, e)| e.pinned).collect();
            let unpinned: Vec<_> = filtered.iter().filter(|(_, e)| !e.pinned).collect();

            let mut col = widget::list_column().spacing(0);

            if !pinned.is_empty() {
                col = col.add(section_label(fl!("pinned"), cosmic::iced::Padding::ZERO));
                for (i, entry) in &pinned {
                    col = col.add(self.history_row(*i, entry));
                }
            }

            if !unpinned.is_empty() {
                col = col.add(section_label(fl!("history"), cosmic::iced::Padding::ZERO));
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

    /// Called when a second instance tries to launch (Super+V while already running).
    /// Toggles window — focus if minimized/hidden, minimize if visible.
    fn dbus_activation(
        &mut self,
        _msg: cosmic::dbus_activation::Message,
    ) -> Task<cosmic::Action<Self::Message>> {
        if let Some(id) = self.core.main_window_id() {
            if self.window_minimized {
                self.window_minimized = false;
                cosmic::iced::window::gain_focus::<cosmic::Action<Message>>(id)
            } else {
                self.window_minimized = true;
                cosmic::iced::window::minimize::<cosmic::Action<Message>>(id, true)
            }
        } else {
            Task::none()
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {        let config_watch = self
            .core()
            .watch_config::<Config>(Self::APP_ID)
            .map(|update| Message::UpdateConfig(update.config));

        if !self.clipboard_available {
            return config_watch;
        }

        let clipboard_watch = clipboard::watch().map(Message::ClipboardEvent);
        Subscription::batch([config_watch, clipboard_watch])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::ClipboardEvent(event) => match event {
                ClipboardEvent::Text(text) => {
                    let hash = fx_hash(text.as_bytes());
                    if self.suppress_next == Some(hash) {
                        self.suppress_next = None;
                        return Task::none();
                    }
                    self.suppress_next = None;
                    match block_on(self.db.insert("text/plain;charset=utf-8", text.into_bytes())) {
                        Ok(idx) => {
                            self.active_id = self.db.get(idx).map(|e| e.id);
                        }
                        Err(e) => error!("db insert text: {e}"),
                    }
                }

                ClipboardEvent::Image { data, mime_type } => {
                    let hash = fx_hash(&data);
                    if self.suppress_next == Some(hash) {
                        self.suppress_next = None;
                        return Task::none();
                    }
                    self.suppress_next = None;
                    match block_on(self.db.insert(&mime_type, data)) {
                        Ok(idx) => {
                            self.active_id = self.db.get(idx).map(|e| e.id);
                        }
                        Err(e) => error!("db insert image: {e}"),
                    }
                }

                ClipboardEvent::Cleared => {
                    self.suppress_next = None;
                    self.active_id = None;
                }

                ClipboardEvent::Unavailable => {
                    self.clipboard_available = false;
                }
            },

            Message::SelectItem(i) => {
                let entry = if self.config.move_to_top_on_select {
                    if let Err(e) = block_on(self.db.promote(i)) {
                        error!("promote: {e}");
                    }
                    self.db.get(0)
                } else {
                    self.db.get(i)
                };

                if let Some(entry) = entry {
                    let data = entry.content.clone();
                    let mime = entry.mime_type.clone();
                    let id = entry.id;
                    let hash = fx_hash(&data);
                    self.suppress_next = Some(hash);

                    let result = if mime.starts_with("image/") {
                        clipboard::set_image(&data, &mime)
                    } else {
                        match String::from_utf8(data) {
                            Ok(text) => clipboard::set_text(&text),
                            Err(e) => Err(e.to_string()),
                        }
                    };

                    match result {
                        Ok(()) => self.active_id = Some(id),
                        Err(e) => {
                            error!("set clipboard: {e}");
                            self.suppress_next = None;
                        }
                    }
                }
            }

            Message::PinItem(i) => {
                let active_id = self.active_id;
                if let Err(e) = block_on(self.db.toggle_pin(i)) {
                    error!("toggle pin: {e}");
                }
                // Recompute active_id position after reorder (id is stable)
                self.active_id = active_id;
            }

            Message::DeleteItem(i) => {
                if self.db.get(i).map(|e| e.id) == self.active_id {
                    self.active_id = None;
                }
                if let Err(e) = block_on(self.db.remove(i)) {
                    error!("remove: {e}");
                }
            }

            Message::RequestClearAll => {
                self.show_confirm_clear = true;
            }

            Message::ConfirmClearAll => {
                self.show_confirm_clear = false;
                if let Err(e) = block_on(self.db.clear_unpinned()) {
                    error!("clear: {e}");
                }
                self.active_id = None;
            }

            Message::CancelClearAll => {
                self.show_confirm_clear = false;
            }

            Message::TogglePrivateMode => {
                self.config.private_mode = !self.config.private_mode;
                PRIVATE_MODE.store(self.config.private_mode, atomic::Ordering::Relaxed);
                if let Err(e) = self.config.set_private_mode(&self.config_ctx, self.config.private_mode) {
                    error!("failed to persist private_mode: {e}");
                }
            }

            Message::ToggleLaunchOnLogin => {
                self.config.launch_on_login = !self.config.launch_on_login;
                let path = autostart_path();
                if self.config.launch_on_login {
                    // Write autostart desktop file
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let content = format!(
                        "[Desktop Entry]\nName=Clip Pop\nType=Application\nExec=clip_pop\nHidden=false\nX-GNOME-Autostart-enabled=true\n"
                    );
                    if let Err(e) = std::fs::write(&path, content) {
                        error!("failed to write autostart file: {e}");
                        self.config.launch_on_login = false;
                    }
                } else {
                    let _ = std::fs::remove_file(&path);
                }
                if let Err(e) = self.config.set_launch_on_login(&self.config_ctx, self.config.launch_on_login) {
                    error!("failed to persist launch_on_login: {e}");
                }
            }

            Message::SearchChanged(query) => {
                self.search_query = query;
            }

            Message::SearchClear => {
                self.search_query.clear();
            }

            Message::UpdateConfig(config) => {
                let config = config.validated();
                PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);
                self.db.set_max(config.max_history);
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
                    error!("open url {url:?}: {e}");
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

    fn settings_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let section = widget::settings::section()
            .add(
                widget::settings::item::builder(fl!("private-mode"))
                    .description(fl!("private-mode-description"))
                    .toggler(self.config.private_mode, |_| Message::TogglePrivateMode),
            )
            .add(
                widget::settings::item::builder(fl!("launch-on-login"))
                    .description(fl!("launch-on-login-description"))
                    .toggler(self.config.launch_on_login, |_| Message::ToggleLaunchOnLogin),
            );
        widget::column::with_capacity(1)
            .push(section)
            .spacing(spacing.space_m)
            .padding(spacing.space_m)
            .into()
    }

    fn history_row<'a>(
        &'a self,
        index: usize,
        entry: &'a crate::db::ClipEntry,
    ) -> Element<'a, Message> {
        let spacing = cosmic::theme::spacing();
        let is_active = self.active_id == Some(entry.id);

        let indicator: Element<_> = if is_active {
            widget::icon(widget::icon::from_name("media-record-symbolic").handle())
                .size(8)
                .into()
        } else {
            widget::container(widget::Space::new()).width(8).into()
        };

        let content_area: Element<_> = if entry.is_image() {
            // Decode image bytes for thumbnail
            let handle = widget::image::Handle::from_bytes(entry.content.clone());
            widget::column::with_capacity(2)
                .push(
                    widget::image(handle)
                        .width(Length::Fixed(120.0))
                        .height(Length::Fixed(68.0))
                        .content_fit(cosmic::iced::ContentFit::Cover),
                )
                .push(widget::text::caption(entry.relative_time_i18n()))
                .spacing(spacing.space_xxxs)
                .width(Length::Fill)
                .into()
        } else {
            widget::column::with_capacity(2)
                .push(
                    widget::text::body(entry.preview(self.config.preview_chars))
                        .width(Length::Fill)
                        .wrapping(cosmic::iced::widget::text::Wrapping::WordOrGlyph),
                )
                .push(widget::text::caption(entry.relative_time_i18n()))
                .spacing(spacing.space_xxxs)
                .width(Length::Fill)
                .into()
        };

        let actions = widget::row::with_capacity(2)
            .push(
                widget::button::icon(widget::icon::from_name("pin-symbolic"))
                    .on_press(Message::PinItem(index))
                    .selected(entry.pinned),
            )
            .push(
                widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                    .on_press(Message::DeleteItem(index)),
            )
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

fn section_label<'a, M: 'a>(
    label: String,
    spacing: cosmic::iced::Padding,
) -> impl Into<Element<'a, M>> {
    let _ = spacing;
    let spacing = cosmic::theme::spacing();
    widget::text::caption(label)
        .width(Length::Fill)
        .apply(|t| widget::container(t).padding([spacing.space_xxs, spacing.space_s]))
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum ContextPage {
    #[default]
    About,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
    Settings,
}

impl menu::action::MenuAction for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ToggleContextPage(ContextPage::About),
            MenuAction::Settings => Message::ToggleContextPage(ContextPage::Settings),
        }
    }
}
