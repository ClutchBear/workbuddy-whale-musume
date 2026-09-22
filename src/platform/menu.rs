//! 右键菜单：菜单项 id（`M_*`）与弹出逻辑。

#![allow(unused_imports)]

use super::*;

/* 菜单命令 id */
pub(crate) const M_FEED: usize = 1;
pub(crate) const M_POKE: usize = 2;
pub(crate) const M_PRAISE: usize = 3;
pub(crate) const M_GAME: usize = 4;
pub(crate) const M_CATCH: usize = 5;
pub(crate) const M_GROW: usize = 6;
pub(crate) const M_SETTINGS: usize = 7;
pub(crate) const M_HOME: usize = 8;
pub(crate) const M_QUIT: usize = 9;
pub(crate) const M_SHOW: usize = 10;

/* ============================ 右键菜单 ============================ */

pub(crate) fn show_menu(hwnd: HWND, x: i32, y: i32) -> u32 {
    unsafe {
        let menu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => return 0,
        };
        let _ = AppendMenuW(menu, MF_STRING, M_FEED, w!("投喂小点心"));
        let _ = AppendMenuW(menu, MF_STRING, M_POKE, w!("戳一下"));
        let _ = AppendMenuW(menu, MF_STRING, M_PRAISE, w!("夸夸她"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, M_GAME, w!("小游戏：戳泡泡"));
        let _ = AppendMenuW(menu, MF_STRING, M_CATCH, w!("小游戏：接零食"));
        let _ = AppendMenuW(menu, MF_STRING, M_GROW, w!("成长面板"));
        let _ = AppendMenuW(menu, MF_STRING, M_SETTINGS, w!("设置"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, M_HOME, w!("回到原位"));
        let _ = AppendMenuW(menu, MF_STRING, M_SHOW, w!("隐藏 / 显示"));
        let _ = AppendMenuW(menu, MF_STRING, M_QUIT, w!("退出"));

        // 不调 SetForegroundWindow：对 NOACTIVATE/TOPMOST 工具窗激活流程可能死锁；
        // 菜单选择走鼠标 + TPM_RETURNCMD，无需前台。
        logf("menu: enter TrackPopupMenu (no foreground)");
        let r = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD, x, y, Some(0), hwnd, None);
        let r = r.0 as u32;
        logf(&format!("menu: TrackPopupMenu returned {r}"));
        let _ = DestroyMenu(menu);
        r
    }
}
