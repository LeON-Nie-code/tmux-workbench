use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{Connection, params};

use crate::{
    config::data_path,
    model::{GitInfo, Pane, Workspace},
    util::shell_quote,
};

pub fn open_db() -> Result<Connection> {
    let path = data_path()?.join("workspaces.db");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Connection::open(path).map_err(Into::into)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let old_schema: i64 = conn.query_row(
        "select count(*) from sqlite_master
         where type = 'table'
           and name = 'workspaces'
           and not exists (
             select 1 from pragma_table_info('workspaces') where name = 'id'
           )",
        [],
        |row| row.get(0),
    )?;
    if old_schema > 0 {
        conn.execute_batch(
            "
            drop table if exists panes;
            drop table if exists workspaces;
            ",
        )?;
    }

    conn.execute_batch(
        "
        create table if not exists workspaces (
          id text primary key,
          name text not null,
          alias text,
          server text not null,
          session text not null,
          root_path text not null,
          agent text not null,
          note text not null default '',
          status text not null default 'active',
          tags text not null default '',
          last_seen text not null,
          last_attached_at text,
          attach_count integer not null default 0,
          git_branch text,
          git_head text,
          git_dirty integer,
          git_ahead integer,
          git_behind integer
        );

        create table if not exists panes (
          workspace_id text not null,
          window text not null,
          pane integer not null,
          active integer not null,
          command text not null,
          path text not null,
          title text not null,
          foreign key(workspace_id) references workspaces(id)
        );
        ",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "last_attached_at",
        "alter table workspaces add column last_attached_at text",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "attach_count",
        "alter table workspaces add column attach_count integer not null default 0",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "alias",
        "alter table workspaces add column alias text",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "tags",
        "alter table workspaces add column tags text not null default ''",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "git_branch",
        "alter table workspaces add column git_branch text",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "git_head",
        "alter table workspaces add column git_head text",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "git_dirty",
        "alter table workspaces add column git_dirty integer",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "git_ahead",
        "alter table workspaces add column git_ahead integer",
    )?;
    add_column_if_missing(
        conn,
        "workspaces",
        "git_behind",
        "alter table workspaces add column git_behind integer",
    )?;
    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<()> {
    let sql = format!(
        "select count(*) from pragma_table_info({}) where name = ?1",
        shell_quote(table)
    );
    let exists: i64 = conn.query_row(&sql, params![column], |row| row.get(0))?;
    if exists == 0 {
        conn.execute(alter_sql, [])?;
    }
    Ok(())
}

pub fn upsert_workspace(conn: &Connection, ws: &Workspace) -> Result<()> {
    conn.execute(
        "insert into workspaces (id, name, alias, server, session, root_path, agent, note, status, tags, last_seen, last_attached_at, attach_count, git_branch, git_head, git_dirty, git_ahead, git_behind)
         values (?1, ?2, (select alias from workspaces where id = ?1), ?3, ?4, ?5, ?6, coalesce((select note from workspaces where id = ?1), ''), coalesce((select status from workspaces where id = ?1), 'active'), coalesce((select tags from workspaces where id = ?1), ''), ?7, (select last_attached_at from workspaces where id = ?1), coalesce((select attach_count from workspaces where id = ?1), 0), ?8, ?9, ?10, ?11, ?12)
         on conflict(id) do update set
           name = excluded.name,
           server = excluded.server,
           session = excluded.session,
           root_path = excluded.root_path,
           agent = excluded.agent,
           last_seen = excluded.last_seen,
           alias = workspaces.alias,
           note = workspaces.note,
           status = workspaces.status,
           tags = workspaces.tags,
           last_attached_at = workspaces.last_attached_at,
           attach_count = workspaces.attach_count,
           git_branch = excluded.git_branch,
           git_head = excluded.git_head,
           git_dirty = excluded.git_dirty,
           git_ahead = excluded.git_ahead,
           git_behind = excluded.git_behind",
        params![
            ws.id,
            ws.name,
            ws.server,
            ws.session,
            ws.root_path,
            ws.agent,
            ws.last_seen,
            ws.git.as_ref().and_then(|git| git.branch.as_deref()),
            ws.git.as_ref().and_then(|git| git.head.as_deref()),
            ws.git.as_ref().map(|git| git.dirty as i64),
            ws.git.as_ref().map(|git| git.ahead),
            ws.git.as_ref().map(|git| git.behind),
        ],
    )?;
    conn.execute("delete from panes where workspace_id = ?1", params![ws.id])?;
    for pane in &ws.panes {
        conn.execute(
            "insert into panes (workspace_id, window, pane, active, command, path, title)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ws.id,
                pane.window,
                pane.pane,
                pane.active as i64,
                pane.command,
                pane.path,
                pane.title
            ],
        )?;
    }
    Ok(())
}

pub fn load_workspaces(conn: &Connection) -> Result<Vec<Workspace>> {
    let mut stmt = conn.prepare(
        "select id, name, alias, server, session, root_path, agent, note, status, tags, last_seen, last_attached_at, attach_count, git_branch, git_head, git_dirty, git_ahead, git_behind
         from workspaces order by coalesce(last_attached_at, last_seen) desc, name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            alias: row.get(2)?,
            server: row.get(3)?,
            session: row.get(4)?,
            root_path: row.get(5)?,
            agent: row.get(6)?,
            note: row.get(7)?,
            status: row.get(8)?,
            tags: parse_tags(&row.get::<_, String>(9)?),
            last_seen: row.get(10)?,
            last_attached_at: row.get(11)?,
            attach_count: row.get(12)?,
            git: git_info_from_row(
                row.get(13)?,
                row.get(14)?,
                row.get(15)?,
                row.get(16)?,
                row.get(17)?,
            ),
            panes: Vec::new(),
        })
    })?;

    let mut workspaces = Vec::new();
    for row in rows {
        let mut ws = row?;
        ws.panes = load_panes(conn, &ws.id)?;
        workspaces.push(ws);
    }
    Ok(workspaces)
}

