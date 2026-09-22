# workbuddy-whale-musume（鲸鱼娘桌宠 · Rust 原生版 · 非官方移植）

WorkBuddy 桌宠「小鲸鱼娘」的 **非官方 Rust 原生移植版**，逐项对齐原版 Web 桌宠（[dsh-whale-musume](https://github.com/Sutera-Diffusus/dsh-whale-musume)）的动画、交互与玩法，但不依赖 WebView2 / 浏览器 / 任何运行时——一个约 2 MB 的单文件 exe + 一目录 WEBP 立绘。

- 技术栈：Rust + Win32 GDI（`UpdateLayeredWindow` 逐像素透明分层窗口）+ SQLite 直读
- 平台：Windows 10/11（x64），自动适配 DPI（Per-Monitor V2）
- 角色：`assets/*.webp` 全套立绘 90+ 张（待机 / 工作 / 表情包 / 节日 / 天气 / 日常），运行时直接读文件、**换图不用重编译**

---

## 一、功能总览

### 1. 桌宠本体
| 能力 | 说明 |
|---|---|
| 透明置顶 | 分层窗口逐像素 alpha，立绘无白底；**透明区域点击穿透**，不挡底下窗口 |
| 多显示器 | 停在鼠标所在显示器工作区右下角，拖过去会自动重新贴边 |
| 状态感知 | 直读 `%USERPROFILE%\.workbuddy\workbuddy.db`（只读），`working > 0` 切工作态，心跳超 10 分钟判悬挂切睡觉态 |
| 工作光晕 | 工作中角色身后一圈淡蓝色呼吸辉光（对齐原版） |
| 呼吸/摇摆 | 待机呼吸 3.4s + 左右摇摆 6s，微动作（hop/squint）每 18–28s 随机触发 |
| 待机大动作 | 35–60s 随机触发 teasing / 天气姿势等，停留 4.2–5.8s 回落 |
| 互动 | 摸头 / 拍肚 / 戳尾巴 / 点击弹跳 / 三连旋转；投喂🍰、夸夸✨、戳💢等有对应头顶表情与台词气泡 |
| 气泡 | pop 220ms 弹出、整体平滑淡出（无空心字伪影）、光标闪烁 |

### 2. 成长与养成（设置页进入）
- **今日任务 · 周签到**：每日任务领取、周一至周日签到、称号展示
- **成就墙**：成就全集，解锁进度 `x/N`
- **成长日记**：最近 12 条事件流水（摸头 / 升级 / 领奖……）
- 数值：羁绊等级 / 好感 / 心情 / 饱食度

### 3. 天气系统
- 城市配置（设置页 → 城市，留空 = 零联网）
- 数据源：[Open-Meteo](https://open-meteo.com) 免费接口（geocoding + forecast，无需 API Key，选填）
- 特效：雨 / 雪 / 雷电 / 伞 / 高温热浪 / 晴暖光晕 / 阴云 / 寒潮，配套天气立绘
- 每 10 分钟后台刷新；**测试连接**按钮异步检测（约 1 秒内回显结果）

### 4. 界面
- 右键菜单：工作台 / 打工 / 换装 / 投喂 / 设置 / 成长与养成 / 退出
- 设置、成就、日记、任务四页固定**浅色白底主题**（与系统右键菜单风格一致，不透明）
- 城市输入弹窗：淡蓝输入框 + 27% 轻遮罩，点框外自动保存
- 列表页标题栏 sticky 置顶，滚动内容从其下方穿过；「返回」回上一级
- exe 内嵌 idle-cute 立绘图标（`tools/pet.ico`，五档尺寸）

---

## 二、环境创建

### 1. 前置依赖（唯一硬要求）
- **Rust（MSVC 工具链）**：安装 [rustup](https://rustup.rs)，选 `x86_64-pc-windows-msvc`
  - 需要 Visual Studio Build Tools（含 Windows SDK，`link.exe` 与 `rc.exe` 来自这里）
- 无需 Node / Python / WebView2；SQLite 已通过 `rusqlite` 的 `bundled` feature 编进二进制

### 2. 克隆与依赖
```powershell
git clone https://github.com/ClutchBear/workbuddy-whale-musume.git
cd workbuddy-whale-musume
```
首次 `cargo build` 会自动拉取 crates.io 依赖（windows 0.61 / image 0.25 / rusqlite 0.40 / serde 1），无需手动安装。

### 3. 环境变量（全部可选）
| 变量 | 默认 | 说明 |
|---|---|---|
| `WORKBUDDY_DB` | `%USERPROFILE%\.workbuddy\workbuddy.db` | WorkBuddy 会话库路径；不存在也能跑（显示为空闲） |
| `WORKBUDDY_PET_STATE` | exe 同目录 `whale-state.json` | 成长数值存档位置 |
| `WORKBUDDY_PET_THEME` | 跟随系统深浅色 | 宠物页主题：`light` / `dark` |
| `WORKBUDDY_PET_TRACE` | 关 | 设为 `1` 打开详细日志（排查问题用） |

---

## 三、构建与部署

### 1. 编译
```powershell
cargo build --release
# 产物：target\release\workbuddy-pet.exe（约 2 MB，图标已内嵌）
```

### 2. 组装运行目录（dist 布局）
exe 要求 `assets/` 在**自己所在目录**下：
```
dist/
├── wb-pet.exe          ← 复制自 target\release\workbuddy-pet.exe
└── assets/             ← 仓库根 assets/ 全部内容 + assets/emoji/（Twemoji 头像）
```
```powershell
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\workbuddy-pet.exe dist\wb-pet.exe
Copy-Item -Recurse -Force assets dist\assets
```
> **为什么叫 `wb-pet.exe`**：WorkBuddy 主程序会按进程名 `workbuddy-pet.exe` 定期强杀同名进程；改名后即可共存。

### 3. 更换 exe 图标（可选）
图标源文件 `tools/pet.ico` 已通过 `build.rs` 链接（资源脚本 `tools/app.rc` → SDK `rc.exe` 编译的 `app.res` 会由 build 脚本自动重编）。想换成别的图：
```powershell
cargo run --release --example gen_icon   # 从 dist/assets/idle-cute.webp 重新生成 tools/pet.ico
```

### 4. 重新生成任务/成就数据（可选）
`src/data.rs` 由 `tools/gen-data.mjs`（Node）从原版数据生成；日常改动直接编辑 `src/data.rs` 即可，不跑 Node。

---

## 四、本地运行

```powershell
cd dist
.\wb-pet.exe        # 或直接双击
```
- **单实例**：重复双击会被命名互斥体拦下，第二个进程自动退出（日志记「已有 wb-pet 实例运行」）
- 立绘上**右键**打开菜单；透明空白处点击会穿透到下层窗口（设计如此）
- 左键点立绘 = 立即刷新一次状态
- 日志：exe 同目录 `workbuddy-pet.log`，每次启动重写

---

## 五、常见问题（FAQ）

**Q1：启动后什么都没有 / 立绘不显示？**
确认 exe 旁边有 `assets/` 目录且非空。日志里会写「扫描到 N 张立绘」；为 0 就是路径不对。

**Q2：桌宠被莫名杀掉？**
exe 必须改名（如 `wb-pet.exe`）。WorkBuddy 主程序会按进程名强杀 `workbuddy-pet.exe`。

**Q3：右键没反应？**
要点在立绘**不透明**的地方（透明区域是穿透的）。换装后体型变化会改变可点区域。

**Q4：天气测试连接一直没结果？**
1) 看日志 `weather_test:` 行（强制输出，不受 trace 开关限制）；2) 检查网络能否访问 `geocoding-api.open-meteo.com`；3) 城市名支持中文/拼音（Open-Meteo geocoding）。

