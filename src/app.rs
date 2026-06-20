use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::config::Config;
use crate::document::{
    loader,
    model::{plain_text, Block, Document},
    parser,
};
use crate::highlight::Highlighter;
use crate::input::{DocumentJump, InputAction, MermaidViewportCommand, NavigationState};
use crate::mermaid::{cache::MermaidCache, renderer::MermaidRenderer};
use crate::render::{self, settings::ViewerSettings};
use crate::sync::CursorSync;
use crate::watcher::FileWatcher;

#[derive(Debug, Clone)]
struct TocEntry {
    block_index: usize,
    level: u8,
    text: String,
}

#[derive(Debug, Default)]
struct SearchState {
    active: bool,
    query: String,
    matches: Vec<usize>,
    selected: usize,
    request_focus: bool,
}

impl SearchState {
    fn open(&mut self) {
        self.active = true;
        self.request_focus = true;
    }

    fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
        self.selected = 0;
    }

    fn update_matches(&mut self, document: &Document) {
        self.matches.clear();
        self.selected = 0;
        if self.query.is_empty() {
            return;
        }
        let q = self.query.to_lowercase();
        for (index, block) in document.blocks.iter().enumerate() {
            if block_contains_query(block, &q) {
                self.matches.push(index);
            }
        }
    }

    fn current_match(&self) -> Option<usize> {
        self.matches.get(self.selected).copied()
    }

    fn next(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
    }

    fn prev(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + self.matches.len() - 1) % self.matches.len();
        }
    }
}

fn block_contains_query(block: &Block, query: &str) -> bool {
    match block {
        Block::Heading { content, .. } | Block::Paragraph { content } => {
            plain_text(content).to_lowercase().contains(query)
        }
        Block::CodeBlock { code, .. } => code.to_lowercase().contains(query),
        Block::List { items, .. } => items.iter().any(|item| {
            item.blocks
                .iter()
                .any(|b| block_contains_query(b, query))
        }),
        Block::Quote { blocks } => blocks.iter().any(|b| block_contains_query(b, query)),
        _ => false,
    }
}

pub enum ImageEntry {
    Loading,
    Loaded(egui::ColorImage),
    Failed(String),
}

struct ImageJobResult {
    path: String,
    result: Result<egui::ColorImage, String>,
}

const RELOAD_DEBOUNCE: Duration = Duration::from_millis(200);
const CURSOR_SYNC_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Default)]
struct CommandPalette {
    open: bool,
    query: String,
    selected: usize,
    request_focus: bool,
    status: Option<String>,
}

impl CommandPalette {
    fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
        self.request_focus = true;
        self.status = None;
    }

    fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.selected = 0;
        self.status = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteCommand {
    Help,
    Settings,
    Reload,
    Top,
    Bottom,
    MermaidNext,
    MermaidPrevious,
    MermaidOpen,
    Fit,
    ZoomIn,
    ZoomOut,
    Toc,
    Search,
    ToggleTheme,
    Quit,
}

#[derive(Debug, Clone, Copy)]
struct PaletteEntry {
    name: &'static str,
    description: &'static str,
    command: PaletteCommand,
}

const PALETTE_ENTRIES: &[PaletteEntry] = &[
    PaletteEntry {
        name: "help",
        description: "show commands and keyboard shortcuts",
        command: PaletteCommand::Help,
    },
    PaletteEntry {
        name: "settings",
        description: "toggle viewer settings",
        command: PaletteCommand::Settings,
    },
    PaletteEntry {
        name: "reload",
        description: "reload the current Markdown document",
        command: PaletteCommand::Reload,
    },
    PaletteEntry {
        name: "top",
        description: "scroll to the start of the document",
        command: PaletteCommand::Top,
    },
    PaletteEntry {
        name: "bottom",
        description: "scroll to the end of the document",
        command: PaletteCommand::Bottom,
    },
    PaletteEntry {
        name: "mnext",
        description: "select the next Mermaid diagram",
        command: PaletteCommand::MermaidNext,
    },
    PaletteEntry {
        name: "mprev",
        description: "select the previous Mermaid diagram",
        command: PaletteCommand::MermaidPrevious,
    },
    PaletteEntry {
        name: "mopen",
        description: "open or enlarge the selected Mermaid diagram",
        command: PaletteCommand::MermaidOpen,
    },
    PaletteEntry {
        name: "fit",
        description: "fit the selected Mermaid diagram",
        command: PaletteCommand::Fit,
    },
    PaletteEntry {
        name: "zoom-in",
        description: "zoom into the selected Mermaid diagram",
        command: PaletteCommand::ZoomIn,
    },
    PaletteEntry {
        name: "zoom-out",
        description: "zoom out of the selected Mermaid diagram",
        command: PaletteCommand::ZoomOut,
    },
    PaletteEntry {
        name: "toc",
        description: "toggle the table of contents sidebar",
        command: PaletteCommand::Toc,
    },
    PaletteEntry {
        name: "search",
        description: "search within the document",
        command: PaletteCommand::Search,
    },
    PaletteEntry {
        name: "theme",
        description: "toggle between dark and light theme",
        command: PaletteCommand::ToggleTheme,
    },
    PaletteEntry {
        name: "q",
        description: "close the viewer window",
        command: PaletteCommand::Quit,
    },
];

