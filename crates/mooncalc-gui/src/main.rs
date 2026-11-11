//! MoonCalc desktop application.

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native("MoonCalc", options, Box::new(|_cc| Ok(Box::new(App))))
}

struct App;

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("MoonCalc");
        });
    }
}
