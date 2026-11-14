//! Credits page: every bundled tool with its licence, the foundations Corvus stands on,
//! and a thank-you to MediCat.

use corvus_common::theme::{BORDER, TEXT, TEXT_DIM};
use corvus_common::Tool;
use egui::{RichText, ScrollArea, Ui};

pub fn show(ui: &mut Ui, tools: &[Tool]) {
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.add_space(4.0);
        ui.label(RichText::new("Credits").size(22.0).strong().color(TEXT));
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Corvus is a Ventoy-based technician drive. Everything on it was made by the \
                     people below; Corvus only puts it in one place.",
            )
            .color(TEXT_DIM),
        );
        ui.add_space(16.0);

        section(ui, "Tools");
        let mut categories: Vec<&str> = Vec::new();
        for t in tools {
            if !categories.contains(&t.category.as_str()) {
                categories.push(&t.category);
            }
        }
        for cat in categories {
            ui.add_space(8.0);
            ui.label(RichText::new(cat).size(14.0).strong().color(TEXT));
            ui.add_space(2.0);
            for t in tools.iter().filter(|t| t.category == cat) {
                tool_line(ui, t);
            }
        }

        ui.add_space(20.0);
        section(ui, "Foundation");
        entry(
            ui,
            "Ventoy",
            "GPL-3.0",
            "https://www.ventoy.net",
            "Boots every image on the drive.",
        );
        entry(
            ui,
            "egui / eframe",
            "MIT OR Apache-2.0",
            "https://github.com/emilk/egui",
            "The UI toolkit both Corvus programs are drawn with.",
        );
        entry(
            ui,
            "Rust",
            "MIT OR Apache-2.0",
            "https://www.rust-lang.org",
            "",
        );
        entry(
            ui,
            "Corvus",
            "MIT",
            "",
            "Launcher, drive builder and build tools by Aiden.",
        );

        ui.add_space(20.0);
        section(ui, "Thanks");
        ui.label(
            RichText::new(
                "MediCat USB, by Jayro and its contributors, carried this bench for years and \
                     set the bar Corvus is built to meet. Thank you.",
            )
            .color(TEXT),
        );
        ui.add_space(24.0);
    });
}

fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(16.0).strong().color(TEXT));
    let rect = ui.available_rect_before_wrap();
    let y = rect.top() + 4.0;
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(1.0, BORDER),
    );
    ui.add_space(10.0);
}

fn tool_line(ui: &mut Ui, t: &Tool) {
    let license = if t.license.is_empty() {
        "licence not recorded"
    } else {
        &t.license
    };
    entry(ui, &t.name, license, &t.source, &t.notes);
    if !t.version.is_empty() {
        // Version sits on its own dim line so names line up.
        ui.add_space(-2.0);
        ui.label(
            RichText::new(format!("    version {}", t.version))
                .size(11.0)
                .color(TEXT_DIM),
        );
    }
}

fn entry(ui: &mut Ui, name: &str, license: &str, source: &str, note: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(name).color(TEXT));
        ui.label(RichText::new("·").color(TEXT_DIM));
        ui.label(RichText::new(license).color(TEXT_DIM));
        if !source.is_empty() {
            ui.label(RichText::new("·").color(TEXT_DIM));
            ui.label(RichText::new(source).size(11.0).color(TEXT_DIM));
        }
    });
    if !note.is_empty() {
        ui.label(
            RichText::new(format!("    {note}"))
                .size(11.0)
                .color(TEXT_DIM),
        );
    }
    ui.add_space(2.0);
}
