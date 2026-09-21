"""workbuddy-pet 隔离自测：用假库驱动状态迁移，验证 idle / working / stale / error 四态。

用法：python selftest_pet.py
"""
import os
import shutil
import sqlite3
import subprocess
import time

ROOT = r"D:\work\demo\workbuddy-pet-native"
TDIR = os.path.join(ROOT, "_selftest")
DB = os.path.join(TDIR, "fake.db")
EXE = os.path.join(TDIR, "workbuddy-pet.exe")
LOG = os.path.join(TDIR, "workbuddy-pet.log")

now_ms = lambda: int(time.time() * 1000)


def kill():
    subprocess.run(["taskkill", "/F", "/IM", "workbuddy-pet.exe"],
                   capture_output=True, text=True)


def make_db(rows):
    """rows: [(id, status, deleted_at, updated_at_delta_ms, title, cwd)]"""
    for f in (DB, DB + "-wal", DB + "-shm"):
        if os.path.exists(f):
            os.remove(f)
    con = sqlite3.connect(DB)
    con.execute(
        "CREATE TABLE sessions (id TEXT, status TEXT, deleted_at INTEGER,"
        " updated_at INTEGER, title TEXT, cwd TEXT)"
    )
    now = now_ms()
    for row in rows:
        sid, status, deleted, delta = row[:4]
        title = row[5 - 1] if len(row) > 4 else ""
        cwd = row[5] if len(row) > 5 else ""
        con.execute(
            "INSERT INTO sessions VALUES (?,?,?,?,?,?)",
            (sid, status, None if deleted is None else now + deleted, now + delta, title, cwd),
        )
    con.commit()
    con.close()


def step(title, rows, wait):
    print(f"\n--- {title} ---")
    make_db(rows)
    time.sleep(wait)


def main():
    kill()
    time.sleep(1)
    shutil.rmtree(TDIR, ignore_errors=True)
    os.makedirs(os.path.join(TDIR, "assets"))
    shutil.copy(os.path.join(ROOT, "target", "release", "workbuddy-pet.exe"), TDIR)
    for n in ("idle.webp", "running.webp", "sleep.webp"):
        shutil.copy(os.path.join(ROOT, "assets", n), os.path.join(TDIR, "assets", n))

    env = dict(os.environ)
    env["WORKBUDDY_DB"] = DB
    proc = subprocess.Popen([EXE], cwd=TDIR, env=env)
    print(f"已启动 pid={proc.pid} WORKBUDDY_DB={DB}")

    step("① 只有已完成会话 → 期望 idle",
         [("a", "completed", None, -600_000, "", r"D:\proj\a"),
          ("b", "archived", None, -900_000, "", r"D:\proj\b")], 5)

    step("② 两条 working（心跳新鲜）+ 一条已删除的 working → 期望 working / active=2 / 标题=写文档",
         [("a", "completed", None, -600_000, "", r"D:\proj\a"),
          ("c", "working", None, -2_000, "写文档", r"D:\proj\doc"),
          ("d", "working", None, -300, "", r"D:\proj\deploy"),
          ("e", "working", -5_000, -1_000, "已删", r"D:\proj\dead")], 5)

    step("③ working 但心跳在 20 分钟前 → 期望 stale（悬挂保护）",
         [("c", "working", None, -20 * 60 * 1000, "卡住了", r"D:\proj\stuck")], 5)

    step("④ 库里没有任何会话 → 期望 idle", [], 5)

    print("\n--- ⑤ 删掉库文件 → 期望 error ---")
    for f in (DB, DB + "-wal", DB + "-shm"):
        if os.path.exists(f):
            os.remove(f)
    time.sleep(5)

    step("⑥ 重建库，回到 working → 期望恢复 working（标题取 cwd 末段）",
         [("c", "working", None, -500, "", r"D:\work\demo\workbuddy-pet-native")], 5)

    print("\n================ 日志 ================")
    with open(LOG, encoding="utf-8", errors="replace") as f:
        print(f.read())
    kill()


if __name__ == "__main__":
    os.environ["WORKBUDDY_DB"] = DB
    main()
