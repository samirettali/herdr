//! The tab rows of the `tree` sidebar layout: every workspace lists its tabs
//! under itself, each carrying the state of the agent it runs, and the agents
//! panel goes away.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use super::*;

pub(super) struct TabRow {
    pub(super) tab_id: String,
    pub(super) status: crate::api::schema::AgentStatus,
    pub(super) focused: bool,
    pub(super) rows: Vec<Vec<crate::ui::ResolvedToken>>,
}

/// The tab rows of one workspace, in tab order. The first agent of a tab
/// lends it the agent tokens; a tab without one only has the tab tokens.
pub(super) fn tab_rows(
    snapshot: &ClientShellSnapshot,
    workspace: &ClientShellWorkspace,
    config: &ClientShellConfig,
    machine: Option<&crate::config::MachineToken>,
) -> Vec<TabRow> {
    snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == workspace.workspace_id)
        .map(|tab| {
            let agent = snapshot
                .agents
                .iter()
                .find(|agent| agent.tab_id == tab.tab_id);
            let pane = agent.and_then(|agent| {
                snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == agent.pane_id)
            });
            let agent_label = agent.and_then(|agent| {
                agent
                    .display_agent
                    .as_deref()
                    .or(agent.name.as_deref())
                    .or(agent.agent.as_deref())
                    .or(agent.title.as_deref())
            });
            let labels = agent
                .map(|agent| {
                    agent
                        .state_labels
                        .iter()
                        .cloned()
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            let tokens = agent
                .map(|agent| agent.tokens.iter().cloned().collect::<HashMap<_, _>>())
                .unwrap_or_default();
            let state_text = labels
                .get(status_text(tab.agent_status))
                .map(String::as_str)
                .unwrap_or_else(|| super::agent_sidebar::sidebar_status_text(tab.agent_status));
            let canonical_agent = agent
                .and_then(|agent| agent.agent.as_deref())
                .and_then(crate::detect::parse_agent_label);
            let rows = crate::ui::sidebar_tab_rows(
                &config.tabs,
                crate::ui::AgentTokenContext {
                    machine: machine.map(|machine| machine.text.as_str()),
                    machine_icon: machine.is_some_and(|machine| machine.icon_only),
                    workspace: &workspace.label,
                    tab: Some(tab.label.as_str()),
                    pane: agent
                        .and_then(|agent| agent.title.as_deref())
                        .or_else(|| pane.and_then(|pane| pane.label.as_deref())),
                    agent_label,
                    terminal_title: agent.and_then(|agent| agent.terminal_title.as_deref()),
                    terminal_title_stripped: agent
                        .and_then(|agent| agent.terminal_title_stripped.as_deref()),
                    canonical_agent,
                    tokens: &tokens,
                },
                state_text,
            );
            TabRow {
                tab_id: tab.tab_id.clone(),
                status: tab.agent_status,
                focused: tab.focused,
                rows,
            }
        })
        .collect()
}

pub(super) fn tab_row_height(row: &TabRow) -> u16 {
    row.rows.len().max(1).min(u16::MAX as usize) as u16
}

/// Draws one tab row under its workspace. `area` is the rect the workspace
/// rows were drawn in, so the branch glyphs line up under the workspace
/// label, one level deeper for the tabs of a worktree child.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_tab_row(
    buffer: &mut Buffer,
    area: Rect,
    entry: &WorkspaceEntry,
    row: &TabRow,
    last: bool,
    endpoint_active: bool,
    config: &ClientShellConfig,
) {
    let palette = &config.palette;
    let focused = endpoint_active && row.focused;
    let trunk = if entry.indented {
        if entry.last_child {
            "      "
        } else {
            "   │  "
        }
    } else {
        "   "
    };
    let name_style = if focused {
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.subtext0)
    };
    let secondary = Style::default().fg(if focused {
        palette.mauve
    } else {
        palette.overlay0
    });
    let status_style = Style::default()
        .fg(status_color(row.status, palette))
        .add_modifier(if focused {
            Modifier::empty()
        } else {
            Modifier::DIM
        });
    let icon = (
        status_icon(row.status, config.status_indicators),
        Style::default().fg(status_color(row.status, palette)),
    );
    let rows = if row.rows.is_empty() {
        vec![vec![crate::ui::ResolvedToken {
            kind: crate::ui::ResolvedTokenKind::StateIcon,
            style: Default::default(),
        }]]
    } else {
        row.rows.clone()
    };
    for (index, tokens) in rows.iter().take(area.height as usize).enumerate() {
        let y = area.y + index as u16;
        let branch = if index > 0 {
            if last {
                "   "
            } else {
                "│  "
            }
        } else if last {
            "└─ "
        } else {
            "├─ "
        };
        let prefix = format!("{trunk}{branch}");
        let prefix_width = super::render::display_width(&prefix);
        let mut spans = vec![Span::styled(prefix, Style::default().fg(palette.overlay0))];
        // The tab is the subject of the row, so it takes the name style that
        // the workspace has in the agent rows.
        spans.extend(crate::ui::resolved_token_spans(
            tokens,
            icon,
            status_style,
            secondary,
            name_style,
            Style::default().fg(palette.overlay1),
            palette,
            area.width.saturating_sub(prefix_width + 1) as usize,
        ));
        Paragraph::new(Line::from(spans)).render(Rect::new(area.x, y, area.width, 1), buffer);
    }
    if focused {
        buffer.set_style(area, Style::default().bg(palette.active_row_bg));
    }
}
