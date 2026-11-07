-- workout.nvim: highlight groups for workout-lsp semantic tokens, and
-- attaching workout-lsp only to the workout log (which stays a `.md` file
-- for Obsidian, so it cannot get a filetype of its own).
--
-- Plain Lua, Neovim 0.10+. See editors/nvim/README.md.

local M = {}

-- TokenKind::ALL order (src/tokens.rs); the LSP legend type is
-- `workout` + CamelCase, the highlight group `Workout` + CamelCase.
M.kinds = {
  'date', 'date_bracket', 'body_weight', 'time', 'bullet', 'superset_label',
  'exercise', 'machine', 'initial', 'sides', 'reps', 'repeat', 'load', 'unit',
  'bw', 'operator', 'distance', 'duration', 'speed', 'range', 'comment',
  'conflict', 'conflict_date', 'invalid',
}

local function camel(name)
  return (name:gsub('^%l', string.upper):gsub('_(%l)', string.upper))
end

--- `date_bracket` -> `WorkoutDateBracket`
function M.group(kind)
  return 'Workout' .. camel(kind)
end

--- `date_bracket` -> `workoutDateBracket` (the semantic token type)
function M.token_type(kind)
  return 'workout' .. camel(kind)
end

-- Default highlight of each group. Links follow the colour scheme; the
-- conflict groups have fixed colours so a sync conflict looks the same
-- (red, its date orange) in every colour scheme.
M.default_highlights = {
  WorkoutDate = { link = 'Title' },
  WorkoutDateBracket = { link = 'Delimiter' },
  WorkoutBodyWeight = { link = 'Identifier' },
  WorkoutTime = { link = 'Special' },
  WorkoutBullet = { link = 'Delimiter' },
  WorkoutSupersetLabel = { link = 'Label' },
  WorkoutExercise = { link = 'Function' },
  WorkoutMachine = { link = 'Type' },
  WorkoutInitial = { link = 'Constant' },
  WorkoutSides = { link = 'Operator' },
  WorkoutReps = { link = 'Number' },
  WorkoutRepeat = { link = 'Number' },
  WorkoutLoad = { link = 'Constant' },
  WorkoutUnit = { link = 'Operator' },
  WorkoutBw = { link = 'Keyword' },
  WorkoutOperator = { link = 'Operator' },
  WorkoutDistance = { link = 'Number' },
  WorkoutDuration = { link = 'Number' },
  WorkoutSpeed = { link = 'Number' },
  WorkoutRange = { link = 'Number' },
  WorkoutComment = { link = 'Comment' },
  WorkoutConflict = { fg = '#ff5f5f', ctermfg = 203 },
  WorkoutConflictDate = { fg = '#ff8700', ctermfg = 208 },
  WorkoutInvalid = { link = 'DiagnosticUnderlineError' },
}

M.defaults = {
  -- command that starts the server
  cmd = { 'workout-lsp' },
  -- exact file names (any directory) that are workout logs
  filenames = { 'Workout.md', 'workout.md' },
  -- globs matched against the file name (or the full path if they
  -- contain a `/`)
  patterns = { '*.workout.md' },
  -- every `*.md` below these directories is a log (e.g. a future
  -- `archive/2026-11.md`)
  dirs = { '~/workout-log' },
  -- start workout-lsp on matching buffers
  lsp = true,
  -- turn on inlay hints (reps · volume) in attached buffers
  inlay_hints = true,
  -- extra fields for vim.lsp.start (capabilities, on_attach, ...)
  lsp_config = {},
  -- highlight overrides by group name, e.g.
  -- { WorkoutExercise = { fg = '#88c0d0', bold = true } }; these replace
  -- the defaults and are re-applied on every colour scheme change
  highlights = {},
}

M.options = vim.deepcopy(M.defaults)

local function normalize(path)
  return vim.fs.normalize(vim.fn.fnamemodify(vim.fn.expand(path), ':p'))
end