fn git_info_from_row(
    branch: Option<String>,
    head: Option<String>,
    dirty: Option<i64>,
    ahead: Option<i64>,
    behind: Option<i64>,
) -> Option<GitInfo> {
    if branch.is_none() && head.is_none() && dirty.is_none() && ahead.is_none() && behind.is_none()
    {
        return None;
    }
    Some(GitInfo {
        branch,
        head,
        dirty: dirty.unwrap_or(0) != 0,
        ahead: ahead.unwrap_or(0),
        behind: behind.unwrap_or(0),
    })
}

pub fn find_workspace(conn: &Connection, name: &str) -> Result<Option<Workspace>> {
    let matches: Vec<Workspace> = load_workspaces(conn)?
        .into_iter()
        .filter(|ws| {
            ws.id == name
                || ws.name == name
                || ws.session == name
                || ws.alias.as_deref() == Some(name)
        })
        .collect();
    if matches.len() > 1 {
        let ids = matches
            .iter()
            .map(|ws| ws.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("ambiguous workspace `{name}`; use one of: {ids}");
    }
    Ok(matches.into_iter().next())
}

fn load_panes(conn: &Connection, workspace_id: &str) -> Result<Vec<Pane>> {
    let mut stmt = conn.prepare(
        "select window, pane, active, command, path, title
         from panes where workspace_id = ?1 order by window, pane",
    )?;
    let rows = stmt.query_map(params![workspace_id], |row| {
        Ok(Pane {
            window: row.get(0)?,
            pane: row.get(1)?,
            active: row.get::<_, i64>(2)? == 1,
            command: row.get(3)?,
            path: row.get(4)?,
            title: row.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn set_note_by_id(conn: &Connection, id: &str, note: &str) -> Result<usize> {
    conn.execute(
        "update workspaces set note = ?1 where id = ?2",
        params![note, id],
    )
    .map_err(Into::into)
}

pub fn set_status_by_id(conn: &Connection, id: &str, status: &str) -> Result<usize> {
    conn.execute(
        "update workspaces set status = ?1 where id = ?2",
        params![status, id],
    )
    .map_err(Into::into)
}

pub fn set_alias_by_id(conn: &Connection, id: &str, alias: Option<&str>) -> Result<usize> {
    conn.execute(
        "update workspaces set alias = ?1 where id = ?2",
        params![alias, id],
    )
    .map_err(Into::into)
}

pub fn set_tags_by_id(conn: &Connection, id: &str, tags: &[String]) -> Result<usize> {
    conn.execute(
        "update workspaces set tags = ?1 where id = ?2",
        params![format_tags(tags), id],
    )
    .map_err(Into::into)
}

pub fn record_attach(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "update workspaces
         set last_attached_at = ?1,
             attach_count = attach_count + 1
         where id = ?2",
        params![Utc::now().to_rfc3339(), id],
    )?;
    Ok(())
}

fn parse_tags(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn format_tags(tags: &[String]) -> String {
    tags.iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::{
        load_workspaces, migrate, record_attach, set_alias_by_id, set_note_by_id, set_status_by_id,
        set_tags_by_id, upsert_workspace,
    };
    use crate::model::{Pane, Workspace};

    #[test]
    fn upsert_preserves_user_metadata_and_attach_history() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let first = test_workspace("/repo");
        upsert_workspace(&conn, &first).unwrap();
        set_alias_by_id(&conn, &first.id, Some("alias")).unwrap();
        set_note_by_id(&conn, &first.id, "note").unwrap();
        set_status_by_id(&conn, &first.id, "paused").unwrap();
        set_tags_by_id(&conn, &first.id, &["tag".to_string()]).unwrap();
        record_attach(&conn, &first.id).unwrap();

        let second = test_workspace("/repo/subdir");
        upsert_workspace(&conn, &second).unwrap();

        let workspace = load_workspaces(&conn).unwrap().remove(0);
        assert_eq!(workspace.alias.as_deref(), Some("alias"));
        assert_eq!(workspace.note, "note");
        assert_eq!(workspace.status, "paused");
        assert_eq!(workspace.tags, vec!["tag"]);
        assert!(workspace.last_attached_at.is_some());
        assert_eq!(workspace.attach_count, 1);
        assert_eq!(workspace.root_path, "/repo/subdir");
    }

    fn test_workspace(root_path: &str) -> Workspace {
        Workspace {
            id: "server/session".to_string(),
            name: "session".to_string(),
            alias: None,
            server: "server".to_string(),
            session: "session".to_string(),
            root_path: root_path.to_string(),
            agent: "bash".to_string(),
            panes: vec![Pane {
                window: "0:bash".to_string(),
                pane: 0,
                active: true,
                command: "bash".to_string(),
                path: root_path.to_string(),
                title: String::new(),
            }],
            note: String::new(),
            status: "active".to_string(),
            tags: Vec::new(),
            last_seen: "now".to_string(),
            last_attached_at: None,
            attach_count: 0,
            git: None,
        }
    }
}
