---
name: shiyi
description: Read and update the user's 拾忆 (Shiyi) app — ideas, projects, tasks, mind maps and their knowledge graph — through the `shiyi` CLI. Use when the user mentions 拾忆/shiyi, their ideas inbox (灵感), projects, to-dos, today's plan, deadlines, or asks to capture, plan, schedule, review or connect things.
---

# 拾忆 (shiyi)

拾忆 is the user's desktop app for scattered ideas and work/life projects. Everything lives in one local SQLite file. You use it through the `shiyi` command; the user sees the same data in the app, with every change you make listed under **操作记录** where they can undo it.

All commands print JSON: `{"ok":true,…}` or `{"ok":false,"error":{"code","message","hint"}}`. Exit code 0 = ok, 1 = error, 2 = bad command. When an error comes back, read `hint` — it says exactly how to fix the request (valid fields, allowed values, the id to use).

## Before you start

1. `shiyi status` — checks the CLI works, shows the permission and counts. If `shiyi` is not found, ask the user to open 拾忆 → 设置 → AI 接入 → 安装命令行工具 (macOS installs it to `~/.local/bin/shiyi`).
2. Always pass `--as <your name>` on writes (e.g. `--as claude`), or set `SHIYI_AGENT=claude` once. The name appears in the user's 操作记录.
3. Permission (`agent_permission`): `read` = you can only read; `write` (default) = create/update anything, but only delete things agents created; `full` = everything. Never try to work around a `permission_denied` — tell the user what you wanted to do and that they can raise it in 设置 → AI 接入.

## Data model (short)

| type | id prefix | key fields |
|---|---|---|
| idea 灵感 | `idea_` | text (first line = title), note, tags[], status inbox/incubating/converted/archived, starred, project_id, task_id |
| project 项目 | `proj_` | name, area work/life, status active/paused/done, goal, note, start_date, due_date, color |
| task 任务 | `task_` | title, project_id (null = loose task), status todo/doing/done, priority 0 none/1 high/2 mid/3 low, start_date, due_date, today_date (planned for that day), estimate {optimistic, likely, pessimistic} days, depends_on[], note |
| map 导图 | `map_` | title, nodes (build with `outline`), idea_id, project_id |
| link 关联 | `link_` | source, target, relation, note, status confirmed/suggested |

Every item also has `version`, `created_at`, `updated_at`, `created_by`, `updated_by`. `shiyi schema [type]` prints the full schema with aliases and examples.

Relations: relates_to, inspired_by, derived_from, part_of, supports, contradicts, depends_on, similar_to.

## Reading

```bash
shiyi today                                   # overdue, planned today, doing, next 7 days, inbox, active projects
shiyi list tasks --status todo,doing --project 网站改版 --due-before fri
shiyi list ideas --status inbox --tag 设计
shiyi list projects --status active
shiyi search 暗色 模式                         # all words must match
shiyi related <id or exact title>             # possibly related items (local text similarity, no AI)
shiyi related --text "想做一个回放工具"         # same, for text you are about to capture
shiyi get <id or exact title>                 # full item + depends_on, dependents, links_in/out (backlinks), project stats
shiyi graph --center <id> --depth 2           # knowledge graph around one item
shiyi log --actor agents --limit 10           # recent changes by agents
```

Lists return compact items (empty fields dropped, `project` name added, `blocked_by` for tasks waiting on open dependencies). Add `--full` for whole documents, `--limit`/`--offset` to page.

## Writing

Simple commands:

```bash
shiyi idea add "周末去看红叶" --tag 旅行 --as claude
shiyi task add 写周报 --project 工作杂事 --due fri --priority 1 --estimate 1,2,3 --today --as claude
shiyi project add 网站改版 --area work --due +30d --goal "新版官网上线" --as claude
shiyi task set <id> --status doing --due +2d --as claude      # any field: --<field> <value>
shiyi task done <id> [<id>…] --as claude                      # also: start, reopen
shiyi task dep <id> --on <other task> --as claude            # dependency; cycles are rejected
shiyi idea convert <id> --to project --as claude              # or --to task --project <p>
shiyi project schedule <id> --as claude                       # auto-plan dates from estimates + dependencies
shiyi map add 发布会 --outline $'场地\n  预算\n嘉宾\n  名单' --project <id> --as claude
shiyi link add <id> <id> --rel inspired_by --note "同一个痛点" --suggest --as claude
```

