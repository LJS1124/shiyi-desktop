# 十一遗 · 捡起被遗忘的，让项目更完整（macOS 桌面版）

管理随手冒出来的想法、工作和生活里的项目，并把它们连成一张知识图谱。**你和你的 AI 助手用的是同一份数据**：界面给人用，`shiyi` 命令行给 AI 用，AI 做的每一处修改都记在「操作记录」里，可以一键撤销。

默认是 macOS 原生风格（半透明侧栏、系统字体、系统蓝），另有素白高密度、暗夜专业、宣纸水墨、新粗野、黏土五种风格可在「设置 → 外观」切换，每种都有浅色和深色。基于 Tauri 2：界面是原生 HTML/CSS/JS（无打包步骤），外壳用 Rust，数据在一个 SQLite 文件里。

## 功能

| 视图 | 用来做什么 |
| --- | --- |
| 今日 | 逾期、今天到期、手动放进今天的任务，加上灵感收件箱和项目进度 |
| 灵感 | 随手记，`#标签` 自动归类，`[[标题]]` 双链到别的灵感 / 项目 / 任务；可转任务、转项目、用导图展开、AI 拆解 |
| 项目 | 工作 / 生活两类项目卡片；详情页里有看板、甘特图、燃尽图、网络图四个标签页 |
| 看板 | 全部或单个项目的「待办 / 进行中 / 已完成」，拖动卡片改状态和顺序，可按项目分泳道 |
| 甘特图 | 拖动条形改日期、拖两端改工期；前置依赖画成箭头，冲突标红；关键路径任务加红框；日 / 周 / 月缩放；一键自动排期 |
| 燃尽图 | 按任务数或预估工期，画出实际剩余、理想线、总量线和按最近 7 天速度的预测完成日 |
| 网络图 / PERT | 三点估算（乐观 / 最可能 / 悲观）→ ES/EF/LS/LF、松弛时间、关键路径、期望工期和按期完成概率；从卡片拖线建立依赖 |
| 思维导图 | Tab 加子节点、Enter 加同级、拖动换父级、撤销重做；节点或整个分支一键转成任务 / 项目 / 灵感；可从项目生成 |
| 知识图谱 | 手动关联、AI 建议的关联（待你确认）、备注里的 `[[双链]]`、相同标签、项目归属与依赖，画成一张力导向图；可按类型筛选、以某一条为中心看局部、拖线建立关联 |
| 操作记录 | 你和 AI 的每一次修改，按批次列出，任何一批都能撤销（撤销也能再撤销） |

内置 AI 功能（需要在「设置」里填自己的 API Key）：灵感拆成项目和任务、给项目补任务、排今日、估算工期与依赖、扩展导图节点、找关联。所有 AI 结果都会先给你预览、可编辑，确认后才写入。

## 让 AI 助手使用十一遗

任何能运行命令的 AI 助手（Claude Code、Codex、Cursor、自己写的 agent）都可以通过命令行读写十一遗，不需要 MCP。

1. 「设置 → AI 接入 → 安装命令行工具」：在 `~/.local/bin/shiyi` 放一个启动脚本（找不到命令时把 `~/.local/bin` 加进 PATH）。
2. 「安装到 Claude Code」：把技能说明写到 `~/.claude/skills/shiyi/SKILL.md`，Claude Code 会自动读取；别的 AI 可以「复制技能说明」粘进提示词。
3. 试一试：`shiyi today --pretty`，或者直接对 AI 说「看看十一遗里我今天该做什么」。

命令行的输入输出都是 JSON，出错时会给出 `code`、`message` 和告诉 AI 怎么改的 `hint`：

```bash
shiyi today                                         # 今天要做的、逾期的、收件箱、进行中的项目
shiyi list tasks --status todo,doing --project 网站改版
shiyi get <id 或标题>                               # 详情 + 反向链接 + 依赖
shiyi search 暗色 模式
shiyi graph --center <id> --depth 2                 # 知识图谱
shiyi idea add "周末去看红叶" --tag 旅行 --as claude
shiyi task add 写周报 --project 工作杂事 --due fri --estimate 1,2,3 --as claude
shiyi link add <id> <id> --rel inspired_by --note "同一个痛点" --suggest --as claude
shiyi apply - --as claude < plan.json               # 一批操作，要么全成功要么全不生效
shiyi undo <batch> --as claude
shiyi schema                                        # 全部字段、操作和约定
```