fn filtered_palette_entries(query: &str) -> Vec<PaletteEntry> {
    let query = query.trim().trim_start_matches(':').to_lowercase();
    PALETTE_ENTRIES
        .iter()
        .copied()
        .filter(|entry| {
            query.is_empty()
                || entry.name.contains(&query)
                || entry.description.to_lowercase().contains(&query)
        })
        .collect()
}

pub struct NvmdApp {
    config: Config,
    options: AppOptions,
    document: Option<Document>,
    error: Option<String>,
    watcher: Option<FileWatcher>,
    watcher_error: Option<String>,
    pending_reload: Option<Instant>,
    mermaid: MermaidRenderer,
    mermaid_jobs: HashSet<String>,
    mermaid_results: Receiver<MermaidJobResult>,
    mermaid_sender: mpsc::Sender<MermaidJobResult>,
    render_generation: u64,
    highlighter: Option<Highlighter>,
    settings: ViewerSettings,
    show_settings: bool,
    settings_error: Option<String>,
    navigation: NavigationState,
    palette: CommandPalette,
    show_help: bool,
    show_toc: bool,
    cursor_sync: CursorSync,
    toc_entries: Vec<TocEntry>,
    search: SearchState,
    image_cache: std::collections::HashMap<String, ImageEntry>,
    image_results: Receiver<ImageJobResult>,
    image_sender: mpsc::Sender<ImageJobResult>,
}

#[derive(Debug, Clone)]
pub struct AppOptions {
    pub render_mermaid: bool,
    pub cursor_file: Option<std::path::PathBuf>,
    pub content_file: Option<std::path::PathBuf>,
}

const MAX_CONCURRENT_MERMAID_JOBS: usize = 4;

struct MermaidJobResult {
    key: String,
    generation: u64,
    result: Result<egui::ColorImage, String>,
}

impl NvmdApp {
    pub fn try_new(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        options: AppOptions,
    ) -> Result<Self, String> {
        let ctx = cc.egui_ctx.clone();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let settings = ViewerSettings::load();
            let markdown_style = settings.style();
            render::theme::configure(&ctx, &markdown_style);

            let reload_path = reload_path(&config, &options);
            let watcher_result = FileWatcher::watch(reload_path);
            let watcher_error = watcher_result.as_ref().err().map(|err| err.to_string());
            let (mermaid_sender, mermaid_results) = mpsc::channel();
            let (image_sender, image_results) = mpsc::channel();
            let cursor_sync = CursorSync::new(options.cursor_file.clone());
            let highlighter = Highlighter::new().ok();
            let mut app = Self {
                watcher: watcher_result.ok(),
                watcher_error,
                config,
                options,
                document: None,
                error: None,
                pending_reload: None,
                mermaid: MermaidRenderer::new(),
                mermaid_jobs: HashSet::new(),
                mermaid_results,
                mermaid_sender,
                render_generation: 0,
                highlighter,
                settings,
                show_settings: false,
                settings_error: None,
                navigation: NavigationState::default(),
                palette: CommandPalette::default(),
                show_help: false,
                show_toc: false,
                cursor_sync,
                toc_entries: Vec::new(),
                search: SearchState::default(),
                image_cache: HashMap::new(),
                image_results,
                image_sender,
            };

            if app.watcher.is_none() {
                app.watcher_error = Some(format!(
                    "{}; live reload disabled",
                    app.watcher_error
                        .take()
                        .unwrap_or_else(|| "file watcher is unavailable".to_owned())
                ));
            }
            app.reload();
            app
        }))
        .map_err(|payload| format!("startup panic: {}", panic_message(payload)))
    }

    pub fn from_startup_error(message: String) -> Self {
        let (mermaid_sender, mermaid_results) = mpsc::channel();
        let (image_sender, image_results) = mpsc::channel();
        Self {
            config: Config::fallback(),
            options: AppOptions {
                render_mermaid: false,
                cursor_file: None,
                content_file: None,
            },
            document: None,
            error: Some(message),
            watcher: None,
            watcher_error: Some("startup failed; live reload disabled".to_owned()),
            pending_reload: None,
            mermaid: MermaidRenderer::new(),
            mermaid_jobs: HashSet::new(),
            mermaid_results,
            mermaid_sender,
            render_generation: 0,
            highlighter: None,
            settings: ViewerSettings::default(),
            show_settings: false,
            settings_error: None,
            navigation: NavigationState::default(),
            palette: CommandPalette::default(),
            show_help: false,
            show_toc: false,
            cursor_sync: CursorSync::default(),
            toc_entries: Vec::new(),
            search: SearchState::default(),
            image_cache: HashMap::new(),
            image_results,
            image_sender,
        }
    }

    fn reload(&mut self) {
        self.render_generation = self.render_generation.wrapping_add(1);
        self.mermaid_jobs.clear();
        self.image_cache.clear();
        match loader::load_markdown(reload_path(&self.config, &self.options)) {
            Ok(source) => {
                let mut document = parser::parse_markdown(&source);
                document.source_path = Some(self.config.markdown_path.clone());
                self.toc_entries = extract_toc(&document);
                if self.search.active {
                    self.search.update_matches(&document);
                }
                self.document = Some(document);
                self.error = None;
            }
            Err(err) => {
                self.document = None;
                self.toc_entries.clear();
                self.error = Some(err.to_string());
            }
        }
    }

    fn handle_watcher(&mut self, ctx: &egui::Context) {
        let reload_path = reload_path(&self.config, &self.options);
        if let Some(watcher) = &self.watcher {
            let changed = watcher
                .changed_paths()
                .into_iter()
                .any(|path| same_file_event(&path, reload_path));
            if changed {
                self.pending_reload = Some(Instant::now());
                ctx.request_repaint_after(RELOAD_DEBOUNCE);
            }
        }

        if self
            .pending_reload
            .map(|at| at.elapsed() >= RELOAD_DEBOUNCE)
            .unwrap_or(false)
        {
            self.pending_reload = None;
            self.reload();
        }
    }

    fn handle_cursor_sync(&mut self, ctx: &egui::Context) {
        if let Some(line) = self.cursor_sync.take_changed_line() {
            if let Some(index) = self
                .document
                .as_ref()
                .and_then(|document| document.block_index_for_line(line))
            {
                self.navigation.request_source_block(index);
            }
        }
        if self.cursor_sync.is_enabled() {
            ctx.request_repaint_after(CURSOR_SYNC_POLL_INTERVAL);
        }
    }
}

