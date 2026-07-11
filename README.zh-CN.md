# Tmux Workbench

[English](README.md) | 简体中文

Tmux Workbench 是一个面向 SSH、tmux 和 AI coding agent 工作流的 workspace
记忆管理器。

它会索引本机和远程 SSH 服务器上的 tmux workspace，记录项目路径、pane、git
状态、AI agent 初始化文档、备注、标签和进入历史，让你用一个简短命令回到完整工作现场：

```bash
ws
```

<p align="center">
  <img src="docs/assets/demo.gif" alt="Tmux Workbench CLI and TUI demo" width="100%">
</p>

## 为什么需要它

SSH + tmux 很稳定，但当项目和服务器变多以后，你需要记住太多东西：

- 项目在哪台服务器
- tmux session 叫什么
- 该从哪个路径继续
- 哪个 pane 正在跑 agent / shell
- git 当前是什么分支，是否 dirty
- 项目里有没有 `AGENTS.md`、`CLAUDE.md` 这类 agent 初始化文档
- 上次给这个 workspace 留了什么备注

Tmux Workbench 不替代 tmux。它是在 tmux 之上增加一层本地“项目记忆”。

## AI Agent 工作流

现在很多 workspace 里都会长期跑 Claude Code、Codex、Gemini 或 Aider 这类
coding agent。Tmux Workbench 会把这些 pane 当成重要的 workspace context：

- 优先使用 agent pane 的路径作为 workspace root
- 索引 `CLAUDE.md`、`AGENTS.md`、`.cursorrules` 这类 agent 初始化文档
- 在 TUI detail 里展示 agent docs
- 用 `ws agent <workspace>` 查看已索引的 agent context
- 远程 attach 较慢时显示 loading 状态，避免误以为卡住

## 功能

- 索引本机 tmux session 和远程 SSH 服务器上的 tmux session。
- 使用稳定 ID 进入 workspace：`<server>/<tmux-session>`。
- 通过 CLI 管理 server。
- TUI 支持搜索、server 过滤、active/archived 视图和备注编辑。
- scan 时保留 notes、alias、tags、status 和 attach history。
- 单独记录 missing session，不覆盖用户手动 archive 的状态。
- 记录 git branch、commit、dirty、ahead/behind 和 remote URL。
- 检测 workspace root 下的 AI agent context 文件。
- 后台刷新，不阻塞 TUI 操作。
- 用 `ws doctor` 诊断本机和远程环境。
- 用 `ws stats` 查看本地使用统计。
- 所有状态都保存在本地 SQLite。

## 安装

依赖：

- tmux
- git
- 如果使用远程服务器，需要 ssh

### 推荐安装

```bash
curl -fsSL https://raw.githubusercontent.com/LeON-Nie-code/tmux-workbench/master/install.sh | bash
```

安装脚本会自动选择当前平台的二进制文件，安装到一个可写目录，验证安装结果，
并在 `PATH` 未配置时给出修复命令。

如果想指定安装目录：

```bash
curl -fsSL https://raw.githubusercontent.com/LeON-Nie-code/tmux-workbench/master/install.sh \
  | TMUX_WORKBENCH_INSTALL_DIR="$HOME/bin" bash
```

### 其他安装方式

从 GitHub 通过 Cargo 安装：

```bash
cargo install --git https://github.com/LeON-Nie-code/tmux-workbench ws
```

从本地 checkout 安装：

```bash
git clone https://github.com/LeON-Nie-code/tmux-workbench.git
cd tmux-workbench
cargo install --path .
```

手动下载：

```bash
curl -L -o ws https://github.com/LeON-Nie-code/tmux-workbench/releases/download/v0.1.2/ws-macos-aarch64
chmod +x ws
mkdir -p ~/.local/bin
mv ws ~/.local/bin/ws
```

Homebrew：

可以从当前仓库作为 Homebrew tap 安装。Homebrew 6 对自定义 tap 需要先显式
trust：

```bash
brew tap LeON-Nie-code/tmux-workbench https://github.com/LeON-Nie-code/tmux-workbench
brew trust LeON-Nie-code/tmux-workbench
brew install LeON-Nie-code/tmux-workbench/ws
```

验证安装：

```bash
ws --version
ws doctor
```

## 快速开始

```bash
ws init
ws servers
ws add-server prod --ssh "ssh prod"
ws scan
ws
```

直接进入某个 workspace：

```bash
ws attach prod/api
```

## 常用命令

```bash
ws servers
ws add-server prod --ssh "ssh prod"
ws add-server local-dev --local
ws remove-server prod

ws scan
ws list
ws list --server prod
ws list --status active
ws list --all
ws list --json

ws attach prod/api
ws agent prod/api
ws recreate prod/api

ws note prod/api "Backend uses uv. Frontend is in ./web."
ws alias prod/api api
ws tags prod/api work backend
ws status prod/api archived

ws doctor
ws stats
ws open-config
```

远程连接直接使用系统 `ssh`，所以已有的 `~/.ssh/config`、key、ProxyCommand、
云厂商生成的 SSH host 都可以继续使用。

## TUI

```bash
ws
```

快捷键：

```text
Enter  进入 workspace
/      搜索
n      编辑 note
a      archive / unarchive
v      切换 all / active / archived
s      切换 server filter
z      在 archived 和 all 之间跳转
r      重新扫描
j/k    移动
q      退出
```

搜索支持普通文本和结构化 filter：

```text
server:prod status:active tag:backend git:dirty
```

README 中的 GIF 使用 [VHS](https://github.com/charmbracelet/vhs) 录制，
运行的是真实 `ws` binary 和本地 fixture 数据。录制脚本见
[`docs/demo/workbench.tape`](docs/demo/workbench.tape)。

## 配置

配置文件：

```text
~/.config/ws/config.yaml
```

示例：

```yaml
servers:
  - name: local
    ssh: ""
    term: xterm-256color
    local: true
  - name: prod
    ssh: ssh prod
    term: xterm-256color
    local: false
```

本地索引：

```text
~/.local/share/ws/workspaces.db
```

## 第一次使用

如果还没有索引任何 workspace，直接运行 `ws` 会显示最短的启动路径：

```bash
ws scan
ws
```

如果要添加远程机器：

```bash
ws add-server prod --ssh "ssh prod"
ws scan
```

如果连接或 tmux 检测有问题：

```bash
ws doctor
```

`ws doctor` 会检查本机命令、config/database 路径、SSH 连通性、远程 tmux
是否可用，以及已索引 workspace 是否仍然存在。

`ws stats` 只读取本地 SQLite，不上传任何 telemetry。它会汇总 workspace
数量、attach 次数、missing session 和最常用 server。

## 项目状态

Tmux Workbench 目前仍是 pre-1.0，优先服务真实的 SSH + tmux 日常开发流。
CLI 和数据库格式后续仍可能调整。

已实现：

- 本机和远程 tmux 索引
- 并发 scan 和命令 timeout
- TUI 后台刷新和 scan 状态展示
- server 管理 CLI
- notes、alias、tags、archive、attach history
- missing session presence tracking
- git snapshot
- SQLite `user_version`
- JSON 输出
- `ws doctor` 和本地 usage stats

计划中：

- pane layout restore
- asciinema / GIF demo
- 更多 release targets
- 项目公开后准备独立 Homebrew tap

更多内部设计见 [`docs/architecture.md`](docs/architecture.md)，和其他工具的区别见
[`docs/comparison.md`](docs/comparison.md)。

## License

MIT. See [LICENSE](LICENSE).
