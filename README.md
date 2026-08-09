# herdr


<p align="center">
  <img src="assets/logo.png" alt="herdr" width="100" />
</p>

<p align="center">
  <a href="https://herdr.dev">herdr.dev</a> · <a href="#install">install</a> · <a href="https://herdr.dev/docs/quick-start/">quick start</a> · <a href="https://herdr.dev/docs/">docs</a>
</p>

<p align="center">
  English · <a href="README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
  <a href="https://github.com/herdrdev/herdr/releases"><img src="https://img.shields.io/github/downloads/herdrdev/herdr/total?labelColor=333333&color=666666" alt="total GitHub release downloads" /></a>
  <a href="https://github.com/herdrdev/herdr/stargazers"><img src="https://img.shields.io/github/stars/herdrdev/herdr?labelColor=333333&color=666666&logo=github" alt="GitHub stars" /></a>
  <a href="https://github.com/herdrdev/herdr/releases/latest"><img src="https://img.shields.io/github/v/release/herdrdev/herdr?label=release&labelColor=333333&color=666666" alt="latest stable release" /></a>
  <a href="https://formulae.brew.sh/formula/herdr"><img src="https://img.shields.io/homebrew/v/herdr?label=homebrew&labelColor=333333&color=666666" alt="Homebrew version" /></a>
  <a href="https://x.com/herdrdev"><img src="https://img.shields.io/badge/follow-%40herdrdev-000000?logo=x&logoColor=white" alt="follow @herdrdev on X" /></a>
</p>

---

> **This is a fork.** Branch `patched` carries eight commits on top of the `v0.8.0`
> tag, described in [fork changes](#fork-changes). Everything else is upstream
> [herdrdev/herdr](https://github.com/herdrdev/herdr).

---

https://github.com/user-attachments/assets/043ec09f-4bdd-41d5-aee0-8fda6b83e267

**the runtime your coding agents live on.**

- **always running** — herdr is a background server; the terminals live inside it. close the lid, drop the network, or restart the machine; agents keep working and sessions come back. reattach from any terminal, or over ssh.
- **never hunt for the stuck one** — every pane is marked working, blocked, or idle. when an agent stops and needs an answer, herdr says so.
- **agent-native** — agents drive herdr through the cli and socket api: they can spawn panes, prompt each other, and wait until another agent is genuinely blocked. [agent skill →](https://herdr.dev/docs/agent-skill/)
- **runs what you already run** — claude code, codex, cursor, opencode, grok and the rest. herdr doesn't wrap or replace them; it owns their terminals.
- **keyboard and mouse, both first-class** — tmux-style prefix keys *and* click, drag, split. pick per moment, not per tool.
- **plugins** — extend panes and workflows. [browse the marketplace →](https://herdr.dev/plugins/)
- **one rust binary, no electron** — runs in whatever terminal you already use.

---

## fork changes

Eight commits on top of `v0.8.0`, one per feature, kept separate so each can be rebased or
dropped on its own. Every option below defaults to the upstream behaviour, so an unchanged
`config.toml` renders exactly like vanilla Herdr — except for the tab label padding, which
becomes symmetric (same total width, see below).

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

The padding default of 2 is the one intentional change of appearance: upstream spends one
column to the left of the label and three to the right, which puts the label visibly off
centre inside a coloured tab. The total width is unchanged.

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

### Dividers without the outer border

Herdr has no frame widget: what reads as a border around the pane area is the perimeter of the
per-pane boxes. With `pane_outer_border = false` every border edge that faces no other pane is
dropped, so only the dividers between panes survive and a row at the top and bottom, plus a
column on each side, go back to the terminal:

```toml
[ui]
pane_outer_border = false
```

It also drops every border title, agent labels and manual pane names alike. Titles live inside
a top border, and without the frame only some panes still have one, so keeping them would show
titles on an arbitrary subset of the splits.

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

## install

```bash
curl -fsSL https://herdr.dev/install.sh | sh
```

or `brew install herdr` · `mise use -g herdr` · windows beta: `powershell -ExecutionPolicy Bypass -c "irm https://herdr.dev/install.ps1 | iex"` · [binaries](https://github.com/herdrdev/herdr/releases)

then start it where the work lives:

```bash
herdr
```

run your agents, split panes, walk away. `ctrl+b q` detaches, `herdr` reattaches. [quick start →](https://herdr.dev/docs/quick-start/)

## docs

everything lives at [herdr.dev/docs](https://herdr.dev/docs/): [quick start](https://herdr.dev/docs/quick-start/) · [concepts](https://herdr.dev/docs/concepts/) · [supported agents](https://herdr.dev/docs/agents/) · [keyboard](https://herdr.dev/docs/keyboard/) · [configuration](https://herdr.dev/docs/configuration/) · [session state](https://herdr.dev/docs/session-state/) · [remote](https://herdr.dev/docs/persistence-remote/) · [integrations](https://herdr.dev/docs/integrations/) · [plugins](https://herdr.dev/docs/plugins/) · [socket api](https://herdr.dev/docs/socket-api/)

## thanks

every past sponsor and backer is listed in [SPONSORS.md](./SPONSORS.md) — thank you 🐑

enterprise / partnership: hey@herdr.dev

## agent instructions

if you are an ai agent helping with this repository, read [`AGENTS.md`](./AGENTS.md) before making changes and read [`CONTRIBUTING.md`](./CONTRIBUTING.md) before opening issues or PRs.

## development

```bash
git clone https://github.com/herdrdev/herdr
cd herdr
cargo build --release

just test        # unit tests
just check       # formatting, tests, and maintenance checks
```

## license

Herdr is licensed under the [Apache License 2.0](LICENSE).
