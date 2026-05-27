use eframe::egui::{
    self, pos2, vec2, Color32, FontFamily, FontId, Key, Label, Rect, RichText, Sense, Stroke,
    TextureOptions, Vec2,
};

use crate::input::{MermaidViewportCommand, NavigationState};
use crate::mermaid::{cache::MermaidCache, renderer::MermaidRenderState};

const DIAGRAM_MARGIN: f32 = 14.0;
const TOOLBAR_GAP: f32 = 10.0;
const MIN_VIEWPORT_HEIGHT: f32 = 240.0;
const MAX_VIEWPORT_HEIGHT: f32 = 720.0;
const VIEWPORT_HEIGHT_RATIO: f32 = 0.60;
const PAN_STEP: f32 = 32.0;
const ZOOM_STEP: f32 = 0.10;
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 4.0;
const EXPANDED_INITIAL_SCALE: f32 = 0.70;
const EXPANDED_SCALE_STEP: f32 = 0.10;
const EXPANDED_WINDOW_MARGIN: f32 = 16.0;
const CANVAS_MIN_GUTTER: f32 = 32.0;
const CANVAS_MAX_GUTTER: f32 = 96.0;

#[derive(Debug, Clone)]
struct MermaidViewportState {
    zoom: f32,
    pan: Vec2,
    fit_to_viewport: bool,
}

impl Default for MermaidViewportState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_to_viewport: false,
        }
    }
}

pub fn render_block(
    ui: &mut egui::Ui,
    diagram_index: usize,
    diagram_count: usize,
    source: &str,
    render_state: &mut MermaidRenderState,
    navigation: &mut NavigationState,
) {
    match render_state {
        MermaidRenderState::Rendered { image } => {
            let texture = ui.ctx().load_texture(
                format!("mermaid-{}", MermaidCache::key(source)),
                image.clone(),
                TextureOptions::LINEAR,
            );
            let original_size = texture.size_vec2();
            let state_id = egui::Id::new(("mermaid-viewport", diagram_index));
            let mut state = ui
                .data_mut(|data| data.get_temp::<MermaidViewportState>(state_id))
                .unwrap_or_default();
            let keyboard_active = navigation.is_selected(diagram_index);
            let expanded_size_step = navigation.expanded_size_step(diagram_index);
            if let Some(command) = navigation.take_mermaid_command(diagram_index) {
                apply_viewport_command(&mut state, command);
            }

            let response = egui::Frame::new()
                .fill(Color32::from_rgb(252, 253, 254))
                .stroke(Stroke::new(
                    if keyboard_active { 2.0 } else { 1.0 },
                    if keyboard_active {
                        Color32::from_rgb(80, 135, 190)
                    } else {
                        Color32::from_rgb(220, 226, 233)
                    },
                ))
                .inner_margin(egui::Margin::same(DIAGRAM_MARGIN as i8))
                .show(ui, |ui| {
                    let viewport_width = ui.available_width().max(1.0);
                    let width_fit_size = fit_width_size(original_size, viewport_width);
                    let viewport_height = viewport_height(ui, width_fit_size.y);
                    let viewport_size = vec2(viewport_width, viewport_height);
                    let fit_size =
                        viewport_image_size(original_size, viewport_size, state.fit_to_viewport);

                    toolbar(
                        ui,
                        &mut state,
                        keyboard_active,
                        false,
                        diagram_index,
                        diagram_count,
                    );
                    ui.add_space(TOOLBAR_GAP);

                    let (viewport_rect, viewport_response) =
                        ui.allocate_exact_size(viewport_size, Sense::click_and_drag());
                    if expanded_size_step.is_none() && viewport_response.dragged() {
                        state.pan += viewport_response.drag_delta();
                    }
                    if expanded_size_step.is_none() {
                        state.pan = clamp_pan(state.pan, viewport_size, fit_size * state.zoom);
                    }

                    ui.painter()
                        .rect_filled(viewport_rect, 5.0, Color32::from_rgb(247, 249, 251));
                    let image_size = fit_size * state.zoom;
                    let image_rect = Rect::from_min_size(viewport_rect.min + state.pan, image_size);
                    ui.painter_at(viewport_rect).image(
                        texture.id(),
                        image_rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );

                    viewport_response
                });

            navigation.register_target(diagram_index, response.response.rect);
            if navigation.should_reveal(diagram_index) {
                response.response.scroll_to_me(Some(egui::Align::Center));
            }
            if response.inner.clicked() || response.inner.dragged() {
                navigation.select_from_click(diagram_index);
                ui.ctx().request_repaint();
            }
            if keyboard_active
                && !navigation.control_keys_consumed()
                && !ui.ctx().wants_keyboard_input()
            {
                if keyboard_controls(ui, &mut state) {
                    ui.ctx().request_repaint();
                }
            }
            if let Some(size_step) = expanded_size_step {
                render_expanded_window(
                    ui.ctx(),
                    &texture,
                    original_size,
                    &mut state,
                    navigation,
                    size_step,
                    diagram_index,
                    diagram_count,
                );
            }
            ui.data_mut(|data| data.insert_temp(state_id, state));
        }
        MermaidRenderState::Failed { reason } => {
            source_block(
                ui,
                source,
                Some(&format!("Mermaid render failed: {reason}")),
            );
        }
        MermaidRenderState::Pending => {
            source_block(ui, source, Some("Rendering Mermaid diagram..."));
        }
    }
}

