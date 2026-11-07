# workout.nvim

Neovim support for the plain-text workout log: it starts `workout-lsp`
on the log only, and defines one highlight group per semantic token
type the server sends. Plain Lua, Neovim 0.10+ (tested on 0.12), no
plugin manager needed.

The log stays a `.md` file (Obsidian), so it keeps the `markdown`
filetype. The plugin picks the log by path instead:

* the file names `Workout.md` and `workout.md`, in any directory
* `*.workout.md`
* every `*.md` file under `~/workout-log/` and its subdirectories (so a
  later `archive/2026-11.md` is included)

Other Markdown files are left alone. LSP semantic tokens are drawn at
priority 125, above Tree-sitter's Markdown highlights (100), so the
workout colours show on top.

## Install

Install the server first (from the crate root):

```sh
cargo install --path .        # puts workout-lsp in ~/.cargo/bin
```

Then add the plugin to the runtimepath, in `init.lua`:

```lua
vim.opt.rtp:append('/path/to/plain-text-workout/editors/nvim')
require('workout').setup()
```

or with lazy.nvim:

```lua
{
  dir = '/path/to/plain-text-workout/editors/nvim',
  name = 'workout',
  config = function()
    require('workout').setup({ dirs = { '~/workout-log' } })
  end,
}
```

`setup()` also attaches to log buffers that were already open, so lazy
loading (`event = 'VeryLazy'`) works.

## Options

Defaults shown:

```lua
require('workout').setup({
  cmd = { 'workout-lsp' },               -- how to start the server
  filenames = { 'Workout.md', 'workout.md' }, -- exact file names
  patterns = { '*.workout.md' },         -- globs: the file name, or the full path if they contain '/'
  dirs = { '~/workout-log' },            -- every *.md below these is a log
  lsp = true,                            -- false: only define the highlight groups
  inlay_hints = true,                    -- show "reps · kg" at the end of strength lines
  lsp_config = {},                       -- extra vim.lsp.start fields (capabilities, on_attach, ...)
  highlights = {},                       -- group overrides, see below
})
```

The lists (`cmd`, `filenames`, `patterns`, `dirs`) replace the defaults;
they are not merged.

The plugin uses `vim.lsp.start` from `BufReadPost`/`BufNewFile`, which
works on 0.10 and 0.11+ alike. One client is shared per root: the
configured directory for a file under `dirs`, else the enclosing `.git`
repository, else the file's directory.

Functions: `setup(opts)`, `attach(buf)` (start the server on a buffer
by hand), `is_workout_file(path)`, `apply_highlights()`.

## Highlight groups

Each server token type `workoutX` is drawn with `@lsp.type.workoutX` (and
`@lsp.type.workoutX.markdown`), which link to `WorkoutX`:

| Group | Token | Default |
|---|---|---|
| `WorkoutDate` | date `2026-11-07` | `Title` |
| `WorkoutDateBracket` | `[[` `]]` around a date | `Delimiter` |
| `WorkoutBodyWeight` | weigh-in on a date line | `Identifier` |
| `WorkoutTime` | `18h15` | `Special` |
| `WorkoutBullet` | `-` / `+` | `Delimiter` |
| `WorkoutSupersetLabel` | super-set label | `Label` |
| `WorkoutExercise` | exercise name | `Function` |
| `WorkoutMachine` | machine after `@` | `Type` |
| `WorkoutInitial` | `22` in `+22kg` | `Constant` |
| `WorkoutSides` | `x2` | `Operator` |
| `WorkoutReps` | repetitions | `Number` |
| `WorkoutRepeat` | count after `^` | `Number` |
| `WorkoutLoad` | load number | `Constant` |
| `WorkoutUnit` | `kg lbs km min ⁰ …` | `Operator` |
| `WorkoutBw` | `bw` | `Keyword` |
| `WorkoutOperator` | `x ^ ++ ~ @ …` | `Operator` |
| `WorkoutDistance` | cardio distance | `Number` |
| `WorkoutDuration` | cardio duration | `Number` |
| `WorkoutSpeed` | cardio speed | `Number` |
| `WorkoutRange` | cardio range | `Number` |
| `WorkoutComment` | `; …` | `Comment` |
| `WorkoutConflict` | sync conflict line | red `#ff5f5f` (cterm 203) |
| `WorkoutConflictDate` | date of a conflict line | orange `#ff8700` (cterm 208) |
| `WorkoutInvalid` | text the tokenizer cannot place | `DiagnosticUnderlineError` |

The conflict groups use fixed colours, not links, so a sync conflict
looks the same in every colour scheme. All defaults are set with
`default = true` and re-applied on every `ColorScheme` event.

## Changing a colour

Pass it to `setup`; these are re-applied after every colour scheme
change:

```lua
require('workout').setup({
  highlights = {
    WorkoutExercise = { fg = '#88c0d0', bold = true },
    WorkoutLoad = { link = 'String' },
  },
})
```

A colour scheme can define the `Workout*` groups itself; because the
plugin's defaults use `default = true`, the scheme's definitions win. A
plain `vim.api.nvim_set_hl(0, 'WorkoutExercise', {...})` also works, but
`:colorscheme` clears it, so put it in your own `ColorScheme` autocommand.

To check what applies under the cursor: `:Inspect`.
