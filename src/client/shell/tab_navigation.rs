//! Tab navigation: the workspace navigation mode, over the tab rows of the
//! tree sidebar. Up and down move the selection across every tab of every
//! online machine in sidebar order, Enter focuses the selected tab.

use super::*;

/// A client-only selection. Snapshot identity prevents Enter from using a
/// reused tab ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TabNavigationTarget {
    pub(super) endpoint_id: ClientEndpointId,
    pub(super) tab_id: String,
    boot_id: String,
    generation: Option<u64>,
}

impl TabNavigationTarget {
    pub(super) fn matches(&self, endpoint_id: &ClientEndpointId, tab_id: &str) -> bool {
        &self.endpoint_id == endpoint_id && self.tab_id == tab_id
    }
}

impl ClientShellState {
    pub(super) fn focused_tab_navigation_target(&self) -> Option<TabNavigationTarget> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|entry| entry.endpoint_id == self.active_endpoint_id)?;
        let snapshot = endpoint.snapshot.as_deref()?;
        let tab_id = snapshot.focused_tab_id.as_deref()?;
        Some(TabNavigationTarget {
            endpoint_id: endpoint.endpoint_id.clone(),
            tab_id: tab_id.to_owned(),
            boot_id: snapshot.boot_id.clone(),
            generation: endpoint.snapshot_generation,
        })
    }

    pub(super) fn tab_navigation_target_valid(&self, target: &TabNavigationTarget) -> bool {
        self.endpoints.iter().any(|endpoint| {
            endpoint.endpoint_id == target.endpoint_id
                && endpoint.status == ClientEndpointStatus::Online
                && endpoint.snapshot_generation == target.generation
                && endpoint.snapshot.as_deref().is_some_and(|snapshot| {
                    snapshot.boot_id == target.boot_id
                        && snapshot.tabs.iter().any(|tab| tab.tab_id == target.tab_id)
                })
        })
    }

    /// Every tab in the order the tree sidebar lists them: machine by machine,
    /// workspace by workspace, skipping collapsed worktree groups the way the
    /// sidebar does.
    fn tab_navigation_targets(&self) -> Vec<TabNavigationTarget> {
        let empty_collapsed_groups = HashSet::new();
        let mut targets = Vec::new();
        for endpoint in &self.endpoints {
            if endpoint.status != ClientEndpointStatus::Online {
                continue;
            }
            let Some(snapshot) = endpoint.snapshot.as_deref() else {
                continue;
            };
            let collapsed_groups = self
                .collapsed_groups_for_endpoint(&endpoint.endpoint_id)
                .unwrap_or(&empty_collapsed_groups);
            for entry in render::workspace_entries(snapshot, collapsed_groups) {
                let workspace = &snapshot.workspaces[entry.index];
                targets.extend(
                    snapshot
                        .tabs
                        .iter()
                        .filter(|tab| tab.workspace_id == workspace.workspace_id)
                        .map(|tab| TabNavigationTarget {
                            endpoint_id: endpoint.endpoint_id.clone(),
                            tab_id: tab.tab_id.clone(),
                            boot_id: snapshot.boot_id.clone(),
                            generation: endpoint.snapshot_generation,
                        }),
                );
            }
        }
        targets
    }

    pub(super) fn move_navigate_tab(&mut self, delta: isize) {
        let mut targets = self.tab_navigation_targets();
        if targets.is_empty() {
            return;
        }
        let current = self
            .navigate_tab
            .as_ref()
            .and_then(|selected| targets.iter().position(|target| target == selected));
        let next = match current {
            Some(current) => (current as isize + delta).rem_euclid(targets.len() as isize) as usize,
            None if delta < 0 => targets.len() - 1,
            None => 0,
        };
        let target = targets.swap_remove(next);
        self.collapsed_endpoints.remove(&target.endpoint_id);
        self.navigate_tab = Some(target);
        self.reveal_navigation_workspace = true;
    }

    pub(super) fn accept_navigate_tab(&mut self, outcome: &mut ClientShellInput) {
        let Some(target) = self.navigate_tab.clone() else {
            self.mode = self.copy_or_terminal_mode();
            outcome.repaint = true;
            return;
        };
        if !self.tab_navigation_target_valid(&target) {
            self.receive_endpoint_unavailable(
                "Tab is no longer available; select a connected tab".into(),
            );
            outcome.repaint = true;
            return;
        }
        if self.focus_or_activate(
            target.endpoint_id,
            ClientEndpointFocusTarget::Tab(target.tab_id),
            outcome,
        ) {
            self.mode = ClientShellMode::Terminal;
            self.navigate_tab = None;
        }
        outcome.repaint = true;
    }

    pub(super) fn leave_navigate_tabs(&mut self) {
        self.mode = self.copy_or_terminal_mode();
        self.navigate_tab = None;
    }
}
