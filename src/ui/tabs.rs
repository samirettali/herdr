use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};

use super::text::display_width_u16;
use super::widgets::panel_contrast_fg;
use crate::app::AppState;

const NEW_TAB_WIDTH: u16 = 3;
const TAB_SCROLL_BUTTON_WIDTH: u16 = 3;

#[derive(Debug, Clone, Default)]
pub(crate) struct TabBarView {
    pub scroll: usize,
    pub tab_hit_areas: Vec<Rect>,
    pub scroll_left_hit_area: Rect,
    pub scroll_right_hit_area: Rect,
    pub new_tab_hit_area: Rect,
}

fn tab_width(label: &str, config: &crate::config::TabBarConfig) -> u16 {
    display_width_u16(label)
        .saturating_add(config.label_padding.saturating_mul(2))
        .max(config.min_width)
}

/// Resolve one token against the tab, or `None` when it has no value — an
/// unnamed tab, a pane with no agent — so the label closes up instead of
/// leaving a gap.
fn resolved_token(
    token: &crate::config::TabBarToken,
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
) -> Option<String> {
    use crate::config::TabBarToken;

    let tab = ws.tabs.get(tab_idx)?;
    // Values come from the tab's focused pane: it is the one you would be
    // looking at if you switched to that tab.
    let terminal = || {
        tab.terminal_id(tab.layout.focused())
            .and_then(|id| terminals.get(id))
    };

    match token {
        TabBarToken::Index => Some((tab_idx + 1).to_string()),
        TabBarToken::Name => tab.custom_name.clone(),
        TabBarToken::Agent => terminal().and_then(|terminal| {
            terminal
                .effective_display_agent()
                .or_else(|| terminal.effective_agent_label().map(str::to_string))
        }),
        TabBarToken::TerminalTitle => terminal().and_then(|terminal| terminal.terminal_title.clone()),
        TabBarToken::TerminalTitleStripped => {
            terminal().and_then(|terminal| terminal.terminal_title_stripped())
        }
        TabBarToken::Custom(name) => terminal()
            .and_then(|terminal| terminal.metadata_tokens.values().get(name).cloned()),
        TabBarToken::Text(text) => Some(text.clone()),
    }
    .filter(|value| !value.is_empty())
}

pub(crate) fn tab_chrome_label(
    app: &AppState,
    ws: &crate::workspace::Workspace,
    tab_idx: usize,
) -> String {
    let resolved = app
        .tab_bar
        .label
        .iter()
        .map(|token| (token, resolved_token(token, ws, tab_idx, &app.terminals)))
        .collect::<Vec<_>>();

    // A literal is punctuation between two values, so it is dropped when it
    // would lead, trail, or sit next to another literal: an unnamed tab with
    // `["index", { text = ":" }, "name"]` reads `1`, not `1:`.
    let mut label = String::new();
    for (position, (token, value)) in resolved.iter().enumerate() {
        let Some(value) = value else { continue };
        if token.is_text() {
            let has_value_before = resolved[..position]
                .iter()
                .any(|(token, value)| !token.is_text() && value.is_some());
            let has_value_after = resolved[position + 1..]
                .iter()
                .any(|(token, value)| !token.is_text() && value.is_some());
            if !has_value_before || !has_value_after {
                continue;
            }
        }
        label.push_str(value);
    }

    // Zoom stays outside the label: it is pane state the user did not ask to
    // see, and hiding it would hide that the tab is showing one pane of several.
    if tab_zoomed(ws, tab_idx) {
        if label.is_empty() {
            return "Z".to_string();
        }
        return format!("{label} Z");
    }
    label
}

fn tab_zoomed(ws: &crate::workspace::Workspace, tab_idx: usize) -> bool {
    ws.tabs.get(tab_idx).is_some_and(|tab| tab.zoomed)
}

/// Every tab's label, in order. Computed once by the caller because laying the
/// row out needs the widths, and the widths need the labels.
pub(crate) fn tab_bar_labels(app: &AppState, ws: &crate::workspace::Workspace) -> Vec<String> {
    (0..ws.tabs.len())
        .map(|tab_idx| tab_chrome_label(app, ws, tab_idx))
        .collect()
}

