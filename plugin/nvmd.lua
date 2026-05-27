if vim.g.loaded_nvmd == 1 then
  return
end
vim.g.loaded_nvmd = 1

require("nvmd").setup()