fn toolbar(
    ui: &mut egui::Ui,
    state: &mut MermaidViewportState,
    active: bool,
    expanded: bool,
    diagram_index: usize,
    diagram_count: usize,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(if active { "MERMAID ACTIVE" } else { "MERMAID" })
                .font(FontId::new(12.0, FontFamily::Monospace))
                .color(Color32::from_rgb(96, 111, 129)),
        );
        ui.add_space(10.0);
        if ui.small_button("Fit").clicked() {
            fit_viewport(state);
        }
        if ui.small_button("-").clicked() {
            zoom(state, -ZOOM_STEP);
        }
        ui.label(
            RichText::new(format!("{:.0}%", state.zoom * 100.0))
                .font(FontId::new(12.0, FontFamily::Monospace))
                .color(Color32::from_rgb(70, 82, 95)),
        );
        if ui.small_button("+").clicked() {
            zoom(state, ZOOM_STEP);
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new(if expanded {
                format!(
                    "{} of {} | f fit | Enter enlarge | Esc close | h j k l pan | [ ] zoom",
                    diagram_index + 1,
                    diagram_count.max(1)
                )
            } else if active {
                format!(
                    "{} of {} | f fit | Enter large view | Esc exit | h j k l pan | [ ] zoom",
                    diagram_index + 1,
                    diagram_count.max(1)
                )
            } else {
                format!(
                    "{} of {} | Space j/k select | : commands",
                    diagram_index + 1,
                    diagram_count.max(1)
                )
            })
            .size(12.0)
            .color(Color32::from_rgb(96, 111, 129)),
        );
    });
}