fn layout_tab_hit_areas(
    labels: &[String],
    area: Rect,
    scroll: usize,
    config: &crate::config::TabBarConfig,
) -> Vec<Rect> {
    let mut rects = vec![Rect::default(); labels.len()];
    if area.width == 0 || area.height == 0 {
        return rects;
    }

    let mut x = area.x;
    let right = area.x + area.width;
    for (idx, rect) in rects.iter_mut().enumerate().skip(scroll) {
        if x >= right {
            break;
        }
        let desired = tab_width(&labels[idx], config);
        let remaining = right.saturating_sub(x);
        let width = desired.min(remaining).max(1);
        *rect = Rect::new(x, area.y, width, 1);
        x = x.saturating_add(width.saturating_add(config.gap));
    }
    rects
}

fn centered_tab_scroll(
    labels: &[String],
    active_tab: usize,
    area: Rect,
    config: &crate::config::TabBarConfig,
) -> usize {
    let mut best_scroll = active_tab;
    let mut best_distance = u16::MAX;
    let viewport_center = area.x.saturating_mul(2).saturating_add(area.width);

    for scroll in 0..=active_tab {
        let rects = layout_tab_hit_areas(labels, area, scroll, config);
        let Some(active_rect) = rects.get(active_tab).copied() else {
            continue;
        };
        if active_rect.width == 0 {
            continue;
        }

        let active_center = active_rect
            .x
            .saturating_mul(2)
            .saturating_add(active_rect.width);
        let distance = active_center.abs_diff(viewport_center);
        if distance <= best_distance {
            best_distance = distance;
            best_scroll = scroll;
        }
    }

    best_scroll
}

fn trailing_tab_controls_x(tab_hit_areas: &[Rect], fallback_x: u16) -> u16 {
    tab_hit_areas
        .iter()
        .rev()
        .find(|rect| rect.width > 0)
        .map(|rect| rect.x + rect.width)
        .unwrap_or(fallback_x)
}

fn max_tab_scroll(
    labels: &[String],
    area: Rect,
    config: &crate::config::TabBarConfig,
) -> usize {
    (0..labels.len())
        .find(|&scroll| {
            layout_tab_hit_areas(labels, area, scroll, config)
                .last()
                .is_some_and(|rect| rect.width > 0)
        })
        .unwrap_or(0)
}