fn reload_path<'a>(config: &'a Config, options: &'a AppOptions) -> &'a std::path::Path {
    options
        .content_file
        .as_deref()
        .unwrap_or(&config.markdown_path)
}

fn same_file_event(event_path: &std::path::Path, target_path: &std::path::Path) -> bool {
    event_path == target_path
        || (event_path.file_name().is_some()
            && event_path.file_name() == target_path.file_name()
            && event_path.parent() == target_path.parent())
}

fn settings_section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(10.0);
    let outer_width = ui.available_width();
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(10, 14, 22))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 40, 70)))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width((outer_width - 24.0).max(1.0));
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(84, 104, 136)),
            );
            ui.add_space(8.0);
            add_contents(ui);
        });
}

fn settings_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(13.0)
                .color(egui::Color32::from_rgb(140, 164, 200)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::DragValue::new(value)
                    .range(range.clone())
                    .speed(0.5)
                    .max_decimals(1),
            );
        });
    });
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(
            egui::Slider::new(value, range)
                .show_value(false)
                .trailing_fill(true),
        );
    });
    ui.add_space(4.0);
}

fn keycap(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(18, 26, 44))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 52, 88)))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(11.0)
                    .color(egui::Color32::from_rgb(194, 208, 232)),
            );
        });
}

impl eframe::App for NvmdApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.update_inner(ctx);
        })) {
            self.error = Some(format!("UI render panic: {}", panic_message(payload)));
        }
    }
}

