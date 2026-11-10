//! The feed of `bench/feed/Feed.qml` written with egui alone, as an egui
//! user would write it: one immediate-mode pass that lays out and paints
//! the whole window each frame, with the list's rows built only for the
//! part scrolled into view (`ScrollArea::show_rows`). `ljos bench` runs it
//! beside the QML (`mise run bench:egui`) to measure what drawing only what
//! changed is worth against rebuilding the frame.
//!
//! `--still` leaves the spinner out, as `bench/feed-still.qml` does.

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, RichText, ScrollArea, Sense, UiBuilder, vec2,
};

const ROWS: usize = 1000;
const ROW_HEIGHT: f32 = 64.0;

const NAMES: [&str; 12] = [
    "Ada Lovelace",
    "Alan Turing",
    "Grace Hopper",
    "Edsger Dijkstra",
    "Barbara Liskov",
    "Donald Knuth",
    "Margaret Hamilton",
    "Ken Thompson",
    "Frances Allen",
    "John Backus",
    "Radia Perlman",
    "Niklaus Wirth",
];
const SUBJECTS: [&str; 10] = [
    "Quarterly report",
    "Lunch on Friday?",
    "Re: the build is red",
    "Design review notes",
    "Your invoice",
    "Trip photos",
    "Meeting moved to 3 pm",
    "Draft for comments",
    "Welcome aboard",
    "Release checklist",
];
const WORDS: [&str; 20] = [
    "the", "quick", "brown", "fox", "jumps", "over", "a", "lazy", "dog", "while", "light", "falls",
    "on", "northern", "hills", "and", "every", "small", "window", "glows",
];
const TAGS: [&str; 5] = ["work", "family", "travel", "finance", "later"];
const TAG_COLORS: [u32; 5] = [
    0x00db_eafe,
    0x00dc_fce7,
    0x00fe_f3c7,
    0x00fc_e7f3,
    0x00e5_e7eb,
];
const AVATAR_COLORS: [u32; 8] = [
    0x00ef_4444,
    0x00f9_7316,
    0x00ea_b308,
    0x0022_c55e,
    0x0014_b8a6,
    0x003b_82f6,
    0x008b_5cf6,
    0x00ec_4899,
];
const FOLDERS: [&str; 12] = [
    "Inbox", "Starred", "Sent", "Drafts", "Archive", "Spam", "Trash", "Work", "Family", "Travel",
    "Finance", "Later",
];

