# Summary bug

Redirects are currently never counted, even though there is a category for that.

echo "https://httpstat.us/301" | cargo run - -vv --format detailed

📝 Summary
---------------------
🔍 Total............1
✅ Successful.......1
⏳ Timeouts.........0
🔀 Redirected.......0
👻 Excluded.........0
❓ Unknown..........0
🚫 Errors...........0

-> fixed in bf9b2bf

# Only show 200 OK links in -vv output. (TBD)

-> Create separate issue

# Redirection tracking

- Option 1: Keep using `redirect` from `reqwest` Client (introduces state and complexity)
- Option 2: Remove `reqwest`'s `redirect` method, instead introduce a new `Chain` element which handles redirections

# UX

- This redirection tracking/reporting should be independent of `--suggest` (granularity). It probably should always happen.
- When to show redirects? Always or only when explicitly wanted by user?
- Keep debugging statement (`[DEBUG] Redirecting`)
- Add separate section at the end, as with `--suggest`
