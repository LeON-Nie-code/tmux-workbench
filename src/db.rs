use anyhow::{Result, bail};
use chrono::Utc;
use rusqlite::{Connection, params};

use crate::{
    config::data_path,
    model::{Pane, Workspace},
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
          server text not null,
          session text not null,
          root_path text not null,
          agent text not null,
          note text not null default '',
          status text not null default 'active',
          last_seen text not null,
          last_attached_at text,
          attach_count integer not null default 0
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
        "insert into workspaces (id, name, server, session, root_path, agent, note, status, last_seen, last_attached_at, attach_count)
         values (?1, ?2, ?3, ?4, ?5, ?6, coalesce((select note from workspaces where id = ?1), ''), coalesce((select status from workspaces where id = ?1), 'active'), ?7, (select last_attached_at from workspaces where id = ?1), coalesce((select attach_count from workspaces where id = ?1), 0))
         on conflict(id) do update set
           name = excluded.name,
           server = excluded.server,
           session = excluded.session,
           root_path = excluded.root_path,
           agent = excluded.agent,
           last_seen = excluded.last_seen",
        params![
            ws.id,
            ws.name,
            ws.server,
            ws.session,
            ws.root_path,
            ws.agent,
            ws.last_seen
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
        "select id, name, server, session, root_path, agent, note, status, last_seen, last_attached_at, attach_count
         from workspaces order by coalesce(last_attached_at, last_seen) desc, name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            server: row.get(2)?,
            session: row.get(3)?,
            root_path: row.get(4)?,
            agent: row.get(5)?,
            note: row.get(6)?,
            status: row.get(7)?,
            last_seen: row.get(8)?,
            last_attached_at: row.get(9)?,
            attach_count: row.get(10)?,
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

pub fn find_workspace(conn: &Connection, name: &str) -> Result<Option<Workspace>> {
    let matches: Vec<Workspace> = load_workspaces(conn)?
        .into_iter()
        .filter(|ws| ws.id == name || ws.name == name || ws.session == name)
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
