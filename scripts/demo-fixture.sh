#!/usr/bin/env bash
set -euo pipefail

home="${1:?usage: demo-fixture.sh <home>}"
db_dir="$home/.local/share/ws"
config_dir="$home/.config/ws"

mkdir -p "$db_dir" "$config_dir"

cat >"$config_dir/config.yaml" <<'YAML'
servers:
  - name: local
    ssh: ""
    term: xterm-256color
    local: true
  - name: prod
    ssh: ssh prod
    term: xterm-256color
    local: false
  - name: research
    ssh: ssh research
    term: xterm-256color
    local: false
YAML

sqlite3 "$db_dir/workspaces.db" <<'SQL'
create table workspaces (
  id text primary key,
  name text not null,
  alias text,
  server text not null,
  session text not null,
  root_path text not null,
  agent text not null,
  note text not null default '',
  status text not null default 'active',
  presence text not null default 'seen',
  tags text not null default '',
  last_seen text not null,
  last_attached_at text,
  attach_count integer not null default 0,
  git_branch text,
  git_head text,
  git_remote text,
  git_dirty integer,
  git_ahead integer,
  git_behind integer
);

create table panes (
  workspace_id text not null,
  window text not null,
  pane integer not null,
  active integer not null,
  command text not null,
  path text not null,
  title text not null
);

insert into workspaces values
('prod/api','api','api','prod','api','/srv/api','codex','Backend uses uv. Check worker before deploy.','active','seen','backend,prod','2026-07-01T10:00:00Z','2026-07-01T10:35:00Z',8,'main','d43063f','https://github.com/example/api',1,1,0),
('prod/worker','worker',null,'prod','worker','/srv/worker','bash','Runs queue consumers and btop.','active','seen','backend,prod','2026-07-01T09:40:00Z','2026-07-01T09:55:00Z',4,'release','a81f222','https://github.com/example/worker',1,0,0),
('research/neuroplay','neuroplay','neuro','research','neuroplay','/data/code/neuroplay','claude','Frontend in ./web. Dataset notes in docs/.','active','seen','research,frontend','2026-07-01T08:25:00Z','2026-07-01T09:10:00Z',11,'main','91c2f04','https://github.com/example/neuroplay',0,0,0),
('local/tmux-workbench','tmux-workbench','tmux-workbench','local','tmux-workbench','~/code/tmux-workbench','zsh','Open source polish and release prep.','active','seen','oss,rust','2026-07-01T10:45:00Z','2026-07-01T10:50:00Z',15,'master','acade4e','https://github.com/LeON-Nie-code/tmux-workbench',1,0,0),
('prod/old-dashboard','old-dashboard',null,'prod','old-dashboard','/srv/dashboard','node','Archived after migration to admin-v2.','archived','missing','frontend,legacy','2026-06-28T12:00:00Z',null,1,'legacy','0ac91be','https://github.com/example/dashboard',0,0,3);

insert into panes values
('prod/api','0:api',0,1,'codex','/srv/api','api agent'),
('prod/api','1:shell',0,0,'zsh','/srv/api','shell'),
('prod/worker','0:worker',0,1,'bash','/srv/worker','worker shell'),
('prod/worker','1:monitor',0,0,'btop','/srv/worker','monitor'),
('research/neuroplay','0:main',0,1,'claude','/data/code/neuroplay','claude'),
('research/neuroplay','1:web',0,0,'npm','/data/code/neuroplay/web','web'),
('local/tmux-workbench','0:main',0,1,'zsh','~/code/tmux-workbench','local shell'),
('local/tmux-workbench','1:tests',0,0,'cargo','~/code/tmux-workbench','tests'),
('prod/old-dashboard','0:legacy',0,0,'node','/srv/dashboard','legacy');

pragma user_version = 1;
SQL
