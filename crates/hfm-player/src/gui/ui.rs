//! Egui UI layout.

use super::{AppMode, AppState, Backend, Bridge};
use egui::{Color32, Margin};
use hfm_core::pipeline::SeekDelta;

pub fn ui(ui: &mut egui::Ui, state: &mut AppState, bridge: &Bridge) {
    match state.mode {
        AppMode::Setup => {
            egui::CentralPanel::default().show(ui, |ui| {
                setup_ui(ui, state, bridge);
            });
        }
        AppMode::Playback => {
            // 1. Bottom panel – capture its response
            let res = egui::Panel::bottom("playback_controls")
                .resizable(false)
                .min_size(60.0)
                .frame(
                    egui::Frame::new()
                        .fill(Color32::BLACK)
                        .corner_radius(10.0)
                        .inner_margin(Margin::symmetric(10, 5)),
                )
                .show(ui, |ui| {
                    playback_ui(ui, state, bridge);
                });

            // Store the exact height in logical points
            state.bottom_panel_height = res.response.rect.height();

            // 2. Central panel (Video area)
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    // Render the loading card inside the central panel bounds
                    if state.is_loading {
                        ui.centered_and_justified(|ui| {
                            egui::Frame::window(ui.style())
                                .fill(egui::Color32::from_black_alpha(200))
                                .corner_radius(10.0)
                                .inner_margin(20.0)
                                .show(ui, |ui| {
                                    ui.heading("⏳ Loading video...");
                                    ui.add_space(10.0);
                                    ui.label("Preparing pipeline...");
                                });
                        });
                    }
                });
        }
    }
}

fn setup_ui(ui: &mut egui::Ui, state: &mut AppState, bridge: &Bridge) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            egui::Frame::new()
                .fill(ui.visuals().panel_fill)
                .corner_radius(10.0)
                .inner_margin(Margin::same(20))
                .show(ui, |ui| {
                    ui.heading("🎬 hfm-player Setup");
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        if ui.button("📂 Open Video").clicked() {
                            bridge.open_video_file();
                        }
                        ui.label(match &state.video_path {
                            Some(p) => p.file_name().unwrap_or_default().to_string_lossy(),
                            None => "No video selected (default will be used)".into(),
                        });
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut state.video_filter_enabled,
                            "🎨 Enable Video Filter (PPHumanSeg)",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut state.audio_processing_enabled,
                            "🎵 Enable Audio Processing (HT-Demucs)",
                        );
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label("Video Backend:");
                        egui::ComboBox::from_id_salt("video_backend")
                            .selected_text(format!("{:?}", state.video_backend))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut state.video_backend, Backend::Cpu, "CPU");
                                ui.selectable_value(
                                    &mut state.video_backend,
                                    Backend::DirectML,
                                    "DirectML",
                                );
                                ui.selectable_value(
                                    &mut state.video_backend,
                                    Backend::OpenVINO,
                                    "OpenVINO",
                                );
                                ui.selectable_value(
                                    &mut state.video_backend,
                                    Backend::CoreML,
                                    "CoreML",
                                );
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Audio Backend:");
                        egui::ComboBox::from_id_salt("audio_backend")
                            .selected_text(format!("{:?}", state.audio_backend))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut state.audio_backend, Backend::Cpu, "CPU");
                                ui.selectable_value(
                                    &mut state.audio_backend,
                                    Backend::DirectML,
                                    "DirectML",
                                );
                                ui.selectable_value(
                                    &mut state.audio_backend,
                                    Backend::OpenVINO,
                                    "OpenVINO",
                                );
                                ui.selectable_value(
                                    &mut state.audio_backend,
                                    Backend::CoreML,
                                    "CoreML",
                                );
                            });
                    });

                    ui.add_space(20.0);

                    if ui.button("▶ Confirm & Play").clicked() {
                        bridge.send(super::GuiCommand::ConfirmSetup);
                    }
                    ui.label("If no video is selected, a default video will be used.");

                    ui.add_space(10.0);
                    if let Some(last) = state.log_lines.last() {
                        ui.colored_label(Color32::RED, last);
                    }
                });
        });
    });
}

fn playback_ui(ui: &mut egui::Ui, state: &mut AppState, bridge: &Bridge) {
    ui.horizontal_centered(|ui| {
        let label = if state.is_playing() { "⏸" } else { "▶" };
        if ui.button(label).clicked() {
            bridge.send(super::GuiCommand::TogglePlayPause);
        }

        if ui.button("⏪").clicked() {
            bridge.send(super::GuiCommand::Seek(SeekDelta::Backward(10_000_000_000)));
        }
        if ui.button("⏩").clicked() {
            bridge.send(super::GuiCommand::Seek(SeekDelta::Forward(10_000_000_000)));
        }

        let current_secs = state.current_time_ns / 1_000_000_000;
        let total_secs = state.total_duration_ns / 1_000_000_000;
        ui.label(format!(
            "{:02}:{:02} / {:02}:{:02}",
            current_secs / 60,
            current_secs % 60,
            total_secs / 60,
            total_secs % 60
        ));

        ui.label("🔊");
        if ui.button("−").clicked() {
            bridge.send(super::GuiCommand::VolumeDown(5));
        }
        ui.label(format!("{}%", state.volume.get()));
        if ui.button("+").clicked() {
            bridge.send(super::GuiCommand::VolumeUp(5));
        }

        if ui.button("⚙️ Setup").clicked() {
            bridge.send(super::GuiCommand::BackToSetup);
        }
    });
}
