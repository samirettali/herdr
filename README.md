# herdr, patched

A fork of [herdr](https://github.com/herdrdev/herdr) — the terminal workspace that keeps
several coding agents visible at once and tells you which one is waiting for you. Docs, install
and everything else: [herdr.dev](https://herdr.dev). This README covers only what this branch
changes.

## fork changes

One commit per feature, kept separate so each can be rebased or dropped on its own. Every
option below defaults to the upstream behaviour, so an unchanged `config.toml` renders exactly
like vanilla Herdr — except for the tab label, which gains the tab number.

![The patched UI: branch names right-aligned in the spaces list, the tab label of each agent
right-aligned in the agents panel, tabs labelled with their number and name, no border around
the pane area](assets/patched.png)

Four of the patches at once: the `spacer` token pushing the branch names and the agent tab
labels to the right edge, tab labels composed from tokens, tighter tab spacing, and
per-component colours. The screenshot also has upstream's `pane_outer_borders = false` on, so
the only line left is the sidebar divider.

### Per-component theme tokens

`[theme.custom]` upstream only exposes palette-wide tokens, so restyling one component drags
every other user of that token along. This adds background and foreground pairs for the
sidebar spaces, the agent panel and the tab bar, each falling back to the palette token that
component used before:

```toml
[theme.custom]
space_active_bg = "#000000"
space_active_fg = "#ffffff"
space_inactive_fg = "#888888"
space_selected_bg = "#000000"
space_selected_fg = "green"
agent_active_bg = "#000000"
agent_active_fg = "#ffffff"
agent_inactive_bg = "#000000"
agent_inactive_fg = "#888888"
tab_active_bg = "#000000"
tab_active_fg = "#ffffff"
tab_inactive_bg = "#000000"
tab_inactive_fg = "#888888"
sidebar_divider = "#333333"
```

Leaving `agent_inactive_bg` unset keeps those rows unpainted, the way they are upstream.
Auto-named tabs keep their own dimmer foreground unless `tab_inactive_fg` is set, which then
wins for both kinds of tab.

### A `spacer` sidebar token

Sidebar rows are already token lists. A `spacer` eats whatever width the other tokens of its
row left over, so everything after it renders flush right, one column of gutter in from the
edge to mirror the one on the left:

```toml
[ui.sidebar.spaces]
rows = [["state_icon", "workspace", "spacer", { token = "branch", dim = true }]]

[ui.sidebar.agents]
rows = [["state_icon", "workspace", "spacer", "tab"], ["agent"]]
```

Several spacers in one row split the slack evenly, which gives centring as well. In a sidebar
too narrow for the row, spacers collapse to nothing and the layout falls back to upstream
behaviour.

### Tab labels from tokens

A tab was labelled with its `custom_name` or, failing that, its number — there was no way to
show what it is actually running. The label is now composed the way sidebar rows are:

```toml
[ui.tab_bar]
label = ["index", { text = " " }, "name"]        # the default
# label = ["index", { text = ":" }, "agent"]
# label = ["agent", { text = " · " }, "terminal_title_stripped"]
```

Built-ins are `index`, `name`, `agent`, `terminal_title` and `terminal_title_stripped`; pane
metadata reported through `herdr pane report-metadata` is available as `$name`. Values are read
from the tab's **focused pane** — the one you would be looking at if you switched to that tab.

Two rules make the result predictable:

- **A token with no value is dropped**, so an unnamed tab or a pane with no agent closes the
  label up instead of leaving a gap.
- **There is no implicit separator.** `{ text = "…" }` puts one exactly where you want it, and a
  literal is dropped unless it sits between two tokens that resolved to something — an unnamed
  tab with `["index", { text = ":" }, "name"]` reads `1`, not `1:`.

The zoom marker stays appended outside the label: it is state about the pane rather than a
field of the tab, and losing it would hide that the tab is showing one pane out of several.

Note that the default gains the number on named tabs, where vanilla showed the name alone. And
with long labels `min_width` stops mattering while overflow becomes the normal case, so the
scroll arrows earn their keep.

### Tab row spacing

The tab row geometry was three hardcoded constants. It is now configurable:

```toml
[ui.tab_bar]
label_padding = 2   # blank columns on each side of a tab label
gap = 1             # blank columns between two tabs
min_width = 8       # smallest tab width, padding included; 0 lets short labels shrink
```

The defaults match upstream's hardcoded geometry, so the row looks the same until you change
one of them. `label_padding` sets the width a label reserves on each side; the label itself
stays centred in the tab, so whatever `min_width` adds on top is spread evenly.

### The sidebar can sit on the right

```toml
[ui]
sidebar_position = "right"
```

The divider, the collapse toggle and the column you drag to resize all follow the sidebar, so
on the right it grows leftwards and the toggle stays next to the panes rather than against the
screen edge. Both the renderer and the mouse hit test ask the same
`SidebarPositionConfig::divider_x`, so they cannot disagree about where the divider is.

### The prefix hint bar is optional

Prefix mode lasts a single keystroke, and while it is armed a one-line bar is drawn over the
bottom row of the pane area — the tab bar, when it sits at the bottom. So every prefix press
flashes a reminder of keys you already know over content you were reading:

```toml
[ui]
prefix_hint = false
```

With it off nothing is drawn at all, badge included, and prefix mode is visible only from the
next key not reaching the pane. The hint bars for copy, resize and navigate mode are untouched:
those modes persist rather than flashing, so their reminder still earns its row.

### A zoomed pane without its border

Upstream v0.8.2 ships the outer-border option this fork used to carry, as `[ui]
pane_outer_borders`, so that patch is gone. What remains is the zoom half of it.

A zoomed split pane keeps its border by default. To let its terminal reclaim the surrounding
row and columns while zoomed:

```toml
[ui]
hide_pane_borders_when_zoomed = true
```

This only affects the zoomed view. Unzooming restores the configured split borders.

### Re-running commands on restore

A cold restore gives every pane a bare shell in its saved directory. Agent panes are the
exception: Herdr saves their session id and re-runs the agent with its own resume flag. This
generalises that to any command you list:

```toml
[session]
restore_commands = ["nvim", "lazygit"]
```

When the session is saved, a pane whose foreground process matches one of those executables
records its argv, and the restore re-runs it through the same deferred resume path the agents
use. Matching is on the executable name, so a store path or any absolute path still matches a
bare `nvim`. The allowlist is deliberate: without it a restore would relaunch whatever happened
to be in the foreground, including an `ssh`, a `psql` or a half-finished destructive command.

This restarts the program, not its state. `nvim` reopens empty unless a session plugin such as
persistence.nvim restores it; `lazygit` needs nothing, since reopening it in the same repository
is the whole of its state.

### Pane keys a program can keep for itself

Bind the pane keys without a prefix and Herdr eats them everywhere, so `ctrl+h` never reaches
the editor that wants it for its own splits. This is the missing half of what
`vim-tmux-navigator` gets from tmux's `#{pane_current_command}` conditional:

```toml
[keys]
focus_pane_left = "ctrl+h"
focus_pane_down = "ctrl+j"
focus_pane_up = "ctrl+k"
focus_pane_right = "ctrl+l"
passthrough_commands = ["nvim"]
```

While one of those executables is the focused pane's foreground process, a prefix-less
`focus_pane_*` chord is forwarded to the pane instead of moving focus. The program navigates its
own splits and calls `herdr pane focus --direction left` when it hits its edge, which is one
keymap on the editor side:

```lua
for key, direction in pairs({ h = "left", j = "down", k = "up", l = "right" }) do
  vim.keymap.set("n", "<C-" .. key .. ">", function()
    local from = vim.api.nvim_get_current_win()
    vim.cmd.wincmd(key)
    if vim.api.nvim_get_current_win() == from then
      vim.system({ "herdr", "pane", "focus", "--direction", direction })
    end
  end)
end
```

Prefix bindings and every other action are untouched, so `prefix+h` still moves focus from
inside `nvim` and a direct `ctrl+alt+g` custom command still fires. Matching is on the executable
name, like `restore_commands`. Only the foreground process group leader is inspected — the
command the user typed — and the lookup is two process queries on the keystroke itself, so
nothing is cached and nothing goes stale after a `:sh` or a `Ctrl-Z`.
