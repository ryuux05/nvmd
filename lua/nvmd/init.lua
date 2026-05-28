local M = {}
local uv = vim.uv or vim.loop

local defaults = {
  live_reload = true,
  debounce_ms = 150,
}

local config = vim.deepcopy(defaults)
local sessions = {}
local group_name = "nvmd_cursor_sync"

local function is_markdown_file(path)
  return path and path:lower():match("%.md$") ~= nil
end

local function binary_name()
  return vim.fn.has("win32") == 1 and "nvmd.exe" or "nvmd"
end

local function plugin_root()
  local module_path = debug.getinfo(1, "S").source:gsub("^@", "")
  return vim.fn.fnamemodify(module_path, ":p:h:h:h")
end

local function built_binary()
  return plugin_root() .. "/target/release/" .. binary_name()
end

local function executable(path)
  return path and path ~= "" and vim.fn.executable(path) == 1
end

local function resolve_binary()
  if config.binary and config.binary ~= "" then
    if executable(config.binary) then
      return config.binary
    end
    return nil, "configured binary is not executable: " .. config.binary
  end

  local candidate = built_binary()
  if executable(candidate) then
    return candidate
  end

  local from_path = binary_name()
  if executable(from_path) then
    return from_path
  end

  return nil, "binary not found. Run :NvmdBuild or reinstall with your plugin manager build hook"
end

local function markdown_path(bufnr)
  bufnr = bufnr or vim.api.nvim_get_current_buf()
  local name = vim.api.nvim_buf_get_name(bufnr)
  if name == "" then
    return nil
  end
  if vim.bo[bufnr].filetype ~= "markdown" and not is_markdown_file(name) then
    return nil
  end
  return vim.fn.fnamemodify(name, ":p")
end

local function readable_markdown(path)
  if not path or path == "" or not is_markdown_file(path) then
    return nil
  end
  local expanded = vim.fn.expand(path)
  local candidates = {
    vim.fn.fnamemodify(expanded, ":p"),
  }
  local buffer_name = vim.api.nvim_buf_get_name(0)
  if buffer_name ~= "" then
    table.insert(candidates, vim.fn.fnamemodify(buffer_name, ":p:h") .. "/" .. expanded)
  end
  for _, candidate in ipairs(candidates) do
    local absolute = vim.fn.fnamemodify(candidate, ":p")
    if vim.fn.filereadable(absolute) == 1 then
      return absolute
    end
  end
  return nil
end

local function path_from_cursor()
  local cfile = vim.fn.expand("<cfile>")
  local direct = readable_markdown(cfile)
  if direct then
    return direct
  end

  local line = vim.api.nvim_get_current_line()
  for candidate in line:gmatch("[%w%._%-%~/]+%.md") do
    local resolved = readable_markdown(candidate)
    if resolved then
      return resolved
    end
  end

  return nil
end

local function open_target(opts)
  opts = opts or {}
  local arg = opts.args and vim.trim(opts.args) or ""
  if arg ~= "" then
    return readable_markdown(arg)
  end

  local bufnr = vim.api.nvim_get_current_buf()
  local path = markdown_path(bufnr)
  if path then
    return path
  end

  return path_from_cursor()
end

local function session_file(path, prefix, suffix)
  local dir = vim.fn.stdpath("cache") .. "/nvmd"
  vim.fn.mkdir(dir, "p")
  return dir .. "/" .. prefix .. "-" .. vim.fn.sha256(path):sub(1, 16) .. suffix
end

local function cursor_file(path)
  return session_file(path, "cursor", ".txt")
end

local function content_file(path)
  return session_file(path, "content", ".md")
end

local function publish_cursor(bufnr)
  local path = markdown_path(bufnr)
  local session = path and sessions[path] or nil
  if not session or vim.api.nvim_get_current_buf() ~= bufnr then
    return
  end
  local line = vim.api.nvim_win_get_cursor(0)[1]
  vim.fn.writefile({ tostring(line) }, session.cursor_file)
end

local function write_snapshot(bufnr, session)
  if not session or not session.content_file or not vim.api.nvim_buf_is_valid(bufnr) then
    return
  end
  vim.fn.writefile(vim.api.nvim_buf_get_lines(bufnr, 0, -1, false), session.content_file)
end

local function stop_timer(session)
  if session and session.timer then
    session.timer:stop()
    session.timer:close()
    session.timer = nil
  end
end

local function schedule_snapshot(bufnr)
  local path = markdown_path(bufnr)
  local session = path and sessions[path] or nil
  if not session or not session.content_file then
    return
  end
  stop_timer(session)
  session.timer = uv.new_timer()
  session.timer:start(config.debounce_ms, 0, vim.schedule_wrap(function()
    if sessions[path] == session then
      write_snapshot(bufnr, session)
      stop_timer(session)
    end
  end))
end