批量操作里可以用 `"ref"` 给新建的条目起名，后面用 `"$名字"` 引用；日期可以写 `today`、`+3d`、`fri`、`下周一`；`--dry-run` 只校验不保存。完整说明见 [`skills/shiyi/SKILL.md`](skills/shiyi/SKILL.md)。

**安全边界**：「设置 → AI 接入 → AI 权限」可选只读 / 可写（默认）/ 完全。可写时 AI 只能删除 AI 自己建的内容、只能撤销 AI 的操作；删除都是软删除。命令行永远以 `agent:<名字>` 的身份写入，界面里的修改才算「我」。

## 安装

1. 打开仓库的 **Actions → Build macOS app**，进入最近一次成功的运行，下载 `Shiyi-macOS-universal`（同时支持 Apple 芯片和 Intel）。
2. 解压后打开 `Shiyi_1.1.0_universal.dmg`，把 Shiyi 拖进「应用程序」。
3. 因为没有付费的 Apple 开发者签名，第一次打开会被系统拦下。在「终端」里执行一次：

   ```bash
   xattr -dr com.apple.quarantine /Applications/Shiyi.app
   ```

   或者：系统设置 → 隐私与安全性 → 找到 Shiyi → 「仍要打开」。

## 使用要点

- **⌥⇧Space**：在任何应用里唤出十一遗并聚焦记录框（关闭窗口后应用仍在后台，快捷键照样可用；⌘Q 才是真正退出）
- **⌘K** 聚焦记录框，**⌘1–⌘8** 切换视图，**Esc** 关闭详情
- 记录框里以 `/t ` 开头直接记成任务；输入 `[[` 会弹出条目列表，选中即插入双链
- AI 设置支持 Anthropic Messages 接口，也支持 OpenAI 兼容格式（LiteLLM、自建网关等），接口地址和模型名都能改

## 数据

- 位置：`~/Library/Application Support/app.shiyi.desktop/shiyi.db`（SQLite；环境变量 `SHIYI_DB` 可以改）
- 升级时会自动把旧版的 `data.json` 导入数据库，原文件改名为 `data.json.imported`
- 每天自动备份一份到 `backups/shiyi-日期.db`，保留 14 天；操作记录保留 180 天
- 「设置 → 数据」可以导出 / 导入 JSON；网页版导出的备份、旧版备份都能导入，导入本身也能撤销
- API Key 存在同目录的 `settings.json`，权限 600，只在本机

## 本地开发

需要 Rust（`rustup`）、Node 18+ 和 Xcode Command Line Tools。

```bash
npm install
npm run dev          # 开发模式
npm run build:mac    # 打包 universal .app 和 .dmg

cd core && cargo test                      # 数据层 + 命令行的测试
cargo run --bin shiyi -- today --pretty    # 直接用命令行（SHIYI_DB=/tmp/x.db 用测试库）
```

不装 Rust 也能预览界面：`cd src && python3 -m http.server`，用浏览器打开即可（数据存在浏览器里）。`python3 scripts/build-preview.py` 会把整个前端打成一个单文件页面 `preview/index.html`。

端到端测试（界面和命令行同时改一个数据库）：`cd core && cargo build --bins --examples && cd .. && python3 scripts/build-preview.py && python3 scripts/e2e/test_app.py`。

## 打包

日常提交只跑 Linux 上的测试（`.github/workflows/test.yml`）。需要新安装包时，在 GitHub 的 Actions → Build macOS app 里点 Run workflow，或者推一个 `v*` 版本标签。

## 结构

```
core/               数据层（Rust）：SQLite、操作与撤销、知识图谱、`shiyi` 命令行
  src/model.rs      条目类型、字段、宽松输入的校验与纠正
  src/ops.rs        写操作、批次、权限、撤销
  src/query.rs      today / list / get / search / log / doctor / snapshot
  src/graph.rs      知识图谱（显式关联 + 字段 + [[双链]] + 标签）
  src/cli.rs        命令行
skills/shiyi/       给 AI 的技能说明（SKILL.md，也编进了应用）
src/                前端（无构建步骤）
  js/core.js        状态、渲染循环、详情面板、设置
  js/store.js       唯一的写入通道：应用里写数据库，网页预览里写浏览器
  js/graph.js       知识图谱视图、双链、关联
  js/agent.js       操作记录、AI 接入设置
  js/views-main.js  今日 / 灵感 / 项目
  js/kanban.js  gantt.js  burndown.js  pert.js  mindmap.js  ai.js
src-tauri/          Rust 外壳：接上 core、监听 AI 的修改、安装命令行和技能、AI 代理、导入导出、全局快捷键
scripts/            预览打包、端到端测试
```