fn render_expanded_window(
    ctx: &egui::Context,
    texture: &egui::TextureHandle,
    original_size: Vec2,
    state: &mut MermaidViewportState,
    navigation: &mut NavigationState,
    size_step: u8,
    diagram_index: usize,
    diagram_count: usize,
) {
    let available = ctx.available_rect().size();
    let max_size = expanded_window_max_size(available);
    let popup_size = expanded_window_size(available, size_step);
    let mut open = true;
    egui::Window::new(format!("Mermaid Diagram {}", diagram_index + 1))
        .id(egui::Id::new(("mermaid-expanded-window", size_step)))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .collapsible(false)
        .resizable(true)
        .default_size(popup_size)
        .min_size(vec2(320.0_f32.min(max_size.x), 240.0_f32.min(max_size.y)))
        .max_size(max_size)
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(252, 253, 254))
                .stroke(Stroke::new(1.0, Color32::from_rgb(190, 202, 215)))
                .inner_margin(egui::Margin::same(DIAGRAM_MARGIN as i8)),
        )
        .show(ctx, |ui| {
            ui.set_min_size(popup_size);
            toolbar(ui, state, true, true, diagram_index, diagram_count);
            ui.add_space(TOOLBAR_GAP);

            let viewport_size = vec2(
                ui.available_width().max(1.0),
                (ui.available_height() - TOOLBAR_GAP).max(160.0),
            );
            let fit_size = viewport_image_size(original_size, viewport_size, state.fit_to_viewport);
            let image_size = fit_size * state.zoom;
            let (viewport_rect, response) =
                ui.allocate_exact_size(viewport_size, Sense::click_and_drag());
            if response.dragged() {
                state.pan += response.drag_delta();
            }
            let origin = expanded_canvas_origin(viewport_size, image_size, state.fit_to_viewport);
            state.pan = clamp_canvas_pan(state.pan, viewport_size, image_size, origin);
            ui.painter()
                .rect_filled(viewport_rect, 5.0, Color32::from_rgb(247, 249, 251));
            ui.painter_at(viewport_rect).image(
                texture.id(),
                Rect::from_min_size(viewport_rect.min + origin + state.pan, image_size),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        });
    if !open {
        navigation.close_expanded_mermaid();
    }
}

fn expanded_window_max_size(available: Vec2) -> Vec2 {
    vec2(
        (available.x - EXPANDED_WINDOW_MARGIN * 2.0).max(1.0),
        (available.y - EXPANDED_WINDOW_MARGIN * 2.0).max(1.0),
    )
}

fn expanded_window_size(available: Vec2, size_step: u8) -> Vec2 {
    let max_size = expanded_window_max_size(available);
    let scale = (EXPANDED_INITIAL_SCALE + f32::from(size_step) * EXPANDED_SCALE_STEP).min(1.0);
    vec2(
        (available.x * scale)
            .max(320.0_f32.min(max_size.x))
            .min(max_size.x),
        (available.y * scale)
            .max(240.0_f32.min(max_size.y))
            .min(max_size.y),
    )
}

fn fit_width_size(original_size: Vec2, viewport_width: f32) -> Vec2 {
    original_size * (viewport_width / original_size.x).min(1.0)
}

fn viewport_image_size(original_size: Vec2, viewport_size: Vec2, fit_to_viewport: bool) -> Vec2 {
    if fit_to_viewport {
        original_size
            * (viewport_size.x / original_size.x)
                .min(viewport_size.y / original_size.y)
                .min(1.0)
    } else {
        fit_width_size(original_size, viewport_size.x)
    }
}

fn expanded_canvas_origin(viewport_size: Vec2, image_size: Vec2, fit_to_viewport: bool) -> Vec2 {
    let x = (viewport_size.x - image_size.x) / 2.0;
    let y = if fit_to_viewport || image_size.y <= viewport_size.y {
        (viewport_size.y - image_size.y) / 2.0
    } else {
        canvas_gutter(viewport_size.y)
    };
    vec2(x, y)
}

fn canvas_gutter(viewport_axis: f32) -> f32 {
    (viewport_axis * 0.15).clamp(CANVAS_MIN_GUTTER, CANVAS_MAX_GUTTER)
}

fn clamp_canvas_pan(pan: Vec2, viewport_size: Vec2, image_size: Vec2, origin: Vec2) -> Vec2 {
    vec2(
        clamp_canvas_axis(pan.x, viewport_size.x, image_size.x, origin.x),
        clamp_canvas_axis(pan.y, viewport_size.y, image_size.y, origin.y),
    )
}

fn clamp_canvas_axis(value: f32, viewport: f32, image: f32, origin: f32) -> f32 {
    let gutter = canvas_gutter(viewport);
    let positioned = origin + value;
    let leading_edge = gutter;
    let trailing_edge = viewport - gutter - image;
    positioned.clamp(
        trailing_edge.min(leading_edge),
        trailing_edge.max(leading_edge),
    ) - origin
}