local function session_running(session)
  return session and vim.fn.jobwait({ session.job_id }, 0)[1] == -1
end

function M.build()
  if vim.fn.executable("cargo") ~= 1 then
    vim.notify("nvmd: cargo is not executable; install Rust first", vim.log.levels.ERROR)
    return
  end

  local root = plugin_root()
  vim.notify("nvmd: building release binary...", vim.log.levels.INFO)
  vim.fn.jobstart({ "cargo", "build", "--release" }, {
    cwd = root,
    stdout_buffered = true,
    stderr_buffered = true,
    on_exit = function(_, code)
      if code == 0 and executable(built_binary()) then
        vim.schedule(function()
          vim.notify("nvmd: build complete", vim.log.levels.INFO)
        end)
      else
        vim.schedule(function()
          vim.notify("nvmd: build failed; run `cargo build --release` in " .. root, vim.log.levels.ERROR)
        end)
      end
    end,
  })
end

function M.open(opts)
  local bufnr = vim.api.nvim_get_current_buf()
  local path = open_target(opts)
  if not path then
    vim.notify("nvmd: no readable Markdown file found for current buffer or cursor", vim.log.levels.WARN)
    return
  end
  local buffer_path = markdown_path(bufnr)
  local use_buffer = buffer_path == path

  local current = sessions[path]
  if session_running(current) then
    if use_buffer then
      publish_cursor(bufnr)
    end
    return
  end

  local sync_path = cursor_file(path)
  local snapshot_path = config.live_reload and use_buffer and content_file(path) or nil
  if snapshot_path then
    write_snapshot(bufnr, { content_file = snapshot_path })
  end
  local binary, binary_error = resolve_binary()
  if not binary then
    vim.notify("nvmd: " .. binary_error, vim.log.levels.ERROR)
    return
  end
  local args = {
    binary,
    "--window-process",
    "--cursor-file",
    sync_path,
  }
  if snapshot_path then
    vim.list_extend(args, { "--content-file", snapshot_path })
  end
  table.insert(args, path)
  local job_id
  job_id = vim.fn.jobstart(args, {
    detach = false,
    on_exit = function()
      if sessions[path] and sessions[path].job_id == job_id then
        stop_timer(sessions[path])
        vim.fn.delete(sessions[path].cursor_file)
        if sessions[path].content_file then
          vim.fn.delete(sessions[path].content_file)
        end
        sessions[path] = nil
      end
    end,
  })
  if job_id <= 0 then
    vim.notify("nvmd: failed to launch " .. binary, vim.log.levels.ERROR)
    return
  end

  sessions[path] = {
    job_id = job_id,
    cursor_file = sync_path,
    content_file = snapshot_path,
  }
  if use_buffer then
    publish_cursor(bufnr)
  end
end

function M.close(opts)
  local path = open_target(opts)
  local session = path and sessions[path] or nil
  if not session then
    return
  end
  stop_timer(session)
  vim.fn.jobstop(session.job_id)
  vim.fn.delete(session.cursor_file)
  if session.content_file then
    vim.fn.delete(session.content_file)
  end
  sessions[path] = nil
end

function M.toggle(opts)
  local path = open_target(opts)
  if path and session_running(sessions[path]) then
    M.close(opts)
  else
    M.open(opts)
  end
end

function M.refresh()
  local path = markdown_path()
  if path and session_running(sessions[path]) then
    write_snapshot(vim.api.nvim_get_current_buf(), sessions[path])
    publish_cursor(vim.api.nvim_get_current_buf())
  else
    M.open()
  end
end

function M.setup(opts)
  config = vim.tbl_deep_extend("force", vim.deepcopy(defaults), opts or {})
  config.debounce_ms = math.max(0, tonumber(config.debounce_ms) or defaults.debounce_ms)

  vim.api.nvim_create_user_command("NvmdOpen", M.open, {
    force = true,
    nargs = "?",
    complete = "file",
  })
  vim.api.nvim_create_user_command("NvmdClose", M.close, {
    force = true,
    nargs = "?",
    complete = "file",
  })
  vim.api.nvim_create_user_command("NvmdToggle", M.toggle, {
    force = true,
    nargs = "?",
    complete = "file",
  })
  vim.api.nvim_create_user_command("NvmdRefresh", M.refresh, { force = true })
  vim.api.nvim_create_user_command("NvmdBuild", M.build, { force = true })

  local group = vim.api.nvim_create_augroup(group_name, { clear = true })
  vim.api.nvim_create_autocmd({ "CursorMoved", "CursorMovedI", "BufEnter" }, {
    group = group,
    callback = function(args)
      publish_cursor(args.buf)
    end,
  })
  vim.api.nvim_create_autocmd({ "TextChanged", "TextChangedI" }, {
    group = group,
    callback = function(args)
      schedule_snapshot(args.buf)
    end,
  })
end

return M
