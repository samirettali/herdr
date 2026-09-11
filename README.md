# herdr, patched

A fork of [herdr](https://github.com/herdrdev/herdr), the terminal workspace that keeps several
coding agents visible at once and tells you which one is waiting for you. Docs, install and
everything else: [herdr.dev](https://herdr.dev). This README covers only what this branch
changes.

## fork changes

Branch `patched` tracks upstream `master` rather than a release tag, the same way the NUR
package does: the multi-machine work landed in 0.9.0 and its fixes keep arriving on `master`.
One commit per feature, kept separate so each can be rebased or dropped on its own. Every
option below defaults to the upstream behaviour, so an unchanged `config.toml` renders exactly
like vanilla Herdr.

Earlier versions of this fork carried more: tab labels from tokens, tab row spacing,
`sidebar_position`, `restore_commands`, sidebar space and agent colours. Those were dropped in
the rewrite for 0.9: upstream now styles every sidebar token inline
(`{ token = "workspace", fg = "#bbbbbb" }`), paints active rows with `active_row_bg`, and the
rest was not worth carrying across a client/server split that rewrote the whole TUI. The old
branch survives as `patched-0.8.2`.

### One tree instead of two panels

Upstream's sidebar is two panels, spaces above agents, and a session's tabs live in a third
place, the tab bar. Knowing where you are means reading all three. The `tree` layout folds
them into one: every workspace lists its tabs under it, each tab carries the state of the
agent it runs, and the agents panel goes away.

```toml
[ui.sidebar]
layout = "tree"   # panels (default) | tree

[ui.sidebar.tabs]
rows = [["state_icon", "tab", "spacer", "agent"]]
```

```
  ▾ 󰇄 mbp
  ● dotfiles                      main
    ├─ ● claude · Claude Code
    └─ · shell
  ○ sottocasa                     feat/booking
    └─ ○ codex
  ▾  andromeda
  ○ servers
    └─ · nvim
```

There is no `machines` header: the first row is the first machine, or the first workspace on a
lone one. Workspace labels sit under the machine's collapse marker rather than two columns
further in, the focused tab's background spans the whole row, gutter to gutter, and a remote
machine's connection signal ends on the same column as the tab rows instead of touching the
edge.

The footer buttons are optional in either layout:

```toml
[ui.sidebar]
new_button = false    # the " new" workspace button
menu_button = false   # the "menu" launcher
```

The global menu opens only from that launcher, so without it the menu and its update badge are
gone. Settings, help, reload and detach keep their keybindings; the release notes have no key
and live only in that menu. The collapse toggle in the corner stays either way.

Tab rows take the agent row vocabulary, `rows_by_agent` included, so a tab that runs an agent
renders exactly like its agents-panel row would. A plain tab only has `state_icon`,
`state_text`, `machine`, `workspace`, `tab` and `spacer`; the agent tokens drop out of its row
and its icon is the dim `·` of an unknown state. The focused tab gets the active row
background and the workspace row does not repeat it, since the tab already says which
workspace is current. Clicking a tab focuses it, on another machine too, and the agent keys
still walk the agents in order. The collapsed sidebar and the mobile switcher are untouched.

The branch glyphs are optional. `guides = "indent"` under `[ui.sidebar.tabs]` drops them and
sets each tab two columns in from its workspace label:

```
  ▾ 󰇄 mbp
  dotfiles                        main
    ● claude · Claude Code
    · shell
  sottocasa                       feat/booking
    ○ codex
```

With the tree open the tab bar repeats what the sidebar shows, so it can go:

```toml
[ui]
hide_tab_bar_with_tree_sidebar = true
```

It hides the tab row only while the expanded sidebar is in the `tree` layout and brings it
back the moment the sidebar is collapsed, so a hidden sidebar never leaves you without a tab
indicator. What goes with the row is what only the row offered: the `+` for a new tab by mouse,
dragging tabs to reorder them and the `tab_bar_right` segments. The keys are unaffected.

### Tab navigation

`prefix+w` walks the workspaces: a selection moves through the sidebar, Enter goes there. With
every tab in the sidebar the same mode wants to walk the tabs:

```toml
[keys]
tab_picker = "prefix+t"   # unset by default
```

It is the workspace navigation mode over the tab rows, not a popup. `navigate_workspace_up`
and `navigate_workspace_down` move the selection through every tab of every online machine
in sidebar order, wrapping at the ends and unfolding a collapsed machine as they pass; Enter
focuses the selected tab, switching machine first when it lives on another one; Esc, the prefix
or a mouse click leave the mode. The selection paints the tab row with `selection_bg`, as the
workspace one does. Nothing else is bound while it is open, so a stray key never reaches the
pane.

### A `spacer` sidebar token

Sidebar rows are already token lists. A `spacer` eats whatever width the other tokens of its
row left over, so everything after it renders flush right, one column of gutter in from the
edge to mirror the one on the left:

```toml
[ui.sidebar.spaces]
rows = [["state_icon", "workspace", "spacer", "branch", "git_status"]]

[ui.sidebar.agents]
rows = [["state_icon", "machine", "workspace", "spacer", "tab"], ["agent"]]
```

Several spacers in one row split the slack evenly, which gives centring as well. In a sidebar
too narrow for the row, spacers collapse to nothing and the layout falls back to upstream
behaviour. A row of nothing but spacers is dropped like a row whose tokens went missing.

### Git arrows before the branch

`git_status` sits one blank from its neighbour on either side, where upstream only kept it
tight after the branch. So `["git_status", "branch"]` reads `↑1 main` rather than
`↑1 · main`, and the order of the row is the whole option.

### Machine labels

Saved machines show under the name `herdr machine add` recorded, and the local one is always
`Local`. A mapping gives each an icon, typically one Nerd Font glyph so the rows line up, and a
display name, in the sidebar only:

```toml
[ui.sidebar.machines]
labels = { local = { icon = "", name = "mbp" }, andromeda = { icon = "" }, work = { icon = "" } }
agent_token = "icon"   # what the machine token of the agent rows shows: icon, name or both
```

Keys are the saved names and match ignoring case; either field may be left out, and a missing
name keeps the saved one. The machines panel and the mobile switcher show icon and name. The
`machine` token of the agent rows follows `agent_token`, and as a bare icon it is followed by a
blank rather than the dot separator, the way `state_icon` is. The CLI, `herdr machine list` and
status messages keep the saved name.

### A detach effect

Detaching drops you back to the shell prompt in one frame. Optionally the client plays an
animation over its last frame first:

```toml
[ui]
detach_effect = "matrix"   # none (default) | matrix | blackhole
detach_effect_ms = 800     # default 500
```

`matrix` grows film-style digital rain out of the characters on screen: every character becomes
the head of a stream of varying speed and tail length, white and flickering, with the rows
behind it fading from white into green; the code it drags behind stands still and only
occasionally mutates, the streams stall and surge rather than slide, and painted surfaces
nothing falls over fade to black on their own.

`blackhole` sends every character into a decaying orbit around the centre of the screen, in an
accretion disk seen almost edge-on and askew: radii shrink on a power curve while angles advance
at the Keplerian rate, so the disk spins up as it falls in, and the whole scene turns around the
centre on top of that. The far half of the disk is lensed up over the shadow into an arch with a
faint second image under it, the approaching side is Doppler-beamed brighter and the receding
side dimmer and redder, particles heat through the disk and redshift just above the horizon,
and the shadow grows as it feeds, ringed by a hot photon ring. Two seconds suit it better than
one.

Frames are generated from the frame already on screen and written through the same path as any
other frame, at 120 a second with late frames dropped rather than stretching the duration, so the
server never knows the effect exists and the effect always ends on time, on a black screen.

### Collapsing a machine hides its agents

Collapsing a machine folds its spaces but upstream keeps its agents in the agents panel. With

```toml
[ui.sidebar.machines]
hide_agents_when_collapsed = true
```

a collapsed machine disappears whole: its agents leave the panel and the previous/next agent
keys skip them, so a key never focuses something you cannot see. The mobile switcher has no
collapsing and is unaffected.

### Per-component theme colours

`[theme.custom]` upstream only exposes palette-wide tokens, so restyling the active tab drags
every other user of `accent` along, the focused pane border included. This adds colours for
the two components that share tokens most awkwardly, the pane borders and the tab bar. Each
falls back to the palette token that component used before, and each is also accepted in the
`light` and `dark` tables:

```toml
[theme.custom]
pane_active_border = "#858585"   # defaults to accent
pane_inactive_border = "#444444" # defaults to overlay0
tab_active_bg = "#000000"        # defaults to accent
tab_active_fg = "#ffffff"        # defaults to the panel contrast colour
tab_inactive_bg = "#000000"      # inactive tabs and scroll arrows, defaults to surface0
tab_inactive_fg = "#888888"      # defaults to overlay1, overlay0 for auto-named tabs
```

Auto-named tabs keep their own dimmer foreground unless `tab_inactive_fg` is set, which then
wins for both kinds of tab.

### The prefix hint bar is optional

Prefix mode lasts a single keystroke, and while it is armed a one-line bar is drawn over the
bottom row of the pane area, the tab bar when it sits at the bottom. So every prefix press
flashes a reminder of keys you already know over content you were reading:

```toml
[ui]
prefix_hint = false
```

With it off nothing is drawn at all, badge included, and prefix mode is visible only from the
next key not reaching the pane. The hint bars for copy, resize and navigate mode are untouched:
those modes persist rather than flashing, so their reminder still earns its row.

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
inside `nvim` and a direct `ctrl+alt+g` custom command still fires. Matching is on the
executable name, so a store path or any absolute path still matches a bare `nvim`.

Since 0.9 the TUI is a client of a server that owns the panes, so the client cannot inspect
the pane's processes itself. The server reports the foreground process group leader's
executable as `foreground_process` on the pane info, next to `foreground_cwd`, and the client
shell snapshot carries it. Only the leader is inspected, on each snapshot: it is the command
the user typed, and nothing cached goes stale after a `:sh` or a `Ctrl-Z`. The snapshot is
bincode, so the new field bumps the protocol version: a fork client and a vanilla server of
the same version refuse each other rather than misread the stream.

## install

Through Nix, override the source of the upstream package with this branch; the fork leaves
`Cargo.lock` alone, so the vendored dependency hash still matches. Through Homebrew:

```bash
brew install samirettali/tap/herdr
```

The formula builds from source at a pinned revision of this branch.
