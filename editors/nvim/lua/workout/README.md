# editors/nvim/lua/workout

The Lua module of the Neovim plugin. `require('workout')` loads
`init.lua` from here once `editors/nvim` is on the runtimepath.

## Files

| File | Purpose |
|---|---|
| `init.lua` | The whole plugin. See the table below. |

## Contents of `init.lua`

| Item | Purpose |
|---|---|
| `M.kinds` | The token kind names in `TokenKind::ALL` order (`src/tokens.rs`). |
| `M.group(kind)` | `date_bracket` → `WorkoutDateBracket`, the highlight group. |
| `M.token_type(kind)` | `date_bracket` → `workoutDateBracket`, the LSP semantic token type. |
| `M.default_highlights` | The default of each group: a link to a standard group, or fixed colours for `WorkoutConflict` and `WorkoutConflictDate`. |
| `M.defaults`, `M.options` | The default options and the options in use after `setup`. |
| `M.is_workout_file(path)` | Whether a path is a log: an exact file name, a glob, or a `*.md` under one of `dirs`. For a match under `dirs`, it also returns that directory. |
| `M.apply_highlights()` | Defines every group with `default = true` (a group in `highlights` is set as given instead), and links `@lsp.type.workoutX` (and `.markdown`) to `WorkoutX`. |
| `M.attach(buf)` | Starts or reuses `workout-lsp` on a buffer that is a log, with `vim.lsp.start`. The root is the matching directory, else the enclosing `.git` repository, else the file's directory. Turns on inlay hints when `inlay_hints` is set. |
| `M.setup(opts)` | Merges the options (list options replace the defaults), applies the highlights, re-applies them on `ColorScheme`, and attaches on `BufReadPost` and `BufNewFile` and to buffers already open. |

## Relations

- `M.kinds` must match `TokenKind::ALL` in `src/tokens.rs`, and
  `M.token_type` must match the legend names from
  `src/lsp/analysis.rs` (`workout` + CamelCase). A new token kind needs a
  new entry here and a row in the highlight table of
  [`../../README.md`](../../README.md).
- The options and highlight groups are documented in
  [`editors/nvim/README.md`](../../README.md).