pub(crate) fn compute_tab_bar_view(
    ws: &crate::workspace::Workspace,
    labels: &[String],
    area: Rect,
    current_scroll: usize,
    follow_active: bool,
    mouse_chrome: bool,
    config: &crate::config::TabBarConfig,
) -> TabBarView {
    if area.width == 0 || area.height == 0 {
        return TabBarView::default();
    }

    if !mouse_chrome {
        let max_scroll = max_tab_scroll(labels, area, config);
        let scroll = if follow_active {
            centered_tab_scroll(labels, ws.active_tab, area, config).min(max_scroll)
        } else {
            current_scroll.min(max_scroll)
        };
        return TabBarView {
            scroll,
            tab_hit_areas: layout_tab_hit_areas(labels, area, scroll, config),
            scroll_left_hit_area: Rect::default(),
            scroll_right_hit_area: Rect::default(),
            new_tab_hit_area: Rect::default(),
        };
    }

    let area_right = area.x + area.width;
    let all_tabs_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(NEW_TAB_WIDTH),
        area.height,
    );
    let all_tabs = layout_tab_hit_areas(labels, all_tabs_area, 0, config);
    let overflow = all_tabs.iter().any(|rect| rect.width == 0);
    if !overflow {
        let new_tab_x = trailing_tab_controls_x(&all_tabs, area.x);
        let new_tab_hit_area = Rect::new(
            new_tab_x,
            area.y,
            area_right.saturating_sub(new_tab_x).min(NEW_TAB_WIDTH),
            1,
        );
        return TabBarView {
            scroll: 0,
            tab_hit_areas: all_tabs,
            scroll_left_hit_area: Rect::default(),
            scroll_right_hit_area: Rect::default(),
            new_tab_hit_area,
        };
    }

    let left_hit_area = Rect::new(area.x, area.y, TAB_SCROLL_BUTTON_WIDTH.min(area.width), 1);
    let tab_area_x = left_hit_area.x + left_hit_area.width;
    let reserved_trailing_width = NEW_TAB_WIDTH.saturating_add(TAB_SCROLL_BUTTON_WIDTH);
    let tab_area_right = area_right.saturating_sub(reserved_trailing_width);
    let tab_area = Rect::new(
        tab_area_x,
        area.y,
        tab_area_right.saturating_sub(tab_area_x),
        area.height,
    );

    let max_scroll = max_tab_scroll(labels, tab_area, config);
    let scroll = if follow_active {
        centered_tab_scroll(labels, ws.active_tab, tab_area, config).min(max_scroll)
    } else {
        current_scroll.min(max_scroll)
    };
    let tab_hit_areas = layout_tab_hit_areas(labels, tab_area, scroll, config);
    let trailing_x = trailing_tab_controls_x(&tab_hit_areas, tab_area_x).min(tab_area_right);
    let right_hit_area = Rect::new(
        trailing_x,
        area.y,
        area_right
            .saturating_sub(trailing_x)
            .min(TAB_SCROLL_BUTTON_WIDTH),
        1,
    );
    let new_tab_x = right_hit_area.x + right_hit_area.width;
    let new_tab_hit_area = Rect::new(
        new_tab_x,
        area.y,
        area_right.saturating_sub(new_tab_x).min(NEW_TAB_WIDTH),
        1,
    );

    TabBarView {
        scroll,
        tab_hit_areas,
        scroll_left_hit_area: left_hit_area,
        scroll_right_hit_area: right_hit_area,
        new_tab_hit_area,
    }
}

fn tab_drop_indicator_x(
    app: &AppState,
    ws: &crate::workspace::Workspace,
    insert_idx: usize,
) -> Option<u16> {
    let mut visible_tabs = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .filter(|(_, rect)| rect.width > 0);
    let first_visible = visible_tabs.clone().next()?;
    let last_visible = visible_tabs.next_back().unwrap_or(first_visible);

    if insert_idx == 0 {
        return Some(if first_visible.0 == 0 {
            first_visible.1.x
        } else {
            app.view.tab_scroll_left_hit_area.x + app.view.tab_scroll_left_hit_area.width
        });
    }

    if let Some((_, rect)) = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .find(|(idx, rect)| *idx == insert_idx && rect.width > 0)
    {
        return Some(rect.x.saturating_sub(1));
    }

    if insert_idx >= ws.tabs.len() {
        return Some(if last_visible.0 + 1 >= ws.tabs.len() {
            last_visible.1.x + last_visible.1.width
        } else {
            app.view.tab_scroll_right_hit_area.x.saturating_sub(1)
        });
    }

    None
}

