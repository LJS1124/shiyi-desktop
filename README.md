# 拾遗 · 灵感与项目管理（macOS 桌面版）

默认是 macOS 原生风格（半透明侧栏、系统字体、系统蓝），另有素白高密度、暗夜专业、宣纸水墨、新粗野、黏土五种风格可在「设置 → 外观」切换，每种都有浅色和深色。这个桌面应用用来管理随手冒出来的想法，以及工作和生活里的项目。基于 Tauri 2：界面是原生 HTML/CSS/JS（无打包步骤），外壳用 Rust，安装包很小。

## 功能

| 视图 | 用来做什么 |
| --- | --- |
| 今日 | 逾期、今天到期、手动放进今天的任务，加上灵感收件箱和项目进度 |
| 灵感 | 随手记，`#标签` 自动归类；可转任务、转项目、用导图展开、AI 拆解 |
| 项目 | 工作 / 生活两类项目卡片；详情页里有看板、甘特图、燃尽图、网络图四个标签页 |
| 看板 | 全部或单个项目的「待办 / 进行中 / 已完成」，拖动卡片改状态和顺序，可按项目分泳道 |
| 甘特图 | 拖动条形改日期、拖两端改工期；前置依赖画成箭头，冲突标红；关键路径任务加红框；日 / 周 / 月缩放；一键自动排期 |
| 燃尽图 | 按任务数或预估工期，画出实际剩余、理想线、总量线和按最近 7 天速度的预测完成日 |
| 网络图 / PERT | 三点估算（乐观 / 最可能 / 悲观）→ ES/EF/LS/LF、松弛时间、关键路径、期望工期和按期完成概率；从卡片拖线建立依赖 |
| 思维导图 | Tab 加子节点、Enter 加同级、拖动换父级、撤销重做；节点或整个分支一键转成任务 / 项目 / 灵感；可从项目生成 |

AI 功能（需要在「设置」里填自己的 API Key）：灵感拆成项目和任务、给项目补任务、排今日、估算工期与依赖、扩展导图节点。所有 AI 结果都会先给你预览、可编辑，确认后才写入。

## 安装

1. 打开仓库的 **Actions → Build macOS app**，进入最近一次成功的运行，下载 `Shiyi-macOS-universal`（同时支持 Apple 芯片和 Intel）。
2. 解压后打开 `Shiyi_1.0.0_universal.dmg`，把 Shiyi 拖进「应用程序」。
3. 因为没有付费的 Apple 开发者签名，第一次打开会被系统拦下。在「终端」里执行一次：

   ```bash
   xattr -dr com.apple.quarantine /Applications/Shiyi.app
   ```

   或者：系统设置 → 隐私与安全性 → 找到 Shiyi → 「仍要打开」。

## 使用要点

- **⌥⇧Space**：在任何应用里唤出拾遗并聚焦记录框（关闭窗口后应用仍在后台，快捷键照样可用；⌘Q 才是真正退出）
- **⌘K** 聚焦记录框，**⌘1–⌘8** 切换视图，**Esc** 关闭详情
- 记录框里以 `/t ` 开头直接记成任务
- AI 设置支持 Anthropic Messages 接口，也支持 OpenAI 兼容格式（LiteLLM、自建网关等），接口地址和模型名都能改

## 数据

- 位置：`~/Library/Application Support/app.shiyi.desktop/data.json`
- 每天第一次保存前自动备份上一版到 `backups/`，保留 14 天
- 「设置 → 数据」可以导出 / 导入 JSON；网页版导出的备份可以直接导入
- API Key 存在同目录的 `settings.json`，权限 600，只在本机

## 本地开发

需要 Rust（`rustup`）、Node 18+ 和 Xcode Command Line Tools。

```bash
npm install
npm run dev          # 开发模式
npm run build:mac    # 打包 universal .app 和 .dmg
```

不装 Rust 也能预览界面：`cd src && python3 -m http.server`，用浏览器打开即可（数据存在浏览器里，AI 不可用）。`python3 scripts/build-preview.py` 会把整个前端打成一个单文件页面 `preview/index.html`，方便放到别处预览。

## 打包

日常提交不会触发编译。需要新安装包时，在 GitHub 的 Actions → Build macOS app 里点 Run workflow，或者推一个 `v*` 版本标签。

## 结构

```
src/                前端（无构建步骤）
  index.html
  styles.css        黏土风设计系统（浅色 / 深色）
  js/core.js        状态、存储、渲染循环、详情面板、设置
  js/views-main.js  今日 / 灵感 / 项目
  js/kanban.js  gantt.js  burndown.js  pert.js  mindmap.js
  js/ai.js          AI 流程（预览后再写入）
src-tauri/          Rust 外壳：本地存储、备份、AI 代理、导入导出、全局快捷键
.github/workflows/  macOS universal 构建
```