impl NvmdApp {
    fn update_inner(&mut self, ctx: &egui::Context) {
        self.handle_input(ctx);

        self.handle_watcher(ctx);
        self.handle_cursor_sync(ctx);
        self.collect_mermaid_results();
        self.collect_image_results();
        self.start_mermaid_jobs(ctx);
        self.start_image_jobs(ctx);
        let mut markdown_style = self.settings.style();

        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(markdown_style.colors.chrome_background)
                    .stroke(egui::Stroke::new(1.0, markdown_style.colors.chrome_border))
                    .inner_margin(egui::Margin::symmetric(18, 11)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.config.file_name())
                            .size(14.0)
                            .strong()
                            .color(markdown_style.colors.strong_text),
                    );
                    ui.label(
                        egui::RichText::new("·")
                            .size(14.0)
                            .color(markdown_style.colors.muted_text),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(self.config.markdown_path.display().to_string())
                                .size(12.0)
                                .color(markdown_style.colors.muted_text),
                        )
                        .truncate(),
                    );
                });
            });

        if self.show_settings {
            self.render_settings_panel(ctx);
            markdown_style = self.settings.style();
        }

        if self.show_toc {
            self.render_toc_panel(ctx, &markdown_style);
        }

        self.render_search_bar(ctx, &markdown_style);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(markdown_style.colors.app_background))
            .show(ctx, |ui| {
                if let Some(watcher_error) = &self.watcher_error {
                    egui::Frame::new()
                        .fill(markdown_style.colors.warning_background)
                        .stroke(egui::Stroke::new(1.0, markdown_style.colors.warning_border))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.colored_label(markdown_style.colors.warning_text, watcher_error);
                        });
                    ui.add_space(10.0);
                }

                let document_scroll = self.navigation.take_document_scroll();
                let document_jump = self.navigation.take_document_jump();
                self.navigation.begin_target_collection();
                let scroll_output = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if document_scroll != 0.0 {
                            ui.scroll_with_delta(egui::vec2(0.0, document_scroll));
                        }
                        if document_jump == Some(DocumentJump::Top) {
                            ui.scroll_to_cursor(Some(egui::Align::TOP));
                        }
                        let available_width = ui.available_width();
                        let side_margin = responsive_side_margin(available_width);
                        let page_width =
                            responsive_page_width(available_width, side_margin, &markdown_style);
                        let inner_margin =
                            responsive_inner_margin(available_width, &markdown_style);
                        let top_margin = if available_width < 560.0 { 8.0 } else { 18.0 };

                        ui.add_space(top_margin);
                        ui.vertical_centered(|ui| {
                            ui.set_width(page_width);
                            egui::Frame::new()
                                .fill(markdown_style.colors.page_background)
                                .stroke(egui::Stroke::new(1.0, markdown_style.colors.page_border))
                                .inner_margin(egui::Margin::same(inner_margin))
                                .show(ui, |ui| {
                                    ui.with_layout(
                                        egui::Layout::top_down(egui::Align::LEFT),
                                        |ui| {
                                            ui.set_width(
                                                (page_width - f32::from(inner_margin) * 2.0)
                                                    .max(1.0),
                                            );
                                            if let Some(error) = &self.error {
                                                ui.colored_label(
                                                    egui::Color32::from_rgb(255, 100, 100),
                                                    error,
                                                );
                                            } else if let Some(document) = &mut self.document {
                                                let search_match = self.search.current_match();
                                                crate::document::render::render_document(
                                                    ui,
                                                    document,
                                                    self.options.render_mermaid,
                                                    &markdown_style,
                                                    &mut self.navigation,
                                                    self.highlighter.as_ref(),
                                                    &mut self.image_cache,
                                                    search_match,
                                                );
                                            }
                                        },
                                    );
                                });
                        });
                        ui.add_space(if available_width < 560.0 { 12.0 } else { 28.0 });
                        if document_jump == Some(DocumentJump::Bottom) {
                            ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        }
                    });
                self.navigation.set_viewport(scroll_output.inner_rect);
            });

        if self.show_help {
            self.render_help_panel(ctx);
        }
        if self.palette.open {
            self.render_command_palette(ctx);
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        if self.palette.open {
            return;
        }
        if self.show_help && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.show_help = false;
            return;
        }
        if self.search.active {
            if !ctx.wants_keyboard_input() {
                if ctx.input(|i| i.key_pressed(egui::Key::N) && i.modifiers.shift) {
                    self.search.prev();
                    if let Some(idx) = self.search.current_match() {
                        self.navigation.request_source_block(idx);
                    }
                } else if ctx.input(|i| i.key_pressed(egui::Key::N)) {
                    self.search.next();
                    if let Some(idx) = self.search.current_match() {
                        self.navigation.request_source_block(idx);
                    }
                }
            }
            return;
        }
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::T)) {
                self.show_toc = !self.show_toc;
                return;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Slash)) {
                self.search.open();
                return;
            }
        }
        for action in crate::input::collect_actions(ctx, &mut self.navigation) {
            match action {
                InputAction::OpenPalette => self.palette.open(),
                InputAction::ToggleSettings => {
                    self.show_settings = !self.show_settings;
                }
                InputAction::CloseWindow => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn render_command_palette(&mut self, ctx: &egui::Context) {
        let width = (ctx.available_rect().width() - 32.0).clamp(300.0, 620.0);
        let mut execute = None;
        let mut close = false;

        egui::Area::new(egui::Id::new("command-palette"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -18.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(self.settings.style().colors.chrome_background)
                    .stroke(egui::Stroke::new(
                        1.0,
                        self.settings.style().colors.chrome_border,
                    ))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(":")
                                    .font(egui::FontId::monospace(16.0))
                                    .color(self.settings.style().colors.link),
                            );
                            let before = self.palette.query.clone();
                            let response = ui.add_sized(
                                [width - 28.0, 28.0],
                                egui::TextEdit::singleline(&mut self.palette.query)
                                    .id(egui::Id::new("command-palette-input"))
                                    .hint_text("command"),
                            );
                            if self.palette.request_focus {
                                response.request_focus();
                                self.palette.request_focus = false;
                            }
                            if before != self.palette.query {
                                self.palette.selected = 0;
                                self.palette.status = None;
                            }
                        });

                        let entries = filtered_palette_entries(&self.palette.query);
                        if !entries.is_empty() {
                            if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                                self.palette.selected = (self.palette.selected + 1) % entries.len();
                            }
                            if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                                self.palette.selected =
                                    (self.palette.selected + entries.len() - 1) % entries.len();
                            }
                            self.palette.selected =
                                self.palette.selected.min(entries.len().saturating_sub(1));
                        }

                        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                            close = true;
                        } else if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                            if let Some(entry) = entries.get(self.palette.selected) {
                                execute = Some(entry.command);
                            } else {
                                self.palette.status =
                                    Some("No matching command. Use :help for the list.".to_owned());
                            }
                        }

                        ui.add_space(6.0);
                        for (index, entry) in entries.iter().enumerate() {
                            let selected = index == self.palette.selected;
                            let label = format!(":{:<10} {}", entry.name, entry.description);
                            if ui
                                .selectable_label(
                                    selected,
                                    egui::RichText::new(label).font(egui::FontId::monospace(12.0)),
                                )
                                .clicked()
                            {
                                execute = Some(entry.command);
                            }
                        }
                        if entries.is_empty() {
                            ui.label(
                                egui::RichText::new("No matching commands")
                                    .color(self.settings.style().colors.muted_text),
                            );
                        }
                        if let Some(status) = &self.palette.status {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(status)
                                    .color(self.settings.style().colors.warning_text),
                            );
                        }
                    });
            });

        if close {
            self.palette.close();
        } else if let Some(command) = execute {
            self.execute_palette_command(ctx, command);
        }
    }

    fn execute_palette_command(&mut self, ctx: &egui::Context, command: PaletteCommand) {
        let result = match command {
            PaletteCommand::Help => {
                self.show_help = true;
                Ok(())
            }
            PaletteCommand::Settings => {
                self.show_settings = !self.show_settings;
                Ok(())
            }
            PaletteCommand::Reload => {
                self.pending_reload = None;
                self.reload();
                Ok(())
            }
            PaletteCommand::Top => {
                self.navigation.request_document_jump(DocumentJump::Top);
                Ok(())
            }
            PaletteCommand::Bottom => {
                self.navigation.request_document_jump(DocumentJump::Bottom);
                Ok(())
            }
            PaletteCommand::MermaidNext => self
                .navigation
                .select_relative_target(1)
                .then_some(())
                .ok_or("No rendered Mermaid diagrams are available."),
            PaletteCommand::MermaidPrevious => self
                .navigation
                .select_relative_target(-1)
                .then_some(())
                .ok_or("No rendered Mermaid diagrams are available."),
            PaletteCommand::MermaidOpen => self
                .navigation
                .open_selected_mermaid()
                .then_some(())
                .ok_or("Select a Mermaid diagram before using :mopen."),
            PaletteCommand::Fit => self
                .navigation
                .request_mermaid_command(MermaidViewportCommand::Fit)
                .then_some(())
                .ok_or("Select a Mermaid diagram before using :fit."),
            PaletteCommand::ZoomIn => self
                .navigation
                .request_mermaid_command(MermaidViewportCommand::ZoomIn)
                .then_some(())
                .ok_or("Select a Mermaid diagram before using :zoom-in."),
            PaletteCommand::ZoomOut => self
                .navigation
                .request_mermaid_command(MermaidViewportCommand::ZoomOut)
                .then_some(())
                .ok_or("Select a Mermaid diagram before using :zoom-out."),
            PaletteCommand::Toc => {
                self.show_toc = !self.show_toc;
                Ok(())
            }
            PaletteCommand::Search => {
                self.search.open();
                Ok(())
            }
            PaletteCommand::ToggleTheme => {
                use crate::render::settings::MarkdownPreset;
                self.settings.preset = if self.settings.preset == MarkdownPreset::Light {
                    MarkdownPreset::Dark
                } else {
                    MarkdownPreset::Light
                };
                render::theme::configure(ctx, &self.settings.style());
                Ok(())
            }
            PaletteCommand::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                Ok(())
            }
        };

        match result {
            Ok(()) => self.palette.close(),
            Err(message) => self.palette.status = Some(message.to_owned()),
        }
    }

    fn render_help_panel(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("Keyboard Commands")
            .id(egui::Id::new("command-help"))
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label("Command palette");
                for entry in PALETTE_ENTRIES {
                    ui.horizontal(|ui| {
                        keycap(ui, &format!(":{}", entry.name));
                        ui.label(entry.description);
                    });
                }
                ui.add_space(8.0);
                ui.separator();
                ui.label("Direct shortcuts");
                ui.label("Esc settings / close large view / exit Mermaid   q quit   j/k scroll");
                ui.label(
                    "Space j/k select Mermaid   Enter open/enlarge   f fit   h/j/k/l pan   [/] zoom",
                );
            });
        self.show_help = open;
    }

    fn render_settings_panel(&mut self, ctx: &egui::Context) {
        let available_rect = ctx.available_rect();
        let popup_width = (available_rect.width() - 32.0).clamp(240.0, 380.0);
        let popup_height = (available_rect.height() - 32.0).clamp(240.0, 680.0);
        let mut popup_open = self.show_settings;
        let mut close_clicked = false;

        egui::Window::new("Viewer Settings")
            .id(egui::Id::new("settings"))
            .open(&mut popup_open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(popup_width)
            .min_width(popup_width)
            .max_width(popup_width)
            .max_height(popup_height)
            .frame(
                egui::Frame::new()
                    .fill(self.settings.style().colors.chrome_background)
                    .stroke(egui::Stroke::new(
                        1.0,
                        self.settings.style().colors.chrome_border,
                    ))
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        use crate::render::settings::MarkdownPreset;
                        let is_light = self.settings.preset == MarkdownPreset::Light;
                        let label = if is_light { "☀ Light" } else { "☾ Dark" };
                        if ui.button(label).clicked() {
                            self.settings.preset = if is_light {
                                MarkdownPreset::Dark
                            } else {
                                MarkdownPreset::Light
                            };
                            render::theme::configure(ctx, &self.settings.style());
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    keycap(ui, "Esc");
                    ui.label("toggle settings");
                    keycap(ui, "q");
                    ui.label("close window");
                    keycap(ui, ":");
                    ui.label("commands");
                });
                ui.add_space(8.0);
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((popup_height - 82.0).max(120.0))
                    .scroll_bar_visibility(
                        egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible,
                    )
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        settings_section(ui, "Document", |ui| {
                            settings_slider(
                                ui,
                                &mut self.settings.page_max_width,
                                360.0..=1600.0,
                                "Max width",
                            );
                            settings_slider(
                                ui,
                                &mut self.settings.page_inner_margin,
                                0.0..=96.0,
                                "Padding",
                            );
                            settings_slider(
                                ui,
                                &mut self.settings.line_height,
                                1.0..=2.4,
                                "Line height",
                            );
                        });

                        settings_section(ui, "Typography", |ui| {
                            settings_slider(
                                ui,
                                &mut self.settings.body_font_size,
                                10.0..=40.0,
                                "Body font",
                            );
                            settings_slider(
                                ui,
                                &mut self.settings.code_font_size,
                                10.0..=40.0,
                                "Code font",
                            );
                        });

                        settings_section(ui, "Spacing", |ui| {
                            settings_slider(
                                ui,
                                &mut self.settings.paragraph_gap,
                                0.0..=64.0,
                                "Paragraph gap",
                            );
                            settings_slider(
                                ui,
                                &mut self.settings.code_margin,
                                0.0..=96.0,
                                "Code padding",
                            );
                            settings_slider(
                                ui,
                                &mut self.settings.quote_margin,
                                0.0..=96.0,
                                "Quote padding",
                            );
                            settings_slider(
                                ui,
                                &mut self.settings.list_marker_width,
                                0.0..=64.0,
                                "List marker",
                            );
                        });

                        settings_section(ui, "Headings", |ui| {
                            for (index, size) in self.settings.heading_sizes.iter_mut().enumerate()
                            {
                                settings_slider(ui, size, 10.0..=40.0, &format!("H{}", index + 1));
                            }
                        });

                        settings_section(ui, "Table", |ui| {
                            settings_slider(
                                ui,
                                &mut self.settings.table_spacing_x,
                                0.0..=64.0,
                                "Cell spacing X",
                            );
                            settings_slider(
                                ui,
                                &mut self.settings.table_spacing_y,
                                0.0..=64.0,
                                "Cell spacing Y",
                            );
                        });

                        ui.add_space(10.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            let save = egui::Button::new(
                                egui::RichText::new("Save settings")
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(38, 96, 188));
                            if ui.add(save).clicked() {
                                self.save_settings();
                            }

                            let reset = egui::Button::new(
                                egui::RichText::new("Reset")
                                    .color(egui::Color32::from_rgb(140, 164, 200)),
                            )
                            .fill(egui::Color32::from_rgb(18, 26, 44));
                            if ui.add(reset).clicked() {
                                self.settings.reset_to_default();
                                self.save_settings();
                            }
                        });
                        if let Some(message) = &self.settings_error {
                            ui.add_space(8.0);
                            ui.label(message);
                        }
                    });
            });

        self.show_settings = popup_open && !close_clicked;
    }

    fn save_settings(&mut self) {
        self.settings_error = self
            .settings
            .save()
            .err()
            .map(|err| format!("failed to save settings: {err}"));
        if self.settings_error.is_none() {
            self.settings_error = Some("settings saved".to_owned());
        }
    }

    fn collect_mermaid_results(&mut self) {
        while let Ok(result) = self.mermaid_results.try_recv() {
            self.mermaid_jobs.remove(&result.key);
            if result.generation != self.render_generation {
                continue;
            }
            if let Some(document) = &mut self.document {
                apply_mermaid_result(&mut document.blocks, result);
            }
        }
    }

    fn start_mermaid_jobs(&mut self, ctx: &egui::Context) {
        if !self.options.render_mermaid {
            return;
        }
        if self.mermaid_jobs.len() >= MAX_CONCURRENT_MERMAID_JOBS {
            return;
        }

        let Some(document) = &self.document else {
            return;
        };
        let jobs = pending_mermaid_sources(&document.blocks);
        for source in jobs {
            if self.mermaid_jobs.len() >= MAX_CONCURRENT_MERMAID_JOBS {
                break;
            }
            let key = MermaidCache::key(&source);
            if !self.mermaid_jobs.insert(key.clone()) {
                continue;
            }

            let sender = self.mermaid_sender.clone();
            let mermaid = self.mermaid.clone();
            let repaint_ctx = ctx.clone();
            let generation = self.render_generation;
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    mermaid.render_image(&source).map_err(|err| err.to_string())
                }))
                .map_err(|payload| {
                    format!(
                        "native Mermaid renderer panicked: {}",
                        panic_message(payload)
                    )
                })
                .and_then(|result| result);

                let _ = sender.send(MermaidJobResult { key, generation, result });
                repaint_ctx.request_repaint();
            });
        }
    }

    fn collect_image_results(&mut self) {
        while let Ok(job) = self.image_results.try_recv() {
            let entry = match job.result {
                Ok(image) => ImageEntry::Loaded(image),
                Err(reason) => ImageEntry::Failed(reason),
            };
            self.image_cache.insert(job.path, entry);
        }
    }

    fn start_image_jobs(&mut self, ctx: &egui::Context) {
        let pending: Vec<String> = self
            .image_cache
            .iter()
            .filter_map(|(path, entry)| {
                if matches!(entry, ImageEntry::Loading) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect();

        for path in pending {
            let sender = self.image_sender.clone();
            let path_clone = path.clone();
            let repaint_ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = load_image_from_path(&path_clone);
                let _ = sender.send(ImageJobResult { path: path_clone, result });
                repaint_ctx.request_repaint();
            });
        }
    }

    fn render_toc_panel(&mut self, ctx: &egui::Context, markdown_style: &crate::render::settings::MarkdownStyle) {
        let mut jump_to: Option<usize> = None;
        egui::SidePanel::left("toc-panel")
            .resizable(true)
            .default_width(220.0)
            .min_width(140.0)
            .max_width(360.0)
            .frame(
                egui::Frame::new()
                    .fill(markdown_style.colors.chrome_background)
                    .stroke(egui::Stroke::new(1.0, markdown_style.colors.chrome_border))
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Contents")
                            .size(12.0)
                            .strong()
                            .color(markdown_style.colors.muted_text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            self.show_toc = false;
                        }
                    });
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for entry in &self.toc_entries {
                            let indent = (entry.level.saturating_sub(1) as f32) * 12.0;
                            ui.horizontal(|ui| {
                                ui.add_space(indent);
                                let size = if entry.level == 1 { 13.0 } else { 12.0 };
                                let color = if entry.level == 1 {
                                    markdown_style.colors.strong_text
                                } else {
                                    markdown_style.colors.text
                                };
                                if ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&entry.text)
                                            .size(size)
                                            .color(color),
                                    )
                                    .sense(egui::Sense::click())
                                    .truncate(),
                                ).clicked() {
                                    jump_to = Some(entry.block_index);
                                }
                            });
                            ui.add_space(2.0);
                        }
                    });
            });
        if let Some(index) = jump_to {
            self.navigation.request_source_block(index);
        }
    }

    fn render_search_bar(&mut self, ctx: &egui::Context, markdown_style: &crate::render::settings::MarkdownStyle) {
        if !self.search.active {
            return;
        }

        let mut close = false;
        let mut query_changed = false;
        let mut go_next = false;
        let mut go_prev = false;

        egui::TopBottomPanel::bottom("search-bar")
            .frame(
                egui::Frame::new()
                    .fill(markdown_style.colors.chrome_background)
                    .stroke(egui::Stroke::new(1.0, markdown_style.colors.chrome_border))
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("/")
                            .font(egui::FontId::monospace(14.0))
                            .color(markdown_style.colors.link),
                    );
                    let before = self.search.query.clone();
                    let response = ui.add_sized(
                        [200.0, 24.0],
                        egui::TextEdit::singleline(&mut self.search.query)
                            .id(egui::Id::new("search-input"))
                            .hint_text("search…"),
                    );
                    if self.search.request_focus {
                        response.request_focus();
                        self.search.request_focus = false;
                    }
                    if before != self.search.query {
                        query_changed = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go_next = true;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close = true;
                    }
                    if !self.search.matches.is_empty() {
                        let count_text = format!(
                            "{}/{}",
                            self.search.selected + 1,
                            self.search.matches.len()
                        );
                        ui.label(
                            egui::RichText::new(count_text)
                                .size(12.0)
                                .color(markdown_style.colors.muted_text),
                        );
                    } else if !self.search.query.is_empty() {
                        ui.label(
                            egui::RichText::new("no matches")
                                .size(12.0)
                                .color(markdown_style.colors.warning_text),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            close = true;
                        }
                        if ui.small_button("↓").on_hover_text("Next (n)").clicked() {
                            go_next = true;
                        }
                        if ui.small_button("↑").on_hover_text("Prev (N)").clicked() {
                            go_prev = true;
                        }
                    });
                });
            });

        if close {
            self.search.close();
        } else {
            if query_changed {
                if let Some(doc) = &self.document {
                    let doc_clone = doc.clone();
                    self.search.update_matches(&doc_clone);
                }
            }
            if go_next {
                self.search.next();
            }
            if go_prev {
                self.search.prev();
            }
            if let Some(block_index) = self.search.current_match() {
                if go_next || go_prev {
                    self.navigation.request_source_block(block_index);
                }
            }
        }
    }
}

