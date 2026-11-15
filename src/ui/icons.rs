//! Lucide line icons. One 2× atlas is uploaded per egui context and tinted at draw time.
//! Source: lucide-static 1.73.0 (ISC), see assets/lucide/LICENSE.
//! Each discriminant indexes the corresponding tile in the atlas.
use eframe::egui::{self, Color32, Painter, Rect, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Icon {
    Prev,       // chevron-left
    Next,       // chevron-right
    First,      // chevron-first
    Last,       // chevron-last
    ZoomOut,    // zoom-out
    ZoomIn,     // zoom-in
    Fit,        // maximize
    OpenUrl,    // external-link
    Copy,       // copy
    Details,    // panel-right
    File,       // file
    FolderOpen, // folder-open
    Save,       // save
    Download,   // download
    Upload,     // upload
    Settings,   // settings
    Search,     // search
    Edit,       // pencil
    Table,      // table
    Columns,    // columns-3
    Chart,      // chart-no-axes-combined
    Help,       // circle-help
    Plus,       // plus
    Trash,      // trash-2
    Undo,       // undo-2
    Redo,       // redo-2
    Refresh,    // refresh-cw
    Filter,     // list-filter
    ArrowUp,    // arrow-up
    ArrowDown,  // arrow-down
    ArrowLeft,  // arrow-left
    ArrowRight, // arrow-right
    Eye,        // eye
    Check,      // check
    Code,       // code
    Link,       // link
    Rows,       // rows-3
    Sort,       // arrow-down-up
    Sparkles,   // sparkles
    Close,      // x
    Clipboard,  // clipboard-paste
    Cut,        // scissors
    History,    // history
    Database,   // database
    Cloud,      // cloud
    Lock,       // lock-keyhole
    Book,       // book-open
    Bookmark,   // bookmark
    Palette,    // palette
    Split,      // panels-top-left
    Compare,    // git-compare-arrows
    Merge,      // git-merge
    Folder,     // folder
    Globe,      // globe
    Plug,       // plug
    Shuffle,    // shuffle
    Pivot,      // table-2
    Quality,    // list-checks
    Frequency,  // chart-column
    Timeline,   // chart-line
    Brush,      // brush-cleaning
    Hash,       // hash
    Type,       // type
    Tag,        // tag
    FileText,   // file-text
    Exit,       // log-out
    Hide,       // eye-off
    Freeze,     // snowflake
    Microphone, // mic
    Send,       // arrow-up-right
    Scan,       // scan-text
    Flag,       // flag
    Map,        // map
    Layers,     // layers
    Info,       // info
    Transform,  // repeat-2
    Workflow,   // workflow
    Calculator, // calculator
    Timer,      // timer
    List,       // list
    Pin,        // pin
}

fn atlas(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("octa-lucide-atlas");
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return texture;
    }
    let image = image::load_from_memory(include_bytes!("../../assets/lucide/menu.png"))
        .expect("bundled Lucide atlas")
        .into_rgba8();
    let texture = ctx.load_texture(
        "Lucide",
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            &image,
        ),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

fn uv(icon: Icon) -> Rect {
    let i = icon as usize;
    let tile = egui::vec2(1.0 / 10.0, 1.0 / 9.0);
    Rect::from_min_size(
        egui::pos2((i % 10) as f32 * tile.x, (i / 10) as f32 * tile.y),
        tile,
    )
}
pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    painter.image(atlas(painter.ctx()).id(), rect, uv(icon), color);
}
pub fn icon_image(ui: &egui::Ui, icon: Icon) -> egui::Image<'static> {
    egui::Image::new((atlas(ui.ctx()).id(), Vec2::splat(15.0)))
        .uv(uv(icon))
        .tint(ui.visuals().text_color())
}
/// A square toolbar button showing only an icon; `enabled` greys it like
/// a normal button. The hover text carries the label plus the key hint.
pub fn icon_button(ui: &mut egui::Ui, enabled: bool, icon: Icon, tip: impl Into<String>) -> bool {
    let h = crate::ui::control_row::control_height(ui);
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(h, h),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response = response.on_hover_text(tip.into());
    let visuals = if enabled {
        ui.style().interact(&response)
    } else {
        &ui.visuals().widgets.inactive
    };
    if enabled {
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    let fg = if enabled {
        visuals.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    paint_icon(ui.painter(), rect.shrink(h * 0.18), icon, fg);
    response.clicked()
}

/// A menu action with a font-independent, theme-coloured line icon.
/// The normal egui button retains its focus, keyboard and disabled semantics.
pub fn menu_button(icon: Icon, label: impl Into<egui::WidgetText>) -> MenuIconButton {
    MenuIconButton {
        icon,
        label: label.into(),
        frame: true,
    }
}

pub struct MenuIconButton {
    icon: Icon,
    label: egui::WidgetText,
    frame: bool,
}

impl MenuIconButton {
    pub fn frame(mut self, frame: bool) -> Self {
        self.frame = frame;
        self
    }
}

impl egui::Widget for MenuIconButton {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.add(
            egui::Button::image_and_text(icon_image(ui, self.icon), self.label).frame(self.frame),
        )
    }
}

/// Select a consistent action pictogram from the stable translation key.
pub fn action_icon(key: &str) -> Icon {
    let action = key.split_once('.').map_or(key, |(_, name)| name);
    match action {
        "open_url" | "open_in_browser" => Icon::OpenUrl,
        "copy_path" | "copy_name" | "copy_column_names" => Icon::Copy,
        "move_row_up" => Icon::ArrowUp,
        "move_row_down" => Icon::ArrowDown,
        "move_left" | "move_column_left" => Icon::ArrowLeft,
        "move_right" | "move_column_right" => Icon::ArrowRight,
        "open_api" => Icon::Plug,
        "save_password" | "read_only" => Icon::Lock,
        "recent_files" | "recent_databases" | "versions" | "cell_history" => Icon::History,
        "exit" => Icon::Exit,
        "cut" => Icon::Cut,
        "paste" => Icon::Clipboard,
        "pivot" | "transpose" => Icon::Pivot,
        "random_sample" => Icon::Shuffle,
        "quality" => Icon::Quality,
        "timeseries" => Icon::Timeline,
        "value_frequency" => Icon::Frequency,
        "cleanup" | "harmonise" => Icon::Brush,
        "about" => Icon::Info,
        "documentation" => Icon::Book,
        "describe" | "file_internals" | "schema_drift" => Icon::Scan,
        "assistant" | "chat" => Icon::Sparkles,
        "report" | "raw" => Icon::FileText,
        "open" if key == "report.open" => Icon::OpenUrl,
        _ => {
            const RULES: &[(&str, Icon)] = &[
                ("bookmark", Icon::Bookmark),
                ("unpin", Icon::Pin),
                ("pin_", Icon::Pin),
                ("freeze", Icon::Freeze),
                ("hide", Icon::Hide),
                ("read_only", Icon::Lock),
                ("compare", Icon::Compare),
                ("merge", Icon::Merge),
                ("join", Icon::Merge),
                ("lookup", Icon::Search),
                ("split", Icon::Split),
                ("cloud", Icon::Cloud),
                ("database", Icon::Database),
                ("db_", Icon::Database),
                ("cut", Icon::Cut),
                ("paste", Icon::Clipboard),
                ("delete", Icon::Trash),
                ("remove", Icon::Trash),
                ("clear", Icon::Trash),
                ("copy", Icon::Copy),
                ("duplicate", Icon::Copy),
                ("undo", Icon::Undo),
                ("redo", Icon::Redo),
                ("save", Icon::Save),
                ("export", Icon::Download),
                ("import", Icon::Upload),
                ("search", Icon::Search),
                ("find", Icon::Search),
                ("filter", Icon::Filter),
                ("sort", Icon::Sort),
                ("insert", Icon::Plus),
                ("add_", Icon::Plus),
                ("new", Icon::Plus),
                ("settings", Icon::Settings),
                ("chart", Icon::Chart),
                ("correlation", Icon::Chart),
                ("format", Icon::Palette),
                ("color", Icon::Palette),
                ("mark", Icon::Tag),
                ("rename", Icon::Edit),
                ("edit", Icon::Edit),
                ("hash", Icon::Hash),
                ("type", Icon::Type),
                ("column", Icon::Columns),
                ("row", Icon::Rows),
                ("sql", Icon::Code),
                ("parse", Icon::Code),
                ("open", Icon::FolderOpen),
                ("update", Icon::Refresh),
                ("reload", Icon::Refresh),
                ("view", Icon::Eye),
                ("show", Icon::Eye),
                ("convert", Icon::Transform),
                ("table", Icon::Table),
                ("recipe", Icon::Workflow),
                ("time", Icon::Timer),
                ("map", Icon::Map),
                ("report", Icon::Flag),
                ("analyse", Icon::Chart),
                ("calc", Icon::Calculator),
                ("close", Icon::Close),
            ];
            RULES
                .iter()
                .find(|(part, _)| action.contains(part))
                .map(|(_, icon)| *icon)
                .unwrap_or(if key.starts_with("analyse_menu.") {
                    Icon::Chart
                } else {
                    Icon::List
                })
        }
    }
}

pub fn action_button(key: &str) -> MenuIconButton {
    menu_button(action_icon(key), crate::i18n::t(key))
}

pub fn submenu_atoms(ui: &egui::Ui, key: &str) -> (egui::Image<'static>, String) {
    (icon_image(ui, action_icon(key)), crate::i18n::t(key))
}
