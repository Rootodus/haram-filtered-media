//! egui overlay management: state, renderer, texture updates, and tessellation.

use crate::gui::{self, AppState, Bridge};
use egui::epaint::ClippedPrimitive;
use egui::{Context, ViewportId};
use egui_wgpu::{Renderer as EguiRenderer, ScreenDescriptor};
use egui_winit::State as EguiState;
use parking_lot::Mutex;
use std::sync::Arc;
use wgpu::{Device, Queue};
use winit::event::WindowEvent;
use winit::window::Window;

/// Manages the egui state, renderer, and rendering.
pub struct EguiOverlay {
    egui_state: EguiState,
    egui_renderer: EguiRenderer,
    window: Arc<Window>,
}

impl EguiOverlay {
    /// Create a new egui overlay.
    pub fn new(window: Arc<Window>, device: &Device, format: wgpu::TextureFormat) -> Self {
        let egui_ctx = Context::default();

        // Apply a modern dark theme with custom spacing and font sizes
        let mut style = egui::Style::default();
        style.visuals = egui::Visuals::dark();
        style.spacing.item_spacing = egui::Vec2::new(8.0, 8.0);
        style.spacing.window_margin = egui::Margin::symmetric(12, 8);
        style.spacing.button_padding = egui::Vec2::new(12.0, 6.0);
        style.text_styles = [
            (
                egui::TextStyle::Heading,
                egui::FontId::new(24.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Body,
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Monospace,
                egui::FontId::new(14.0, egui::FontFamily::Monospace),
            ),
            (
                egui::TextStyle::Button,
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
            ),
        ]
        .into();
        egui_ctx.set_style_of(egui::Theme::Dark, style);

        let viewport_id = ViewportId::from_hash_of(window.id());
        let scale_factor = window.scale_factor() as f32;

        let egui_state = EguiState::new(
            egui_ctx,
            viewport_id,
            &window,
            Some(scale_factor),
            None,
            None,
        );

        let egui_renderer =
            EguiRenderer::new(device, format, egui_wgpu::RendererOptions::default());

        Self {
            egui_state,
            egui_renderer,
            window,
        }
    }

    /// Forward window events to egui.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        let _ = self.egui_state.on_window_event(&self.window, event);
    }

    /// Begin the egui frame, returning the full output.
    pub fn begin_frame(
        &mut self,
        state: Arc<Mutex<AppState>>,
        bridge: &Bridge,
    ) -> egui::FullOutput {
        let input = self.egui_state.take_egui_input(&self.window);
        self.egui_state.egui_ctx().run_ui(input, |ctx| {
            let mut state_guard = state.lock();
            gui::ui(ctx, &mut *state_guard, bridge);
        })
    }

    /// Apply texture deltas to the GPU.
    pub fn update_textures(
        &mut self,
        device: &Device,
        queue: &Queue,
        textures_delta: &egui::TexturesDelta,
    ) {
        for (id, deltas) in &textures_delta.set {
            for delta in deltas {
                self.egui_renderer.update_texture(device, queue, *id, delta);
            }
        }
        for id in &textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }

    /// Tessellate the shapes and return the clipped primitives and screen descriptor.
    pub fn tessellate(
        &mut self,
        shapes: Vec<egui::epaint::ClippedShape>,
        pixels_per_point: f32,
    ) -> (Vec<ClippedPrimitive>, ScreenDescriptor) {
        let clipped_primitives = self
            .egui_state
            .egui_ctx()
            .tessellate(shapes, pixels_per_point);

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: self.window.inner_size().into(),
            pixels_per_point: self.egui_state.egui_ctx().pixels_per_point(),
        };

        (clipped_primitives, screen_descriptor)
    }

    /// Update the egui renderer's buffers.
    pub fn update_buffers(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        clipped_primitives: &[ClippedPrimitive],
        screen_descriptor: &ScreenDescriptor,
    ) {
        self.egui_renderer.update_buffers(
            device,
            queue,
            encoder,
            clipped_primitives,
            screen_descriptor,
        );
    }

    /// Render the egui overlay into the render pass.
    pub fn render(
        &mut self,
        pass: &mut wgpu::RenderPass<'static>,
        clipped_primitives: &[ClippedPrimitive],
        screen_descriptor: &ScreenDescriptor,
    ) {
        self.egui_renderer
            .render(pass, clipped_primitives, screen_descriptor);
    }

    /// Access to the egui context for any additional operations.
    pub fn egui_ctx(&self) -> &egui::Context {
        self.egui_state.egui_ctx()
    }
}
