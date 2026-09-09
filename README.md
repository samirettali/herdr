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
`Local`. A mapping renames them in the sidebar only, for example to one Nerd Font glyph each so
the rows line up:

```toml
[ui.sidebar.machines]
labels = { local = "", andromeda = "", work = "" }
```

Keys are the saved names and match ignoring case. The label also feeds the `machine` token of
the agent rows and the mobile switcher; the CLI, `herdr machine list` and status messages keep
the saved name.

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