--- True if `path` is a workout log under the current options.
function M.is_workout_file(path)
  if not path or path == '' then
    return false
  end
  local full = normalize(path)
  local base = vim.fs.basename(full)
  for _, name in ipairs(M.options.filenames) do
    if base == name then
      return true
    end
  end
  for _, glob in ipairs(M.options.patterns) do
    local subject = glob:find('/', 1, true) and full or base
    if vim.fn.match(subject, vim.fn.glob2regpat(vim.fn.expand(glob))) ~= -1 then
      return true
    end
  end
  if full:sub(-3) == '.md' then
    for _, dir in ipairs(M.options.dirs) do
      local d = normalize(dir):gsub('/$', '') .. '/'
      if full:sub(1, #d) == d then
        return true, d:sub(1, -2)
      end
    end
  end
  return false
end

--- (Re)define every highlight group. Defaults use `default = true`, so a
--- colour scheme or an earlier `nvim_set_hl` wins; `opts.highlights`
--- entries are set as given. `@lsp.type.workoutX` (and its `.markdown`
--- variant) link to `WorkoutX`.
function M.apply_highlights()
  local set = vim.api.nvim_set_hl
  for _, kind in ipairs(M.kinds) do
    local group = M.group(kind)
    local user = M.options.highlights[group]
    if user then
      set(0, group, user)
    else
      set(0, group, vim.tbl_extend('force', M.default_highlights[group], { default = true }))
    end
    local lsp = '@lsp.type.' .. M.token_type(kind)
    set(0, lsp, { link = group, default = true })
    set(0, lsp .. '.markdown', { link = group, default = true })
  end
end

--- Start (or reuse) workout-lsp for `buf` if it is a workout log.
function M.attach(buf)
  buf = (buf == nil or buf == 0) and vim.api.nvim_get_current_buf() or buf
  if vim.bo[buf].buftype ~= '' then
    return nil
  end
  local path = vim.api.nvim_buf_get_name(buf)
  local ok, dir = M.is_workout_file(path)
  if not ok then
    return nil
  end
  local full = normalize(path)
  local root = dir or vim.fs.root(full, { '.git' }) or vim.fs.dirname(full)
  local config = vim.tbl_extend('force', {
    name = 'workout',
    cmd = M.options.cmd,
    root_dir = root,
  }, M.options.lsp_config)
  local user_on_attach = config.on_attach
  config.on_attach = function(client, bufnr)
    if M.options.inlay_hints and vim.lsp.inlay_hint then
      pcall(vim.lsp.inlay_hint.enable, true, { bufnr = bufnr })
    end
    if user_on_attach then
      user_on_attach(client, bufnr)
    end
  end
  return vim.lsp.start(config, { bufnr = buf })
end

local group_id

function M.setup(opts)
  if vim.fn.has('nvim-0.10') == 0 then
    vim.notify('workout.nvim needs Neovim 0.10 or newer', vim.log.levels.WARN)
    return
  end
  M.options = vim.tbl_deep_extend('force', vim.deepcopy(M.defaults), opts or {})
  -- lists replace the defaults instead of merging index by index
  for _, key in ipairs({ 'cmd', 'filenames', 'patterns', 'dirs' }) do
    if opts and opts[key] then
      M.options[key] = opts[key]
    end
  end

  group_id = vim.api.nvim_create_augroup('workout', { clear = true })
  M.apply_highlights()
  vim.api.nvim_create_autocmd('ColorScheme', {
    group = group_id,
    callback = M.apply_highlights,
  })

  if M.options.lsp then
    vim.api.nvim_create_autocmd({ 'BufReadPost', 'BufNewFile' }, {
      group = group_id,
      callback = function(ev)
        M.attach(ev.buf)
      end,
    })
    -- buffers opened before setup ran (e.g. a lazy-loaded plugin)
    for _, buf in ipairs(vim.api.nvim_list_bufs()) do
      if vim.api.nvim_buf_is_loaded(buf) then
        M.attach(buf)
      end
    end
  end
end

return M