fn load_image_from_path(path: &str) -> Result<egui::ColorImage, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &rgba,
    ))
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "unknown panic".to_owned()
    }
}

fn pending_mermaid_sources(blocks: &[Block]) -> Vec<String> {
    let mut sources = Vec::new();
    for block in blocks {
        match block {
            Block::Mermaid {
                source,
                render_state,
            } if render_state.is_pending() => sources.push(source.clone()),
            Block::Quote { blocks } => sources.extend(pending_mermaid_sources(blocks)),
            Block::List { items, .. } => {
                for item in items {
                    sources.extend(pending_mermaid_sources(&item.blocks));
                }
            }
            Block::FootnoteDefinition { blocks, .. } => {
                sources.extend(pending_mermaid_sources(blocks));
            }
            Block::DefinitionList { items } => {
                for item in items {
                    for blocks in &item.definitions {
                        sources.extend(pending_mermaid_sources(blocks));
                    }
                }
            }
            _ => {}
        }
    }
    sources
}

fn apply_mermaid_result(blocks: &mut [Block], result: MermaidJobResult) -> bool {
    for block in blocks {
        match block {
            Block::Mermaid {
                source,
                render_state,
            } if MermaidCache::key(source) == result.key => {
                *render_state = match result.result {
                    Ok(image) => crate::mermaid::renderer::MermaidRenderState::Rendered { image },
                    Err(reason) => crate::mermaid::renderer::MermaidRenderState::Failed { reason },
                };
                return true;
            }
            Block::Quote { blocks } => {
                if apply_mermaid_result(blocks, result_ref_clone(&result)) {
                    return true;
                }
            }
            Block::List { items, .. } => {
                for item in items {
                    if apply_mermaid_result(&mut item.blocks, result_ref_clone(&result)) {
                        return true;
                    }
                }
            }
            Block::FootnoteDefinition { blocks, .. } => {
                if apply_mermaid_result(blocks, result_ref_clone(&result)) {
                    return true;
                }
            }
            Block::DefinitionList { items } => {
                for item in items {
                    for blocks in &mut item.definitions {
                        if apply_mermaid_result(blocks, result_ref_clone(&result)) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn responsive_side_margin(available_width: f32) -> f32 {
    if available_width < 420.0 {
        4.0
    } else if available_width < 560.0 {
        10.0
    } else if available_width < 900.0 {
        16.0
    } else {
        28.0
    }
}

fn responsive_inner_margin(
    available_width: f32,
    markdown_style: &crate::render::settings::MarkdownStyle,
) -> i8 {
    let max_margin = (available_width * 0.18).clamp(0.0, 96.0);
    markdown_style.page_inner_margin.min(max_margin).round() as i8
}

fn responsive_page_width(
    available_width: f32,
    side_margin: f32,
    markdown_style: &crate::render::settings::MarkdownStyle,
) -> f32 {
    let usable_width = (available_width - side_margin * 2.0).max(1.0);
    if available_width < markdown_style.page_max_width + side_margin * 2.0 {
        usable_width
    } else {
        usable_width.min(markdown_style.page_max_width)
    }
}

fn extract_toc(document: &Document) -> Vec<TocEntry> {
    document
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            if let Block::Heading { level, content, .. } = block {
                Some(TocEntry {
                    block_index: index,
                    level: *level,
                    text: plain_text(content),
                })
            } else {
                None
            }
        })
        .collect()
}

fn result_ref_clone(result: &MermaidJobResult) -> MermaidJobResult {
    MermaidJobResult {
        key: result.key.clone(),
        generation: result.generation,
        result: result.result.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        filtered_palette_entries, reload_path, responsive_inner_margin, responsive_page_width,
        responsive_side_margin, AppOptions, PaletteCommand,
    };
    use crate::config::Config;
    use crate::render::settings::ViewerSettings;
    use std::path::PathBuf;

    #[test]
    fn narrow_viewports_preserve_responsive_side_margins() {
        let style = ViewerSettings::default().style();
        let width = responsive_page_width(400.0, responsive_side_margin(400.0), &style);

        assert_eq!(width, 392.0);
    }

    #[test]
    fn wide_viewports_cap_page_width_for_centered_gutters() {
        let style = ViewerSettings::default().style();
        let width = responsive_page_width(1440.0, responsive_side_margin(1440.0), &style);

        assert_eq!(width, style.page_max_width);
    }

    #[test]
    fn inner_padding_stays_at_setting_when_space_is_available() {
        let mut settings = ViewerSettings::default();
        settings.page_inner_margin = 48.0;
        let style = settings.style();

        assert_eq!(responsive_inner_margin(1440.0, &style), 48);
    }

    #[test]
    fn command_palette_filters_names_and_descriptions_case_insensitively() {
        assert_eq!(
            filtered_palette_entries(":ZOOM").len(),
            2,
            "both zoom commands should match"
        );
        assert_eq!(
            filtered_palette_entries("current Markdown")[0].command,
            PaletteCommand::Reload
        );
    }

    #[test]
    fn content_snapshot_overrides_disk_reload_source() {
        let config = Config::new(PathBuf::from("/tmp/document.md")).expect("config");
        let snapshot = PathBuf::from("/tmp/document-live.md");
        let options = AppOptions {
            render_mermaid: true,
            cursor_file: None,
            content_file: Some(snapshot.clone()),
        };

        assert_eq!(reload_path(&config, &options), snapshot.as_path());

        let saved_only = AppOptions {
            render_mermaid: true,
            cursor_file: None,
            content_file: None,
        };
        assert_eq!(reload_path(&config, &saved_only), config.markdown_path);
    }
}