fn rgb(c: u32) -> Color32 {
    let [_, r, g, b] = c.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

fn preview(i: usize) -> String {
    (0..14)
        .map(|k| WORDS[(i * 7 + k * 3) % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

/// A five-pointed star, filled or outlined: egui's own fonts have no star
/// glyph, so it is painted, as an egui app paints its icons.
fn star(painter: &egui::Painter, centre: egui::Pos2, radius: f32, filled: bool) {
    let point = |k: usize, r: f32| {
        let angle = std::f32::consts::TAU * k as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
        centre + r * vec2(angle.cos(), angle.sin())
    };
    let outline: Vec<egui::Pos2> = (0..10)
        .map(|k| point(k, if k % 2 == 0 { radius } else { radius * 0.45 }))
        .collect();
    if filled {
        let colour = rgb(0x00f5_9e0b);
        // The tessellator fills convex shapes: the pentagon inside, then
        // each point on it.
        let inner: Vec<egui::Pos2> = (0..5).map(|k| outline[2 * k + 1]).collect();
        painter.add(egui::Shape::convex_polygon(
            inner,
            colour,
            egui::Stroke::NONE,
        ));
        for k in 0..5 {
            let tip = vec![
                outline[(2 * k + 9) % 10],
                outline[2 * k],
                outline[2 * k + 1],
            ];
            painter.add(egui::Shape::convex_polygon(tip, colour, egui::Stroke::NONE));
        }
    } else {
        painter.add(egui::Shape::closed_line(
            outline,
            egui::Stroke::new(1.5, rgb(0x009c_a3af)),
        ));
    }
}

fn initials(name: &str) -> String {
    let mut parts = name.split(' ');
    let first = parts.next().and_then(|p| p.chars().next());
    let last = parts.next_back().and_then(|p| p.chars().next());
    first.into_iter().chain(last).collect()
}

struct Feed {
    spinning: bool,
    folder: usize,
    search: String,
    checked: Vec<bool>,
    starred: Vec<bool>,
}

impl Feed {
    fn new(spinning: bool) -> Self {
        Self {
            spinning,
            folder: 0,
            search: String::new(),
            checked: (0..ROWS).map(|i| i % 3 == 0).collect(),
            starred: (0..ROWS).map(|i| i % 4 == 1).collect(),
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(FOLDERS[self.folder]).strong().size(16.0));
            ui.add_space(120.0 - ui.min_rect().width().min(120.0));
            for name in ["New", "Reply", "Archive"] {
                let _ = ui.button(name);
            }
            ui.add_space((600.0 - ui.min_rect().width()).max(8.0));
            ui.add_sized(
                [260.0, 32.0],
                egui::TextEdit::singleline(&mut self.search).hint_text("Search mail"),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if self.spinning {
                    ui.add(egui::Spinner::new().size(20.0));
                } else {
                    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    ui.painter().circle_stroke(
                        rect.center(),
                        8.5,
                        egui::Stroke::new(3.0, rgb(0x00d1_d5db)),
                    );
                }
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        for (i, name) in FOLDERS.iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            if self.folder == i {
                ui.painter().rect_filled(rect, 0.0, rgb(0x00db_eafe));
            }
            let text = ui.visuals().text_color();
            ui.painter().text(
                rect.left_center() + vec2(16.0, 0.0),
                Align2::LEFT_CENTER,
                *name,
                FontId::proportional(13.0),
                text,
            );
            ui.painter().text(
                rect.right_center() - vec2(16.0, 0.0),
                Align2::RIGHT_CENTER,
                ((i * 37 + 5) % 120).to_string(),
                FontId::proportional(13.0),
                rgb(0x006b_7280),
            );
            if response.clicked() {
                self.folder = i;
            }
        }
    }

    fn row(&mut self, ui: &mut egui::Ui, i: usize) {
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
        let background = if i % 2 == 1 {
            rgb(0x00fa_fafa)
        } else {
            Color32::WHITE
        };
        ui.painter().rect_filled(rect, 0.0, background);
        let name = NAMES[i % NAMES.len()];
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(8.0, 4.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.checkbox(&mut self.checked[i], "");
        let (avatar, _) = row.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
        row.painter().circle_filled(
            avatar.center(),
            20.0,
            rgb(AVATAR_COLORS[i % AVATAR_COLORS.len()]),
        );
        row.painter().text(
            avatar.center(),
            Align2::CENTER_CENTER,
            initials(name),
            FontId::proportional(14.0),
            Color32::WHITE,
        );
        row.add_space(12.0);
        row.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let on = self.starred[i];
            let (rect, response) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
            star(ui.painter(), rect.center(), 9.0, on);
            if response.clicked() {
                self.starred[i] = !on;
            }
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                ui.label(
                    RichText::new(format!("{}:{:02}", i % 12 + 1, i * 7 % 60))
                        .color(rgb(0x006b_7280)),
                );
                let tag = TAGS[i % TAGS.len()];
                egui::Frame::new()
                    .fill(rgb(TAG_COLORS[i % TAGS.len()]))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(8, 2))
                    .show(ui, |ui| ui.label(RichText::new(tag).size(11.0)));
            });
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(name).strong());
                ui.add(egui::Label::new(SUBJECTS[i % SUBJECTS.len()]).truncate());
                ui.add(egui::Label::new(RichText::new(preview(i)).size(11.0).weak()).truncate());
            });
        });
    }
}

impl eframe::App for Feed {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar")
            .exact_size(48.0)
            .resizable(false)
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::left("folders")
            .exact_size(200.0)
            .resizable(false)
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::WHITE))
            .show(ui, |ui| {
                // The rows' text is not selectable, as a QML Text is not, so
                // that a drag on a row flicks the list instead of selecting.
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = 0.0;
                // DECISION: a mouse drag scrolls the list, as a drag
                // flicks a QML ListView; egui scrolls by dragging only on
                // touch unless asked.
                ScrollArea::vertical()
                    .auto_shrink(false)
                    .scroll_source(egui::containers::scroll_area::ScrollSource::ALL)
                    .show_rows(ui, ROW_HEIGHT, ROWS, |ui, range| {
                        for i in range {
                            self.row(ui, i);
                        }
                    });
            });
    }
}

fn main() -> eframe::Result {
    let spinning = !std::env::args().any(|a| a == "--still");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 720.0])
            .with_title("Feed")
            .with_app_id("feed"),
        // The GL painter, as Ljós's shell paints with egui's.
        renderer: eframe::Renderer::Glow,
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "feed",
        options,
        Box::new(move |cc| {
            // QML's Basic style is light.
            cc.egui_ctx.set_theme(egui::Theme::Light);
            Ok(Box::new(Feed::new(spinning)))
        }),
    )
}