Several changes at once — one atomic, undoable batch (use this for plans and breakdowns):

```bash
shiyi apply - --as claude <<'JSON'
{"ops": [
  {"op": "project.create", "ref": "p", "name": "搬家", "area": "life", "due": "2026-11-30"},
  {"op": "task.create", "ref": "pack", "title": "打包书和衣服", "project": "$p", "estimate": "1,2,3"},
  {"op": "task.create", "title": "联系搬家公司", "project": "$p", "priority": 1, "due": "+3d"},
  {"op": "task.create", "title": "搬运", "project": "$p", "depends_on": ["$pack"]},
  {"op": "project.schedule", "id": "$p"}
]}
JSON
```

- `ref` names an item created earlier in the same request; refer to it as `"$name"`.
- References accept an id, an exact title (`"project": "搬家"`), or `$ref`. If a title matches several items you get `ambiguous` with candidates — pick an id.
- Dates: `YYYY-MM-DD`, `today`, `tomorrow`, `+3d`, `+1w`, `fri`, `next mon`, `周五`, `下周一`.
- Field names are forgiving (`due` → due_date, `deps`/`after` → depends_on, `project` → project_id), statuses accept Chinese (`进行中`, `已完成`). Unknown fields are rejected with the valid list.
- Op names: `<type>.create|update|delete|restore`, `task.done|start|reopen|add_dep|remove_dep`, `idea.convert|archive`, `project.schedule`, `link.confirm|reject`, `undo`.
- Add `--dry-run` (or `"dry_run": true`) to validate and preview without saving — do this before large or destructive batches.
- Pass `"if_version": N` on an update when you read the item earlier and the user may have edited it since; a `conflict` means re-read and retry.
- Retrying is safe if you give creates your own id: `{"op":"task.create","id":"task_weekly-2026-10-12",…}` returns `existed:true` instead of duplicating.

## Undo

Every write returns `"batch": "bat_…"`. To revert: `shiyi undo <batch> --as claude` (or `shiyi undo last --as claude` for your own most recent batch). If the user changed the same items afterwards you get `conflict`; tell the user instead of forcing.

## How to behave

- **Look before you write.** Search for an existing project/idea before creating a new one (`shiyi related --text "…"` finds near-duplicates); `get` an item before editing it.
- **Capture fast, organize lightly.** A thought the user mentions in passing → `idea add` into the inbox. Don't convert, tag or schedule it unless asked.
- **The user's things are theirs.** Prefer updating status (`archived`, `paused`, `done`) over deleting. Never delete or rewrite user-created content without being asked.
- **Knowledge graph.** Links you create are suggestions: with `write` permission they are always stored as `status: "suggested"` (the result says `needs_review: true`) and only the user can accept them in the app — `link.confirm` from you returns `permission_denied`. Always give a short `note` saying why the two belong together. `shiyi related <id>` is a cheap way to find candidates before you judge them yourself. You can also write `[[exact title]]` inside idea text or notes; it becomes a "mentions" edge.
- **Plans.** When breaking down a project: 3–12 concrete tasks with verbs, estimates in days as `optimistic,likely,pessimistic`, dependencies only where order truly matters, then `project.schedule`. Use one `apply` batch so the user can undo the whole plan at once.
- **Today.** "What should I do today" → `shiyi today`, then suggest a short list and offer to set `today_date` (`--today`) on the chosen tasks.
- **Report back** in a sentence or two: what you changed (titles, not ids) and the batch id for undo.

## Troubleshooting

- `busy` → the app is writing; retry after a second.
- `not_found` → search for the item; ids are case-sensitive.
- `deleted` → the item is in the trash (`shiyi trash`); `<type> restore <id>` brings it back.
- `shiyi doctor` reports broken references, dependency loops and unresolved `[[links]]`.
- Use a different database for experiments: `--db /tmp/test.db` or `SHIYI_DB=/tmp/test.db`.