pub(super) fn render_tab_bar(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(active_ws_idx) = app.active else {
        return;
    };
    let Some(ws) = app.workspaces.get(active_ws_idx) else {
        return;
    };
    let p = &app.palette;

    frame.render_widget(
        Paragraph::new(" ".repeat(area.width as usize)).style(Style::default().bg(p.panel_bg)),
        area,
    );

    let first_visible_idx = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .find(|(_, rect)| rect.width > 0)
        .map(|(idx, _)| idx);
    let last_visible_idx = app
        .view
        .tab_hit_areas
        .iter()
        .enumerate()
        .rev()
        .find(|(_, rect)| rect.width > 0)
        .map(|(idx, _)| idx);
    let can_scroll_left = app.view.tab_scroll_left_hit_area.width > 0 && app.tab_scroll > 0;
    let can_scroll_right = app.view.tab_scroll_right_hit_area.width > 0
        && last_visible_idx.is_some_and(|idx| idx + 1 < ws.tabs.len());

    if app.mouse_capture && app.view.tab_scroll_left_hit_area.width > 0 {
        let style = if can_scroll_left {
            Style::default().fg(p.overlay1).bg(app.tab_inactive_bg())
        } else {
            Style::default()
                .fg(p.overlay0)
                .bg(app.tab_inactive_bg())
                .add_modifier(Modifier::DIM)
        };
        frame.render_widget(
            Paragraph::new(" < ").style(style),
            app.view.tab_scroll_left_hit_area,
        );
    }

    if app.mouse_capture && app.view.tab_scroll_right_hit_area.width > 0 {
        let style = if can_scroll_right {
            Style::default().fg(p.overlay1).bg(app.tab_inactive_bg())
        } else {
            Style::default()
                .fg(p.overlay0)
                .bg(app.tab_inactive_bg())
                .add_modifier(Modifier::DIM)
        };
        frame.render_widget(
            Paragraph::new(" > ").style(style),
            app.view.tab_scroll_right_hit_area,
        );
    }

    for (idx, tab) in ws.tabs.iter().enumerate() {
        let Some(rect) = app.view.tab_hit_areas.get(idx).copied() else {
            break;
        };
        if rect.width == 0 {
            continue;
        }
        let active = idx == ws.active_tab;
        let style = if active {
            let active_bg = app.tab_active_bg();
            let active_fg = app
                .tab_active_fg_override()
                .unwrap_or_else(|| panel_contrast_fg(p));
            let base = Style::default().fg(active_fg).bg(active_bg);
            if tab.is_auto_named() {
                base
            } else {
                base.add_modifier(Modifier::BOLD)
            }
        } else if tab.is_auto_named() {
            Style::default()
                .fg(app.tab_inactive_fg_override().unwrap_or(p.overlay0))
                .bg(app.tab_inactive_bg())
                .add_modifier(Modifier::DIM)
        } else {
            Style::default()
                .fg(app.tab_inactive_fg_override().unwrap_or(p.overlay1))
                .bg(app.tab_inactive_bg())
        };
        let width = rect.width as usize;
        let name = tab_chrome_label(app, ws, idx);
        // The leading pad is spelled out and the label left-aligned in what is
        // left, so a `min_width` wider than the label spends the extra columns
        // on the right rather than drifting the label out of centre.
        let pad = usize::from(app.tab_bar.label_padding).min(width);
        let text = format!(
            "{:pad$}{:width$}",
            "",
            name,
            pad = pad,
            width = width.saturating_sub(pad)
        );
        frame.render_widget(Paragraph::new(text).style(style), rect);
    }

    if let Some(crate::app::state::DragState {
        target:
            crate::app::state::DragTarget::TabReorder {
                ws_idx,
                insert_idx: Some(insert_idx),
                ..
            },
    }) = &app.drag
    {
        if *ws_idx == active_ws_idx {
            if let Some(x) = tab_drop_indicator_x(app, ws, *insert_idx) {
                frame.buffer_mut()[(x.min(area.x + area.width.saturating_sub(1)), area.y)]
                    .set_symbol("│")
                    .set_style(Style::default().fg(p.accent));
            }
        }
    }

    if app.mouse_capture && app.view.new_tab_hit_area.width > 0 {
        frame.render_widget(
            Paragraph::new(" + ").style(Style::default().fg(p.overlay1)),
            app.view.new_tab_hit_area,
        );
    }

    if first_visible_idx.is_some_and(|idx| idx > 0) {
        let x = if app.mouse_capture && app.view.tab_scroll_left_hit_area.width > 0 {
            app.view.tab_scroll_left_hit_area.x + app.view.tab_scroll_left_hit_area.width
        } else {
            area.x
        };
        if x < area.x + area.width {
            frame.buffer_mut()[(x, area.y)]
                .set_symbol("…")
                .set_style(Style::default().fg(p.overlay0));
        }
    }
    if last_visible_idx.is_some_and(|idx| idx + 1 < ws.tabs.len()) {
        let x = if app.mouse_capture && app.view.tab_scroll_right_hit_area.width > 0 {
            app.view.tab_scroll_right_hit_area.x.saturating_sub(1)
        } else {
            area.x + area.width.saturating_sub(1)
        };
        if x >= area.x && x < area.x + area.width {
            frame.buffer_mut()[(x, area.y)]
                .set_symbol("…")
                .set_style(Style::default().fg(p.overlay0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AppState;
    use crate::workspace::Workspace;
    use ratatui::{backend::TestBackend, Terminal};

    fn buffer_row_text(buffer: &ratatui::buffer::Buffer, area: Rect, row: u16) -> String {
        (area.x..area.x + area.width)
            .map(|x| buffer[(x, row)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn tab_bar_marks_zoomed_tabs_without_renaming_them() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].zoomed = true;
        let custom_tab = ws.test_add_tab(Some("test"));
        ws.tabs[custom_tab].zoomed = true;

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(
            &app.workspaces[0],
            &tab_bar_labels(&app, &app.workspaces[0]),
            app.view.tab_bar_rect,
            0,
            true,
            false,
            &app.tab_bar,
        );
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let row = buffer_row_text(terminal.backend().buffer(), app.view.tab_bar_rect, 0);
        assert!(row.contains(" 1 Z"), "tab row: {row:?}");
        assert!(row.contains(" test Z"), "tab row: {row:?}");
        assert_eq!(app.workspaces[0].tab_display_name(0).as_deref(), Some("1"));
        assert_eq!(
            app.workspaces[0].tab_display_name(custom_tab).as_deref(),
            Some("test")
        );
    }

    #[test]
    fn active_auto_named_tab_keeps_readable_weight() {
        let mut app = AppState::test_new();
        let ws = Workspace::test_new("test");

        app.workspaces = vec![ws];
        app.active = Some(0);
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(
            &app.workspaces[0],
            &tab_bar_labels(&app, &app.workspaces[0]),
            app.view.tab_bar_rect,
            0,
            true,
            false,
            &app.tab_bar,
        );
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let tab_rect = app.view.tab_hit_areas[0];
        let style = terminal.backend().buffer()[(tab_rect.x + 1, tab_rect.y)].style();

        assert_eq!(style.bg, Some(app.palette.accent));
        assert!(!style.add_modifier.contains(Modifier::DIM));
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    fn label_of(app: &AppState, ws: &Workspace, tab_idx: usize) -> String {
        tab_chrome_label(app, ws, tab_idx)
    }

    #[test]
    fn zoom_marker_counts_toward_tab_width() {
        let app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("abcdefgh".into());
        ws.tabs[0].zoomed = true;

        // "1 abcdefgh Z" plus the default padding on each side.
        assert_eq!(
            tab_width(
                &label_of(&app, &ws, 0),
                &crate::config::TabBarConfig::default()
            ),
            16
        );
    }

    #[test]
    fn tab_width_uses_display_width_for_cjk_labels() {
        let app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("提交 herdr 的反馈".into());

        assert_eq!(
            tab_width(
                &label_of(&app, &ws, 0),
                &crate::config::TabBarConfig::default()
            ),
            display_width_u16("1 提交 herdr 的反馈") + 4
        );
    }

    #[test]
    fn default_label_is_the_index_then_the_name() {
        let app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.test_add_tab(None);
        ws.tabs[1].set_custom_name("logs".into());

        // An unnamed tab keeps reading as just its number: the separator is
        // dropped because it would trail nothing.
        assert_eq!(label_of(&app, &ws, 0), "1");
        assert_eq!(label_of(&app, &ws, 1), "2 logs");
    }

    #[test]
    fn literal_tokens_only_survive_between_two_values() {
        let config: crate::config::Config = toml::from_str(
            r#"
[ui.tab_bar]
label = ["index", { text = ":" }, "name"]
"#,
        )
        .expect("tab label config");
        let mut app = AppState::test_new();
        app.tab_bar = config.ui.tab_bar;
        let mut ws = Workspace::test_new("test");
        ws.test_add_tab(None);
        ws.tabs[1].set_custom_name("logs".into());

        assert_eq!(label_of(&app, &ws, 0), "1");
        assert_eq!(label_of(&app, &ws, 1), "2:logs");
    }

    #[test]
    fn agent_and_custom_tokens_come_from_the_focused_pane() {
        let config: crate::config::Config = toml::from_str(
            r#"
[ui.tab_bar]
label = ["agent", { text = " " }, "$model"]
"#,
        )
        .unwrap();
        let mut app = AppState::test_new();
        app.tab_bar = config.ui.tab_bar;
        let ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        let terminal_id = ws.tabs[0].panes[&pane_id].attached_terminal_id.clone();
        let mut terminal =
            crate::terminal::TerminalState::new(terminal_id.clone(), "/tmp".into());
        terminal.detected_agent = Some(crate::detect::Agent::Claude);
        terminal.metadata_tokens.patch(
            std::collections::HashMap::from([("model".into(), Some("haiku".into()))]),
            None,
            std::time::Instant::now(),
        );
        app.terminals.insert(terminal_id, terminal);

        assert_eq!(label_of(&app, &ws, 0), "claude haiku");
    }

    #[test]
    fn rejects_oversized_and_unknown_tab_label_tokens() {
        for label in [
            r#"["index", { text = "far too long" }]"#,
            r#"["nope"]"#,
            r#"["index", "index", "index", "index", "index", "index", "index", "index", "index"]"#,
        ] {
            let input = format!("[ui.tab_bar]\nlabel = {label}\n");
            assert!(
                toml::from_str::<crate::config::Config>(&input).is_err(),
                "accepted {label}"
            );
        }
    }

    #[test]
    fn tab_bar_config_drives_padding_gap_and_minimum_width() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("one".into());
        let second = ws.test_add_tab(Some("two"));
        assert_eq!(second, 1);
        app.active = Some(0);
        app.workspaces = vec![ws];
        app.tab_bar = crate::config::TabBarConfig {
            // Just the name, so the widths under test are the label's own.
            label: vec![crate::config::TabBarToken::Name],
            label_padding: 1,
            gap: 3,
            min_width: 0,
        };
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(
            &app.workspaces[0],
            &tab_bar_labels(&app, &app.workspaces[0]),
            app.view.tab_bar_rect,
            0,
            true,
            false,
            &app.tab_bar,
        );
        app.view.tab_hit_areas = view.tab_hit_areas;

        // "one" plus one column of padding per side, no minimum width padding.
        assert_eq!(app.view.tab_hit_areas[0].width, 5);
        // Three blank columns between the two tabs.
        assert_eq!(app.view.tab_hit_areas[1].x, 8);

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        assert_eq!(
            buffer_row_text(terminal.backend().buffer(), app.view.tab_bar_rect, 0),
            " one     two"
        );
    }

    #[test]
    fn default_tab_bar_pads_labels_symmetrically() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("agents".into());
        app.active = Some(0);
        app.workspaces = vec![ws];
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(
            &app.workspaces[0],
            &tab_bar_labels(&app, &app.workspaces[0]),
            app.view.tab_bar_rect,
            0,
            true,
            false,
            &app.tab_bar,
        );
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let tab = app.view.tab_hit_areas[0];

        // The default label is the index then the name, so "1 agents".
        assert_eq!(tab.width, 12);
        assert_eq!(buffer_row_text(buffer, tab, 0), "  1 agents");
        for x in [tab.x, tab.x + 1, tab.x + 10, tab.x + 11] {
            assert_eq!(buffer[(x, tab.y)].symbol(), " ");
            assert_eq!(buffer[(x, tab.y)].style().bg, Some(app.palette.accent));
        }
    }

    #[test]
    fn tab_bar_renders_trailing_cjk_character() {
        let mut app = AppState::test_new();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("提交 herdr 的反馈".into());

        app.active = Some(0);
        app.workspaces = vec![ws];
        app.view.tab_bar_rect = Rect::new(0, 0, 30, 1);
        let view = compute_tab_bar_view(
            &app.workspaces[0],
            &tab_bar_labels(&app, &app.workspaces[0]),
            app.view.tab_bar_rect,
            0,
            true,
            false,
            &app.tab_bar,
        );
        app.view.tab_hit_areas = view.tab_hit_areas;

        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_tab_bar(&app, frame, app.view.tab_bar_rect))
            .unwrap();

        let row = buffer_row_text(terminal.backend().buffer(), app.view.tab_bar_rect, 0);
        assert!(row.contains('馈'), "tab row: {row:?}");
    }
}
