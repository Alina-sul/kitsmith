//! +Drive view: the global-sample perspective Rytmdash can honestly show
//! today — every +Drive file the loaded project slots resolve to, grouped by
//! folder. The full +Drive browser (tree walk, uploads-here, file ops) arrives
//! with the file-protocol milestone; until then this view does not fake it.

use std::collections::BTreeMap;

use eframe::egui::{self, RichText, ScrollArea};

use crate::gui::app::page_title;
use crate::gui::{theme, widgets};
use crate::model::ProjectModel;

pub fn show(ui: &mut egui::Ui, model: Option<&ProjectModel>) {
    page_title(ui, "+Drive");
    ui.label(
        RichText::new(
            "Global sample storage shared by all projects. Shown here: the +Drive files the \
             active project's loaded sample slots point at.",
        )
        .color(theme::TEXT_SECONDARY),
    );
    ui.add_space(theme::GAP_S);
    ui.label(
        RichText::new(
            "Full +Drive browsing (folder tree, search, uploads) arrives with the file-protocol \
             browser milestone.",
        )
        .color(theme::TEXT_MUTED)
        .size(theme::FONT_META),
    );
    ui.add_space(theme::GAP_L);

    let Some(model) = model.filter(|m| !m.samples.is_empty()) else {
        ui.label(
            RichText::new("No resolved +Drive files yet — sync with the device connected.")
                .color(theme::TEXT_MUTED),
        );
        return;
    };

    // Group known files by their +Drive folder.
    let mut folders: BTreeMap<String, Vec<&crate::model::SampleSlotInfo>> = BTreeMap::new();
    for sample in &model.samples {
        let folder = sample
            .path
            .rsplit_once('/')
            .map_or("/", |(dir, _)| if dir.is_empty() { "/" } else { dir })
            .to_owned();
        folders.entry(folder).or_default().push(sample);
    }

    ScrollArea::vertical().show(ui, |ui| {
        for (folder, files) in &folders {
            widgets::section_label(ui, folder);
            ui.add_space(2.0);
            for file in files {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("~").color(theme::SAMPLE_PURPLE));
                    ui.label(RichText::new(&file.name).color(theme::TEXT_PRIMARY));
                    ui.label(
                        RichText::new(format!("{} KB", file.size / 1024))
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                    widgets::badge(ui, "IN PROJECT", theme::SAMPLE_PURPLE);
                    ui.label(
                        RichText::new(format!("slot {:03}", file.slot))
                            .color(theme::TEXT_MUTED)
                            .size(theme::FONT_META),
                    );
                });
            }
            ui.add_space(theme::GAP_M);
        }
    });
}
