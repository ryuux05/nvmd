use std::time::{Duration, Instant};

use eframe::egui;

const LEADER_TIMEOUT: Duration = Duration::from_millis(750);
const SCROLL_SPEED: f32 = 700.0; // pixels per second while key is held
const MAX_EXPANDED_SIZE_STEP: u8 = 3;

/// Parsed keybindings derived from `KeyConfig` strings.
#[derive(Debug, Clone)]
pub struct KeyMap {
    pub scroll_down: egui::Key,
    pub scroll_up: egui::Key,
    pub palette: egui::Key,
    pub quit: egui::Key,
    pub toc: egui::Key,
    pub search: egui::Key,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self {
            scroll_down: egui::Key::J,
            scroll_up: egui::Key::K,
            palette: egui::Key::Colon,
            quit: egui::Key::Q,
            toc: egui::Key::T,
            search: egui::Key::Slash,
        }
    }
}

pub fn parse_key(s: &str) -> Option<egui::Key> {
    match s.to_lowercase().as_str() {
        "a" => Some(egui::Key::A), "b" => Some(egui::Key::B), "c" => Some(egui::Key::C),
        "d" => Some(egui::Key::D), "e" => Some(egui::Key::E), "f" => Some(egui::Key::F),
        "g" => Some(egui::Key::G), "h" => Some(egui::Key::H), "i" => Some(egui::Key::I),
        "j" => Some(egui::Key::J), "k" => Some(egui::Key::K), "l" => Some(egui::Key::L),
        "m" => Some(egui::Key::M), "n" => Some(egui::Key::N), "o" => Some(egui::Key::O),
        "p" => Some(egui::Key::P), "q" => Some(egui::Key::Q), "r" => Some(egui::Key::R),
        "s" => Some(egui::Key::S), "t" => Some(egui::Key::T), "u" => Some(egui::Key::U),
        "v" => Some(egui::Key::V), "w" => Some(egui::Key::W), "x" => Some(egui::Key::X),
        "y" => Some(egui::Key::Y), "z" => Some(egui::Key::Z),
        "0" => Some(egui::Key::Num0), "1" => Some(egui::Key::Num1),
        "2" => Some(egui::Key::Num2), "3" => Some(egui::Key::Num3),
        "4" => Some(egui::Key::Num4), "5" => Some(egui::Key::Num5),
        "6" => Some(egui::Key::Num6), "7" => Some(egui::Key::Num7),
        "8" => Some(egui::Key::Num8), "9" => Some(egui::Key::Num9),
        "space" => Some(egui::Key::Space),
        "enter" | "return" => Some(egui::Key::Enter),
        "escape" | "esc" => Some(egui::Key::Escape),
        ":" | "colon" => Some(egui::Key::Colon),
        "/" | "slash" => Some(egui::Key::Slash),
        "-" | "minus" => Some(egui::Key::Minus),
        "+" | "plus" | "=" => Some(egui::Key::Plus),
        "[" => Some(egui::Key::OpenBracket),
        "]" => Some(egui::Key::CloseBracket),
        _ => None,
    }
}

