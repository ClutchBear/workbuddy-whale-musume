#![allow(unused_imports)]

use super::*;

/* ============================ WorkBuddy 采样 ============================ */

#[derive(Clone, Default)]
pub(crate) struct Status {
    pub(crate) working: i32,
    pub(crate) working_active: i32,
    pub(crate) total: i32,
    pub(crate) title: String,
    pub(crate) db_ok: bool,
    pub(crate) sampled_at: i64,
    pub(crate) latest: i64,
    pub(crate) stale: bool,
}

pub(crate) fn db_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("WORKBUDDY_DB") {
        return std::path::PathBuf::from(p);
    }
    let base = std::env::var("USERPROFILE").unwrap_or_default();
    std::path::PathBuf::from(base).join(".workbuddy").join("workbuddy.db")
}

pub(crate) fn sample() -> Status {
    let now = now_ms();
    let mut st = Status { sampled_at: now, db_ok: false, ..Default::default() };
    let p = db_path();
    let s = p.to_string_lossy().replace('\\', "/");
    // 只读打开；WAL 模式下再退到 immutable，尽量不干扰宿主进程
    let con = match rusqlite::Connection::open_with_flags(
        format!("file:{}?mode=ro", s),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) {
        Ok(c) => c,
        Err(_) => match rusqlite::Connection::open_with_flags(
            format!("file:{}?immutable=1", s),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return st,
        },
    };
    st.db_ok = true;
    if let Ok(mut q) = con.prepare(
        "SELECT COUNT(*), MAX(updated_at) FROM sessions WHERE deleted_at IS NULL AND status='working'",
    ) {
        if let Ok(mut rows) = q.query([]) {
            if let Ok(Some(r)) = rows.next() {
                st.working = r.get::<_, i32>(0).unwrap_or(0);
                st.latest = r.get::<_, i64>(1).unwrap_or(0);
            }
        }
    }
    if let Ok(mut q) = con.prepare("SELECT COUNT(*) FROM sessions WHERE deleted_at IS NULL") {
        if let Ok(mut rows) = q.query([]) {
            if let Ok(Some(r)) = rows.next() {
                st.total = r.get::<_, i32>(0).unwrap_or(0);
            }
        }
    }
    // 活跃会话标题：既用于信息条，也当作「任务内容 / 关键词」的输入
    if let Ok(mut q) = con.prepare(
        "SELECT COALESCE(title,''), COALESCE(cwd,'') FROM sessions WHERE deleted_at IS NULL AND status='working' ORDER BY updated_at DESC LIMIT 1",
    ) {
        if let Ok(mut rows) = q.query([]) {
            if let Ok(Some(r)) = rows.next() {
                let t: String = r.get::<_, String>(0).unwrap_or_default();
                let c: String = r.get::<_, String>(1).unwrap_or_default();
                st.title = if t.is_empty() {
                    c.rsplit(['\\', '/']).next().unwrap_or("").to_string()
                } else {
                    t
                };
            }
        }
    }
    st.stale = st.working > 0 && st.latest > 0 && (now - st.latest) > STALE_MS;
    st.working_active = if st.stale { 0 } else { st.working };
    st
}
