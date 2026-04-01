app-title = Clip Pop
about = About
repository = Repository
view = View

# Toolbar
clear-all = Clear History
search-placeholder = Search…
private-mode = Private Mode

# Sections
pinned = Pinned
history = History

# Entry types
entry-image = 🖼  Image

# Empty / error states
empty-history = Nothing here yet. Copy something to get started.
no-results = No entries match your search.
clipboard-unavailable = Clipboard protocol unavailable. Your compositor may not support zwlr_data_control or ext_data_control.

# Confirm clear dialog
confirm-clear-title = Clear history?
confirm-clear-body = This will remove all unpinned items. Pinned items are kept.
confirm-clear-confirm = Clear
confirm-clear-cancel = Cancel

# Relative timestamps
time-just-now = just now
time-minutes-ago = { $count ->
    [one] 1 min ago
   *[other] { $count } min ago
}
time-hours-ago = { $count ->
    [one] 1 hr ago
   *[other] { $count } hr ago
}
time-days-ago = { $count ->
    [one] 1 day ago
   *[other] { $count } days ago
}

git-description = Git commit {$hash} on {$date}
