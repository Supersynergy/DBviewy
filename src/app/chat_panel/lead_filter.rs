//! Local navigation for the lead catalogue. The source workbook partitions the
//! records by review outcome; a text search for "offen" would omit HTTP failures.
use std::sync::LazyLock;

use eframe::egui;
use regex::Regex;

use crate::app::chat::types::{ContentBlock, Message};
use crate::app::state::OctaApp;

static OPEN_IN_CITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?ix)^\s*(?:bitte\s+)?(?:zeig(?:e)?(?:\s+mir)?|filter(?:e)?(?:\s+nach)?)\s+(?:bitte\s+)?(?:alle[n]?\s+|die\s+)?offene[n]?\s+impressums[\s-]*prüfungen\s+in\s+([\p{L}][\p{L}\s-]*?)(?:\s+bitte)?[.!?]*\s*$"
).expect("lead filter grammar")
});

impl OctaApp {
    pub(super) fn lead_filter_target(&self, question: &str) -> Option<(usize, usize, String)> {
        if !self.chat.mode.runs_tools() || self.ask_filter_job.is_some() {
            return None;
        }
        let captures = OPEN_IN_CITY.captures(question)?;
        let city = captures.get(1)?.as_str().trim();
        let active = self.tabs.get(self.active_tab)?;
        let path = active.table.source_path.as_ref()?;
        let mut siblings = self.tabs.iter().enumerate().filter(|(_, tab)| {
            tab.table.source_path.as_ref() == Some(path)
                && tab.sheet_name.as_deref() == Some("Namensprüfung offen")
        });
        let (idx, tab) = if active.sheet_name.as_deref() == Some("Namensprüfung offen") {
            (self.active_tab, active)
        } else {
            let candidate = siblings.next()?;
            if siblings.next().is_some() {
                return None;
            }
            candidate
        };
        let col = tab.table.columns.iter().position(|c| c.name == "Ort")?;
        if !["Domain", "Prüfstatus Name"]
            .iter()
            .all(|key| tab.table.columns.iter().any(|c| c.name == *key))
        {
            return None;
        }
        // Only accept multi-word city names present in the source, so trailing
        // instructions such as "Berlin nach Priorität gruppiert" stay a draft.
        if city.split_whitespace().count() > 1 {
            let wanted = city.to_lowercase();
            if !tab.table.rows.iter().any(|row| {
                matches!(row.get(col),
                Some(octa::data::CellValue::String(name)) if name.to_lowercase() == wanted)
            }) {
                return None;
            }
        }
        Some((idx, col, city.to_owned()))
    }

    pub(crate) fn apply_lead_chat_filter(&mut self, ctx: &egui::Context) -> bool {
        let Some((idx, col, city)) = self.lead_filter_target(&self.chat.input) else {
            return false;
        };
        self.active_tab = idx;
        if self.tabs[idx].mark_filter_active {
            self.toggle_filter_to_marked();
        }
        let tab = &mut self.tabs[idx];
        tab.search_text.clear();
        tab.search_nav.reset();
        tab.column_filters.clear();
        tab.predicate_filters = vec![octa::data::predicate_filter::PredicateFilter {
            col,
            op: octa::data::conditional_format::CondOp::Eq,
            value: city.clone(),
            case_sensitive: false,
        }];
        tab.view_mode = octa::data::ViewMode::Table;
        tab.table_state.scroll_to_row(0);
        tab.filter_dirty = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(tab.title_display()));
        self.recompute_filter();
        let count = self.tabs[idx].filtered_rows.len();
        let response = format!("{count} Treffer im Blatt „Namensprüfung offen“ · Ort = {city}");
        self.chat.plan_offer = false;
        let mut session = self.chat.session.lock().unwrap();
        session.error = None;
        session.messages.push(Message::user_text(
            std::mem::take(&mut self.chat.input).trim(),
        ));
        session
            .messages
            .push(Message::assistant(vec![ContentBlock::text(&response)]));
        session.refresh_auto_title();
        self.status_message = Some((response, std::time::Instant::now()));
        true
    }
}
