// SPDX-License-Identifier: MIT

use crate::config::Config;
use crate::fl;
use arboard::Clipboard;
use cosmic::app::context_drawer;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::{self, about::About, menu};
use cosmic::{iced_futures, prelude::*};
use futures_util::SinkExt;
use std::collections::HashMap;
use std::time::Duration;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const APP_ICON: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/icon.svg");

pub struct AppModel {
    core: cosmic::Core,
    context_page: ContextPage,
    about: About,
    key_binds: HashMap<menu::KeyBind, MenuAction>,
    config: Config,
    /// Clipboard history, newest first.
    history: Vec<String>,
    /// The last clipboard content we saw, to detect changes.
    last_clipboard: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    LaunchUrl(String),
    ToggleContextPage(ContextPage),
    UpdateConfig(Config),
    /// Fired on each clipboard poll tick.
    ClipboardTick(Option<String>),
    /// User clicked an item to copy it back.
    CopyItem(usize),
    /// Clear all history.
    ClearAll,
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.github.jvcByte.clip_pop";

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
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|ctx| Config::get_entry(&ctx).unwrap_or_default())
                .unwrap_or_default(),
            history: Vec::new(),
            last_clipboard: None,
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

        let header = widget::row::with_capacity(2)
            .push(widget::text::title3(fl!("clipboard-history")).width(Length::Fill))
            .push(
                widget::button::destructive(fl!("clear-all"))
                    .on_press(Message::ClearAll),
            )
            .padding(spacing.space_s)
            .spacing(spacing.space_s);

        let content: Element<_> = if self.history.is_empty() {
            widget::container(widget::text::body(fl!("empty-history")))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(cosmic::iced::alignment::Horizontal::Center)
                .align_y(cosmic::iced::alignment::Vertical::Center)
                .into()
        } else {
            let items = self.history.iter().enumerate().fold(
                widget::list_column(),
                |col, (i, entry)| {
                    let preview = if entry.len() > 80 {
                        format!("{}…", &entry[..80])
                    } else {
                        entry.clone()
                    };
                    col.add(
                        widget::button::custom(
                            widget::text::body(preview).width(Length::Fill),
                        )
                        .on_press(Message::CopyItem(i))
                        .width(Length::Fill),
                    )
                },
            );
            widget::scrollable(items)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        widget::column::with_capacity(2)
            .push(header)
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

        // Poll clipboard every 500ms in a background stream.
        let clipboard_poll = Subscription::run(|| {
            iced_futures::stream::channel(1, |mut tx| async move {
                let mut clipboard = Clipboard::new().ok();
                let mut last: Option<String> = None;
                let mut interval = tokio::time::interval(Duration::from_millis(500));
                loop {
                    interval.tick().await;
                    let current = clipboard
                        .as_mut()
                        .and_then(|c| c.get_text().ok());
                    if current != last {
                        last = current.clone();
                        let _ = tx.send(Message::ClipboardTick(current)).await;
                    }
                }
            })
        });

        Subscription::batch([config_watch, clipboard_poll])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::ClipboardTick(Some(text)) => {
                if self.last_clipboard.as_deref() != Some(text.as_str()) {
                    self.last_clipboard = Some(text.clone());
                    // Avoid duplicates at the top
                    self.history.retain(|e| e != &text);
                    self.history.insert(0, text);
                    // Trim to max history
                    self.history.truncate(self.config.max_history);
                }
            }

            Message::ClipboardTick(None) => {}

            Message::CopyItem(i) => {
                if let Some(text) = self.history.get(i).cloned() {
                    if let Ok(mut cb) = Clipboard::new() {
                        let _ = cb.set_text(&text);
                        // Move to top
                        self.history.remove(i);
                        self.history.insert(0, text);
                    }
                }
            }

            Message::ClearAll => {
                self.history.clear();
                self.last_clipboard = None;
            }

            Message::UpdateConfig(config) => {
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
                let _ = open::that_detached(&url);
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
