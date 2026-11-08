# editors

Editor integrations for `workout-lsp`. Each subdirectory is one editor.

| Directory | Editor | Purpose |
|---|---|---|
| [`nvim/`](nvim/README.md) | Neovim 0.10+ | Starts `workout-lsp` only on the log files and defines the `Workout*` highlight groups for the semantic tokens. |

The editor support itself is in the server (`src/lsp/`, see
[`../src/lsp/README.md`](../src/lsp/README.md)). A plugin here only
decides which files get the server and how its tokens are coloured.
Install the server with `cargo install --path .` from the crate root.
