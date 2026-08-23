//! Egui UI layout.

use super::{AppState, Backend, Bridge};
use hfm_core::pipeline::SeekDelta;

/// Draw the entire GUI overlay.
pub fn ui(ctx: &egui::Context, state: &mut AppState, bridge: &Bridge) {
    // Main control window
    egui::Window::new("🎛️ hfm‑player")
        .default_open(true)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("Media Controls");

            // --- File/Model selection ---
            ui.horizontal(|ui| {
                if ui.button("📂 Open Video").clicked() {
                    bridge.open_video_file();
                }
                ui.label(match &state.video_path {
                    Some(p) => p.file_name().unwrap_or_default().to_string_lossy(),
                    None => "No video loaded".into(),
                });
            });

            ui.horizontal(|ui| {
                if ui.button("🧠 Load Audio Model").clicked() {
                    bridge.open_audio_model();
                }
                ui.label(match &state.audio_model_path {
                    Some(p) => p.file_name().unwrap_or_default().to_string_lossy(),
                    None => "No model loaded".into(),
                });
            });

            // Backend dropdowns
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
                        ui.selectable_value(&mut state.video_backend, Backend::CoreML, "CoreML");
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
                        ui.selectable_value(&mut state.audio_backend, Backend::CoreML, "CoreML");
                    });
            });

            if ui.button("🔄 Restart Pipeline").clicked() {
                bridge.send(super::GuiCommand::RestartPipeline);
            }

            ui.separator();

            // --- Playback controls ---
            ui.horizontal(|ui| {
                ui.add_enabled_ui(state.can_play(), |ui| {
                    let label = if state.is_playing() {
                        "⏸ Pause"
                    } else {
                        "▶ Play"
                    };
                    if ui.button(label).clicked() {
                        bridge.send(super::GuiCommand::TogglePlayPause);
                    }
                });

                ui.add_enabled_ui(state.can_play(), |ui| {
                    if ui.button("⏪ -10s").clicked() {
                        bridge.send(super::GuiCommand::Seek(SeekDelta::Backward(10_000_000_000)));
                    }
                    if ui.button("⏩ +10s").clicked() {
                        bridge.send(super::GuiCommand::Seek(SeekDelta::Forward(10_000_000_000)));
                    }
                });
            });

            // --- Time display ---
            let current_secs = state.current_time_ns / 1_000_000_000;
            let total_secs = state.total_duration_ns / 1_000_000_000;
            ui.label(format!(
                "⏱️ {:02}:{:02} / {:02}:{:02}",
                current_secs / 60,
                current_secs % 60,
                total_secs / 60,
                total_secs % 60
            ));

            ui.separator();

            // --- Volume control ---
            ui.horizontal(|ui| {
                ui.label("🔊 Volume:");
                if ui.button("−").clicked() {
                    bridge.send(super::GuiCommand::VolumeDown(5));
                }
                ui.label(format!("{}%", state.volume.get()));
                if ui.button("+").clicked() {
                    bridge.send(super::GuiCommand::VolumeUp(5));
                }
            });

            ui.separator();

            // --- Log toggle ---
            if ui
                .button(if state.show_logs {
                    "📜 Hide Logs"
                } else {
                    "📜 Show Logs"
                })
                .clicked()
            {
                bridge.send(super::GuiCommand::ToggleLogs);
            }
        });

    // --- Log panel (separate window) ---
    if state.show_logs {
        egui::Window::new("📜 Logs")
            .default_open(true)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Log output (placeholder)");
                let log_text = state.log_lines.join("\n");
                egui::TextEdit::multiline(&mut log_text.as_str())
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(10)
                    .desired_width(f32::INFINITY)
                    .interactive(false)
                    .show(ui);
                if ui.button("Clear").clicked() {
                    state.log_lines.clear();
                }
            });
    }
}
