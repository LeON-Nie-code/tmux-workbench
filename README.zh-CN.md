# Tmux Workbench

[English](README.md) | 简体中文

Tmux Workbench 是一个面向本机和远程 tmux session 的终端工作区记忆工具。

它会索引你在不同机器和 SSH 服务器上的 tmux workspace，记录项目路径、pane、
git 状态、备注、标签和进入历史，让你用一个简短命令回到完整工作现场：

```bash
ws
```

## 为什么需要它

SSH + tmux 很稳定，但当项目和服务器变多以后，你需要记住太多东西：

- 项目在哪台服务器
- tmux session 叫什么
- 该从哪个路径继续
- 哪个 pane 正在跑 agent / shell
- git 当前是什么分支，是否 dirty
- 上次给这个 workspace 留了什么备注

Tmux Workbench 不替代 tmux。它是在 tmux 之上增加一层本地“项目记忆”。

## 功能

- 索引本机 tmux session 和远程 SSH 服务器上的 tmux session。
- 使用稳定 ID 进入 workspace：`<server>/<tmux-session>`。
- 通过 CLI 管理 server。
- TUI 支持搜索、server 过滤、active/archived 视图和备注编辑。
- scan 时保留 notes、alias、tags、status 和 attach history。
- 单独记录 missing session，不覆盖用户手动 archive 的状态。
- 记录 git branch、commit、dirty、ahead/behind 和 remote URL。
- 后台刷新，不阻塞 TUI 操作。
- 所有状态都保存在本地 SQLite。

## 安装

依赖：

- tmux
- git
- 如果使用远程服务器，需要 ssh

### 安装脚本

```bash
curl -fsSL https://raw.githubusercontent.com/LeON-Nie-code/tmux-workbench/master/install.sh | bash
```

默认安装到 `~/.local/bin/ws`。可以通过 `TMUX_WORKBENCH_INSTALL_DIR` 指定目录。

### Homebrew

```bash
brew tap LeON-Nie-code/tmux-workbench
brew install ws
```

本地开发测试：

```bash
brew install --build-from-source ./Formula/ws.rb
```

### Cargo

```bash
cargo install --git https://github.com/LeON-Nie-code/tmux-workbench ws
```

本地 checkout：

```bash
cargo install --path .
```

### 手动下载

从 [Releases](https://github.com/LeON-Nie-code/tmux-workbench/releases)
下载对应平台的 `ws`，放到 `PATH` 中。

macOS Apple Silicon 示例：

```bash
curl -L -o ws https://github.com/LeON-Nie-code/tmux-workbench/releases/download/v0.1.0/ws-macos-aarch64
chmod +x ws
mkdir -p ~/.local/bin
mv ws ~/.local/bin/ws
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
ws recreate prod/api

ws note prod/api "Backend uses uv. Frontend is in ./web."
ws alias prod/api api
ws tags prod/api work backend
ws status prod/api archived

ws doctor
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

计划中：

- pane layout restore
- asciinema / GIF demo
- 更多 release targets
- 项目公开后准备独立 Homebrew tap

## License

MIT. See [LICENSE](LICENSE).