**Q5：宠物后面出现灰色方框 / 跳动的线？**
这是天气特效，旧版本有「半透明色被预乘管线吃成灰」的渲染 bug，当前版本已修复。若仍复现请带 `WORKBUDDY_PET_TRACE=1` 的日志提 issue。

**Q6：任务栏/资源管理器里图标没更新？**
Windows 图标缓存，换个目录看或重启资源管理器即可。

**Q7：设置页文字发虚 / 看不清？**
旧版本存在 GDI 文字 alpha 重建 bug，重新拉取最新代码构建即可。

**Q8：怎么彻底退出？**
右键菜单「退出」，或点过窗口后按 ESC。

---

## 六、代码结构

```
src/
├── main.rs      # 窗口过程 / 消息循环 / 点击分发 / 定时器 / 单实例互斥
├── render.rs    # Canvas（预乘 alpha 软光栅）/ 主题 / 各页面绘制 / 天气特效
├── core.rs      # 动画状态机（姿态 / 情绪 / 气泡 / 换装时序）
├── assets.rs    # 立绘 / emoji 资源加载
├── state.rs     # 成长数值存档（serde JSON）
├── data.rs      # 任务 / 成就 / 称号静态数据
├── weather.rs   # Open-Meteo 客户端（WinHTTP，5s/10s 超时，后台线程）
tools/           # 图标资源、自测脚本、数据生成
examples/        # gen_icon.rs（ico 生成）、alpha_check.rs（alpha 通道验证）
```

## 七、许可与致谢

本项目以 **MIT 许可**发布，见 [`LICENSE`](LICENSE)。这是一个**非官方移植版**，与上游作者无隶属关系。

**上游**：[dsh-whale-musume](https://github.com/Sutera-Diffusus/dsh-whale-musume)（MIT，Copyright (c) 2026 Sutera-Diffusus）。

本项目复用了上游的以下内容，其原始版权声明已完整保留于 [`LICENSE-upstream`](LICENSE-upstream)：

| 复用什么 | 位置 |
|---|---|
| 全套立绘（92 张 WEBP，字节级同源） | `assets/**/*.webp`、`_selftest/assets/*.webp` |
| 任务 / 成就 / 称号 / 台词数据（由 `tools/gen-data.mjs` 从上游 `whale-moe-core.js` 原样导出） | `src/data.rs`（文件头已标注上游仓库地址） |
| 状态机语义与交互设计（Rust 重实现） | `src/core.rs` |

**其他第三方素材**：

- emoji 图标（236 张 PNG）来自 [Twemoji](https://github.com/jdecked/twemoji)（MIT）。
- exe 图标（`tools/pet.ico`）由上游立绘 `assets/idle-cute.webp` 裁剪生成。

**依赖库**：`windows` / `image` / `serde` / `serde_json`（MIT OR Apache-2.0）、`rusqlite` / `libsqlite3-sys`（MIT，内含 Public Domain 的 SQLite）。全部与 MIT 兼容，无 copyleft 传染。