fn keyboard_controls(ui: &egui::Ui, state: &mut MermaidViewportState) -> bool {
    let mut changed = false;
    if ui.input(|input| input.key_pressed(Key::F)) {
        fit_viewport(state);
        changed = true;
    }
    if ui.input(|input| input.key_pressed(Key::H)) {
        state.pan.x += PAN_STEP;
        changed = true;
    }
    if ui.input(|input| input.key_pressed(Key::L)) {
        state.pan.x -= PAN_STEP;
        changed = true;
    }
    if ui.input(|input| input.key_pressed(Key::K)) {
        state.pan.y += PAN_STEP;
        changed = true;
    }
    if ui.input(|input| input.key_pressed(Key::J)) {
        state.pan.y -= PAN_STEP;
        changed = true;
    }
    if ui.input(|input| input.key_pressed(Key::OpenBracket)) {
        zoom(state, -ZOOM_STEP);
        changed = true;
    }
    if ui.input(|input| input.key_pressed(Key::CloseBracket)) {
        zoom(state, ZOOM_STEP);
        changed = true;
    }
    changed
}

fn zoom(state: &mut MermaidViewportState, delta: f32) {
    state.zoom = (state.zoom + delta).clamp(MIN_ZOOM, MAX_ZOOM);
}

fn fit_viewport(state: &mut MermaidViewportState) {
    state.zoom = 1.0;
    state.pan = Vec2::ZERO;
    state.fit_to_viewport = true;
}

fn apply_viewport_command(state: &mut MermaidViewportState, command: MermaidViewportCommand) {
    match command {
        MermaidViewportCommand::Fit => fit_viewport(state),
        MermaidViewportCommand::ZoomIn => zoom(state, ZOOM_STEP),
        MermaidViewportCommand::ZoomOut => zoom(state, -ZOOM_STEP),
    }
}

fn viewport_height(ui: &egui::Ui, fit_height: f32) -> f32 {
    let responsive_height = (ui.ctx().screen_rect().height() * VIEWPORT_HEIGHT_RATIO)
        .clamp(MIN_VIEWPORT_HEIGHT, MAX_VIEWPORT_HEIGHT);
    fit_height.min(responsive_height).max(1.0)
}

fn clamp_pan(pan: Vec2, viewport_size: Vec2, image_size: Vec2) -> Vec2 {
    vec2(
        clamp_axis(pan.x, viewport_size.x, image_size.x),
        clamp_axis(pan.y, viewport_size.y, image_size.y),
    )
}

fn clamp_axis(value: f32, viewport: f32, image: f32) -> f32 {
    if image <= viewport {
        0.0
    } else {
        value.clamp(viewport - image, 0.0)
    }
}

fn source_block(ui: &mut egui::Ui, source: &str, message: Option<&str>) {
    if let Some(message) = message {
        ui.add(
            Label::new(
                RichText::new(message)
                    .size(14.0)
                    .color(Color32::from_rgb(156, 75, 45)),
            )
            .wrap(),
        );
    }
    ui.add_space(2.0);
    ui.label(
        RichText::new("MERMAID")
            .font(FontId::new(12.0, FontFamily::Monospace))
            .color(Color32::from_rgb(96, 111, 129)),
    );
    egui::Frame::new()
        .fill(Color32::from_rgb(247, 249, 251))
        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 226, 233)))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(source)
                                .font(FontId::new(14.0, FontFamily::Monospace))
                                .color(Color32::from_rgb(35, 43, 53)),
                        )
                        .selectable(true),
                    );
                });
        });
}

#[cfg(test)]
mod tests {
    use super::{
        apply_viewport_command, clamp_canvas_pan, clamp_pan, expanded_canvas_origin,
        expanded_window_max_size, expanded_window_size, fit_width_size, viewport_image_size, zoom,
        MermaidViewportState, MAX_ZOOM, MIN_ZOOM,
    };
    use crate::input::MermaidViewportCommand;
    use eframe::egui::vec2;