impl KeyMap {
    pub fn from_config(config: &crate::render::settings::KeyConfig) -> Self {
        let defaults = Self::default();
        Self {
            scroll_down: parse_key(&config.scroll_down).unwrap_or(defaults.scroll_down),
            scroll_up: parse_key(&config.scroll_up).unwrap_or(defaults.scroll_up),
            palette: parse_key(&config.palette).unwrap_or(defaults.palette),
            quit: parse_key(&config.quit).unwrap_or(defaults.quit),
            toc: parse_key(&config.toc).unwrap_or(defaults.toc),
            search: parse_key(&config.search).unwrap_or(defaults.search),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationMode {
    Document,
    MermaidControl { index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentJump {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MermaidViewportCommand {
    Fit,
    ZoomIn,
    ZoomOut,
}

#[derive(Debug, Clone, Copy)]
struct ControlTarget {
    index: usize,
    rect: egui::Rect,
}

#[derive(Debug, Clone, Copy)]
struct ExpandedMermaid {
    index: usize,
    size_step: u8,
}

#[derive(Debug)]
pub struct NavigationState {
    pub mode: NavigationMode,
    leader_started: Option<Instant>,
    targets: Vec<ControlTarget>,
    viewport: Option<egui::Rect>,
    reveal_target: Option<usize>,
    document_scroll: f32,
    scroll_remaining: f32,
    document_jump: Option<DocumentJump>,
    source_block: Option<usize>,
    mermaid_command: Option<(usize, MermaidViewportCommand)>,
    expanded_mermaid: Option<ExpandedMermaid>,
    consumed_control_keys: bool,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            mode: NavigationMode::Document,
            leader_started: None,
            targets: Vec::new(),
            viewport: None,
            reveal_target: None,
            document_scroll: 0.0,
            scroll_remaining: 0.0,
            document_jump: None,
            source_block: None,
            mermaid_command: None,
            expanded_mermaid: None,
            consumed_control_keys: false,
        }
    }
}

impl NavigationState {
    pub fn begin_target_collection(&mut self) {
        self.targets.clear();
    }

    pub fn register_target(&mut self, index: usize, rect: egui::Rect) {
        self.targets.push(ControlTarget { index, rect });
        self.targets
            .sort_by(|left, right| left.rect.top().total_cmp(&right.rect.top()));
    }

    pub fn set_viewport(&mut self, viewport: egui::Rect) {
        self.viewport = Some(viewport);
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.mode == NavigationMode::MermaidControl { index }
    }

    pub fn select_from_click(&mut self, index: usize) {
        self.mode = NavigationMode::MermaidControl { index };
        self.leader_started = None;
    }

    pub fn should_reveal(&mut self, index: usize) -> bool {
        if self.reveal_target == Some(index) {
            self.reveal_target = None;
            true
        } else {
            false
        }
    }

    pub fn control_keys_consumed(&self) -> bool {
        self.consumed_control_keys
    }

    pub fn take_document_scroll(&mut self) -> f32 {
        std::mem::take(&mut self.document_scroll)
    }

    /// Returns the scroll delta to apply this frame, advancing the smooth-scroll
    /// animation toward the accumulated target using exponential easing.
    pub fn advance_scroll(&mut self, dt: f32) -> f32 {
        if self.scroll_remaining.abs() < 0.5 {
            self.scroll_remaining = 0.0;
            return 0.0;
        }
        let factor = 1.0 - (-14.0_f32 * dt).exp();
        let delta = self.scroll_remaining * factor;
        self.scroll_remaining -= delta;
        delta
    }

    pub fn has_scroll_remaining(&self) -> bool {
        self.scroll_remaining.abs() >= 0.5
    }

    pub fn request_document_jump(&mut self, jump: DocumentJump) {
        self.document_jump = Some(jump);
    }

    pub fn take_document_jump(&mut self) -> Option<DocumentJump> {
        self.document_jump.take()
    }

    pub fn request_source_block(&mut self, index: usize) {
        self.source_block = Some(index);
    }

    pub fn take_source_block(&mut self) -> Option<usize> {
        self.source_block.take()
    }

    pub fn has_pending_source_block(&self) -> bool {
        self.source_block.is_some()
    }

    pub fn apply_synced_block_mode(&mut self, mermaid_index: Option<usize>) {
        self.mode = mermaid_index
            .map(|index| NavigationMode::MermaidControl { index })
            .unwrap_or(NavigationMode::Document);
        self.leader_started = None;
    }

    pub fn select_relative_target(&mut self, direction: i32) -> bool {
        if self.targets.is_empty() {
            return false;
        }
        self.select_target(direction);
        true
    }

    pub fn request_mermaid_command(&mut self, command: MermaidViewportCommand) -> bool {
        let NavigationMode::MermaidControl { index } = self.mode else {
            return false;
        };
        self.mermaid_command = Some((index, command));
        true
    }

    pub fn take_mermaid_command(&mut self, index: usize) -> Option<MermaidViewportCommand> {
        if self.mermaid_command.map(|(target, _)| target) == Some(index) {
            return self.mermaid_command.take().map(|(_, command)| command);
        }
        None
    }

    #[cfg(test)]
    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded_mermaid
            .map(|expanded| expanded.index == index)
            .unwrap_or(false)
    }

    pub fn expanded_size_step(&self, index: usize) -> Option<u8> {
        self.expanded_mermaid
            .filter(|expanded| expanded.index == index)
            .map(|expanded| expanded.size_step)
    }

    pub fn open_selected_mermaid(&mut self) -> bool {
        let NavigationMode::MermaidControl { index } = self.mode else {
            return false;
        };
        if let Some(expanded) = &mut self.expanded_mermaid {
            if expanded.index == index {
                expanded.size_step = (expanded.size_step + 1).min(MAX_EXPANDED_SIZE_STEP);
                return true;
            }
        }
        self.expanded_mermaid = Some(ExpandedMermaid {
            index,
            size_step: 0,
        });
        true
    }

    pub fn close_expanded_mermaid(&mut self) -> bool {
        self.expanded_mermaid.take().is_some()
    }

    fn handle_navigation_input(&mut self, ctx: &egui::Context, keys: &KeyMap) {
        self.consumed_control_keys = false;
        if ctx.wants_keyboard_input() {
            self.leader_started = None;
            return;
        }

        if self
            .leader_started
            .map(|started| started.elapsed() > LEADER_TIMEOUT)
            .unwrap_or(false)
        {
            self.leader_started = None;
        }

        let leader_pressed = ctx.input(|input| input.key_pressed(egui::Key::Space));
        // key_pressed for leader chording; key_down for continuous scroll
        let direction_pressed = if ctx.input(|input| input.key_pressed(keys.scroll_down)) {
            Some(1_i32)
        } else if ctx.input(|input| input.key_pressed(keys.scroll_up)) {
            Some(-1_i32)
        } else {
            None
        };

        if self.apply_leader_keys(leader_pressed, direction_pressed) {
            return;
        }

        if self.mode == NavigationMode::Document {
            let dt = ctx.input(|i| i.unstable_dt).clamp(0.001, 0.1);
            let down = ctx.input(|i| i.key_down(keys.scroll_down));
            let up = ctx.input(|i| i.key_down(keys.scroll_up));
            if down { self.scroll_remaining -= SCROLL_SPEED * dt; }
            if up   { self.scroll_remaining += SCROLL_SPEED * dt; }
        }
    }

    fn apply_leader_keys(&mut self, leader_pressed: bool, direction: Option<i32>) -> bool {
        if leader_pressed {
            self.leader_started = Some(Instant::now());
            self.consumed_control_keys = true;
            if let Some(direction) = direction {
                self.leader_started = None;
                self.select_target(direction);
            }
            return true;
        }

        if self.leader_started.is_some() {
            if let Some(direction) = direction {
                self.leader_started = None;
                self.select_target(direction);
                self.consumed_control_keys = true;
            }
            return true;
        }

        false
    }

    fn select_target(&mut self, direction: i32) {
        if self.targets.is_empty() {
            return;
        }

        let selected = match self.mode {
            NavigationMode::MermaidControl { index } => {
                let current = self
                    .targets
                    .iter()
                    .position(|target| target.index == index)
                    .unwrap_or(0);
                if direction > 0 {
                    (current + 1) % self.targets.len()
                } else {
                    (current + self.targets.len() - 1) % self.targets.len()
                }
            }
            NavigationMode::Document => {
                let viewport = self.viewport.unwrap_or(egui::Rect::NOTHING);
                if direction > 0 {
                    self.targets
                        .iter()
                        .position(|target| target.rect.bottom() >= viewport.top())
                        .unwrap_or(0)
                } else {
                    self.targets
                        .iter()
                        .rposition(|target| target.rect.top() <= viewport.bottom())
                        .unwrap_or(self.targets.len() - 1)
                }
            }
        };

        let index = self.targets[selected].index;
        if let Some(expanded) = &mut self.expanded_mermaid {
            expanded.index = index;
        }
        self.mode = NavigationMode::MermaidControl { index };
        self.reveal_target = Some(index);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    OpenPalette,
    ToggleSettings,
    CloseWindow,
    HeadingJump(i32),
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

pub fn collect_actions(ctx: &egui::Context, navigation: &mut NavigationState, keys: &KeyMap) -> Vec<InputAction> {
    let mut actions = Vec::new();
    if !ctx.wants_keyboard_input() && ctx.input(|input| input.key_pressed(keys.palette)) {
        actions.push(InputAction::OpenPalette);
        return actions;
    }
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        if navigation.close_expanded_mermaid() {
            navigation.consumed_control_keys = true;
        } else if matches!(navigation.mode, NavigationMode::MermaidControl { .. }) {
            navigation.mode = NavigationMode::Document;
            navigation.leader_started = None;
            navigation.consumed_control_keys = true;
        } else {
            actions.push(InputAction::ToggleSettings);
        }
    }
    if !ctx.wants_keyboard_input() && ctx.input(|input| input.key_pressed(keys.quit)) {
        actions.push(InputAction::CloseWindow);
    }
    if !ctx.wants_keyboard_input()
        && ctx.input(|input| input.key_pressed(egui::Key::Enter))
        && navigation.open_selected_mermaid()
    {
        navigation.consumed_control_keys = true;
    }
    if !ctx.wants_keyboard_input() && navigation.mode == NavigationMode::Document {
        if ctx.input(|i| i.key_pressed(egui::Key::CloseBracket)) {
            actions.push(InputAction::HeadingJump(1));
        } else if ctx.input(|i| i.key_pressed(egui::Key::OpenBracket)) {
            actions.push(InputAction::HeadingJump(-1));
        }
    }
    if !ctx.wants_keyboard_input() {
        let zoom_in = ctx.input(|i| {
            i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)
        });
        let zoom_out = ctx.input(|i| i.key_pressed(egui::Key::Minus));
        let zoom_reset = ctx.input(|i| i.key_pressed(egui::Key::Num0));
        if zoom_in { actions.push(InputAction::ZoomIn); }
        if zoom_out { actions.push(InputAction::ZoomOut); }
        if zoom_reset { actions.push(InputAction::ZoomReset); }
    }
    navigation.handle_navigation_input(ctx, keys);
    actions
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentJump, MermaidViewportCommand, NavigationMode, NavigationState,
        MAX_EXPANDED_SIZE_STEP,
    };
    use eframe::egui::{pos2, Rect};

    #[test]
    fn cycles_targets_with_wrap() {
        let mut state = NavigationState::default();
        state.register_target(2, Rect::from_min_max(pos2(0.0, 20.0), pos2(10.0, 30.0)));
        state.register_target(5, Rect::from_min_max(pos2(0.0, 40.0), pos2(10.0, 50.0)));
        state.mode = NavigationMode::MermaidControl { index: 5 };

        state.select_target(1);
        assert_eq!(state.mode, NavigationMode::MermaidControl { index: 2 });
        state.select_target(-1);
        assert_eq!(state.mode, NavigationMode::MermaidControl { index: 5 });
    }

    #[test]
    fn selects_next_target_at_or_below_document_viewport() {
        let mut state = NavigationState::default();
        state.register_target(1, Rect::from_min_max(pos2(0.0, 10.0), pos2(10.0, 30.0)));
        state.register_target(2, Rect::from_min_max(pos2(0.0, 90.0), pos2(10.0, 120.0)));
        state.set_viewport(Rect::from_min_max(pos2(0.0, 60.0), pos2(10.0, 160.0)));

        state.select_target(1);

        assert_eq!(state.mode, NavigationMode::MermaidControl { index: 2 });
    }

    #[test]
    fn selects_previous_target_visible_above_document_position() {
        let mut state = NavigationState::default();
        state.register_target(1, Rect::from_min_max(pos2(0.0, 10.0), pos2(10.0, 30.0)));
        state.register_target(2, Rect::from_min_max(pos2(0.0, 190.0), pos2(10.0, 220.0)));
        state.set_viewport(Rect::from_min_max(pos2(0.0, 60.0), pos2(10.0, 160.0)));

        state.select_target(-1);

        assert_eq!(state.mode, NavigationMode::MermaidControl { index: 1 });
    }

    #[test]
    fn leader_chord_can_select_without_a_second_frame() {
        let mut state = NavigationState::default();
        state.register_target(4, Rect::from_min_max(pos2(0.0, 40.0), pos2(10.0, 80.0)));

        assert!(state.apply_leader_keys(true, Some(1)));

        assert_eq!(state.mode, NavigationMode::MermaidControl { index: 4 });
    }

    #[test]
    fn leader_prefix_selects_on_following_direction_key() {
        let mut state = NavigationState::default();
        state.register_target(4, Rect::from_min_max(pos2(0.0, 40.0), pos2(10.0, 80.0)));

        assert!(state.apply_leader_keys(true, None));
        assert_eq!(state.mode, NavigationMode::Document);
        assert!(state.apply_leader_keys(false, None));
        assert_eq!(state.mode, NavigationMode::Document);
        assert!(state.apply_leader_keys(false, Some(1)));

        assert_eq!(state.mode, NavigationMode::MermaidControl { index: 4 });
    }

    #[test]
    fn queues_document_jump_until_document_consumes_it() {
        let mut state = NavigationState::default();

        state.request_document_jump(DocumentJump::Bottom);

        assert_eq!(state.take_document_jump(), Some(DocumentJump::Bottom));
        assert_eq!(state.take_document_jump(), None);
    }

    #[test]
    fn synced_source_blocks_switch_between_document_and_mermaid_modes() {
        let mut state = NavigationState::default();

        state.request_source_block(4);
        assert_eq!(state.take_source_block(), Some(4));
        state.apply_synced_block_mode(Some(2));
        assert_eq!(state.mode, NavigationMode::MermaidControl { index: 2 });

        state.apply_synced_block_mode(None);
        assert_eq!(state.mode, NavigationMode::Document);
    }

    #[test]
    fn queues_viewport_command_only_for_selected_mermaid() {
        let mut state = NavigationState::default();
        assert!(!state.request_mermaid_command(MermaidViewportCommand::Fit));

        state.mode = NavigationMode::MermaidControl { index: 3 };
        assert!(state.request_mermaid_command(MermaidViewportCommand::ZoomIn));
        assert_eq!(state.take_mermaid_command(2), None);
        assert_eq!(
            state.take_mermaid_command(3),
            Some(MermaidViewportCommand::ZoomIn)
        );
    }

    #[test]
    fn opens_and_closes_expanded_view_for_selected_mermaid() {
        let mut state = NavigationState::default();
        assert!(!state.open_selected_mermaid());

        state.mode = NavigationMode::MermaidControl { index: 3 };
        assert!(state.open_selected_mermaid());
        assert!(state.is_expanded(3));
        assert_eq!(state.expanded_size_step(3), Some(0));
        assert!(state.open_selected_mermaid());
        assert_eq!(state.expanded_size_step(3), Some(1));
        assert!(state.close_expanded_mermaid());
        assert!(!state.is_expanded(3));
    }

    #[test]
    fn switching_target_keeps_expanded_view_open_on_new_diagram() {
        let mut state = NavigationState::default();
        state.register_target(2, Rect::from_min_max(pos2(0.0, 20.0), pos2(10.0, 30.0)));
        state.register_target(5, Rect::from_min_max(pos2(0.0, 40.0), pos2(10.0, 50.0)));
        state.mode = NavigationMode::MermaidControl { index: 2 };
        state.open_selected_mermaid();

        state.select_target(1);

        assert!(state.is_expanded(5));
    }

    #[test]
    fn expanded_mermaid_size_stops_at_the_maximum_step() {
        let mut state = NavigationState::default();
        state.mode = NavigationMode::MermaidControl { index: 2 };
        for _ in 0..10 {
            state.open_selected_mermaid();
        }

        assert_eq!(state.expanded_size_step(2), Some(MAX_EXPANDED_SIZE_STEP));
    }
}
