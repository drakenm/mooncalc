//! The application: owns the persisted [`Settings`], the transient
//! [`UiState`] and the [`Tabs`], and lays out the panels each frame.

use std::time::Duration;

use crate::panels::{inputs, menu, status_bar};
use crate::settings::{SETTINGS_KEY, Settings, ThemeChoice};
use crate::status::StatusInfo;
use crate::tabs::{Tab, TabContext, Tabs};

/// Repaint interval while a background job is running (progress updates).
const JOB_REPAINT_INTERVAL: Duration = Duration::from_millis(100);

/// Transient UI state: never persisted.
#[derive(Debug, Default)]
pub struct UiState {
    pub status: StatusInfo,
    pub inputs: inputs::InputsState,
    /// Theme last applied to the egui context.
    applied_theme: Option<ThemeChoice>,
}

/// The eframe application.
pub struct MoonCalcApp {
    pub settings: Settings,
    pub ui_state: UiState,
    pub tabs: Tabs,
}

impl MoonCalcApp {
    /// Create the app, restoring settings from eframe storage if present.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = cc
            .storage
            .and_then(|s| eframe::get_value::<Settings>(s, SETTINGS_KEY))
            .unwrap_or_default()
            .on_startup();
        Self::with_settings(settings)
    }

    /// Create the app with the given settings (no storage).
    pub fn with_settings(settings: Settings) -> Self {
        Self {
            settings,
            ui_state: UiState {
                inputs: inputs::InputsState::new(),
                ..UiState::default()
            },
            tabs: Tabs::new(),
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        let theme = self.settings.view.theme;
        if self.ui_state.applied_theme != Some(theme) {
            ctx.set_theme(theme.to_egui());
            self.ui_state.applied_theme = Some(theme);
        }
    }
}

impl eframe::App for MoonCalcApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        if self.ui_state.status.begin_frame() {
            ctx.request_repaint();
        }

        let Self {
            settings,
            ui_state,
            tabs,
        } = self;

        tabs.poll(
            &ctx,
            &mut TabContext {
                settings,
                status: &mut ui_state.status,
            },
        );

        egui::Panel::top("menu_bar").show(ui, |ui| {
            menu::menu_bar(ui, settings, &mut ui_state.status);
        });

        if settings.view.show_status_bar {
            egui::Panel::bottom("status_bar").show(ui, |ui| {
                status_bar::status_bar(ui, settings, &ui_state.status);
            });
        }

        if settings.view.show_inputs_panel {
            egui::Panel::left("inputs_panel")
                .resizable(true)
                .default_size(300.0)
                .min_size(240.0)
                .show(ui, |ui| {
                    inputs::inputs_panel(ui, settings, &mut ui_state.inputs);
                });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for tab in Tab::ALL {
                    ui.selectable_value(&mut settings.view.active_tab, tab, tab.title());
                }
            });
            ui.separator();
            let active = settings.view.active_tab;
            tabs.ui(
                active,
                ui,
                &mut TabContext {
                    settings,
                    status: &mut ui_state.status,
                },
            );
        });

        if !ui_state.status.jobs().is_empty() {
            ctx.request_repaint_after(JOB_REPAINT_INTERVAL);
        }
        if ui_state.status.message().is_some() {
            ctx.request_repaint_after(crate::status::MESSAGE_DURATION);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}