    #[test]
    fn clamps_pan_when_diagram_is_larger_than_viewport() {
        assert_eq!(
            clamp_pan(vec2(-250.0, 20.0), vec2(300.0, 240.0), vec2(500.0, 480.0)),
            vec2(-200.0, 0.0),
        );
    }

    #[test]
    fn resets_pan_axis_when_image_fits() {
        assert_eq!(
            clamp_pan(vec2(-20.0, -30.0), vec2(300.0, 240.0), vec2(200.0, 100.0)),
            vec2(0.0, 0.0),
        );
    }

    #[test]
    fn clamps_zoom_range() {
        let mut state = MermaidViewportState::default();
        zoom(&mut state, -10.0);
        assert_eq!(state.zoom, MIN_ZOOM);
        zoom(&mut state, 10.0);
        assert_eq!(state.zoom, MAX_ZOOM);
    }

    #[test]
    fn viewport_commands_reuse_toolbar_behaviors() {
        let mut state = MermaidViewportState {
            zoom: 2.0,
            pan: vec2(-30.0, -20.0),
            fit_to_viewport: false,
        };
        apply_viewport_command(&mut state, MermaidViewportCommand::Fit);
        assert_eq!(state.zoom, 1.0);
        assert_eq!(state.pan, vec2(0.0, 0.0));
        assert!(state.fit_to_viewport);

        apply_viewport_command(&mut state, MermaidViewportCommand::ZoomIn);
        assert_eq!(state.zoom, 1.0 + super::ZOOM_STEP);
        apply_viewport_command(&mut state, MermaidViewportCommand::ZoomOut);
        assert_eq!(state.zoom, 1.0);
    }

    #[test]
    fn repeated_expansion_steps_grow_until_the_available_window_size() {
        let available = vec2(1200.0, 900.0);
        let first = expanded_window_size(available, 0);
        let second = expanded_window_size(available, 1);
        let final_size = expanded_window_size(available, 3);

        assert!(second.x > first.x);
        assert!(second.y > first.y);
        assert_eq!(final_size, expanded_window_max_size(available));
        assert_eq!(expanded_window_size(available, 20), final_size);
    }

    #[test]
    fn expanded_view_keeps_tall_diagrams_pannable_to_the_bottom_edge() {
        let viewport = vec2(700.0, 500.0);
        let image = fit_width_size(vec2(700.0, 2200.0), viewport.x);
        let origin = expanded_canvas_origin(viewport, image, false);

        assert!(image.y > viewport.y);
        assert_eq!(
            origin + clamp_canvas_pan(vec2(0.0, -5000.0), viewport, image, origin),
            vec2(0.0, viewport.y - super::canvas_gutter(viewport.y) - image.y),
        );
    }

    #[test]
    fn expanded_canvas_leaves_workspace_beyond_left_and_right_edges() {
        let viewport = vec2(700.0, 500.0);
        let image = vec2(1200.0, 500.0);
        let origin = expanded_canvas_origin(viewport, image, false);
        let left_position = origin + clamp_canvas_pan(vec2(5000.0, 0.0), viewport, image, origin);
        let right_position = origin + clamp_canvas_pan(vec2(-5000.0, 0.0), viewport, image, origin);

        assert_eq!(left_position.x, super::canvas_gutter(viewport.x));
        assert_eq!(
            right_position.x,
            viewport.x - super::canvas_gutter(viewport.x) - image.x
        );
    }

    #[test]
    fn fit_mode_scales_the_whole_diagram_into_the_viewport() {
        let viewport = vec2(700.0, 500.0);
        let image = viewport_image_size(vec2(700.0, 2200.0), viewport, true);

        assert!(image.x <= viewport.x);
        assert!(image.y <= viewport.y);
    }
}
