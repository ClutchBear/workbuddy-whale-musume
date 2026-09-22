# workbuddy-whale-musume（鲸鱼娘桌宠 · Rust 原生版 · 非官方移植）

[![构建状态](https://github.com/ClutchBear/workbuddy-whale-musume/actions/workflows/build.yml/badge.svg)](https://github.com/ClutchBear/workbuddy-whale-musume/actions/workflows/build.yml)

WorkBuddy 桌宠「小鲸鱼娘」的 **非官方 Rust 原生移植版**，逐项对齐原版 Web 桌宠（[dsh-whale-musume](https://github.com/Sutera-Diffusus/dsh-whale-musume)）的动画、交互与玩法，但不依赖 WebView2 / 浏览器 / 任何运行时——一个约 2 MB 的单文件 exe + 一目录 WEBP 立绘。

> **与上游的关系**：本项目由第三方（ClutchBear）独立开发，**非上游官方发布，与 dsh-whale-musume 作者无隶属关系**。「鲸鱼娘」为上游自创角色名，本项目沿用仅为说明移植来源。项目名、图标、立绘均取自上游（MIT），完整归属见 [第七节](#七许可与致谢)。

- 技术栈：Rust + Win32 GDI（`UpdateLayeredWindow` 逐像素透明分层窗口）+ SQLite 直读
- 平台：Windows 10/11（x64），自动适配 DPI（Per-Monitor V2）
- 角色：`assets/*.webp` 全套立绘 90+ 张（待机 / 工作 / 表情包 / 节日 / 天气 / 日常），运行时直接读文件、**换图不用重编译**

---

## 预览

![鲸鱼娘工作态：抱着笔记本电脑，身后一圈淡蓝呼吸辉光](preview.png)

> **真实截图，不是效果图。** 由本项目 exe 在 Windows 11 上实际运行时抓取的窗口画面 —— 当前是**工作态**（检测到 WorkBuddy 在运行时自动切换，对齐原版的淡蓝呼吸辉光），其余状态见[功能总览](#一功能总览)。

---

## 直接下载（免编译）

不想装 Rust 工具链的话，直接拿现成的包：

**→ 到 [Releases 页面](https://github.com/ClutchBear/workbuddy-whale-musume/releases) 下载 `workbuddy-whale-musume-*-win64.zip`**

1. 解压到任意位置（例如 `D:\wb-pet\`）
2. 双击 `wb-pet.exe`
3. 立绘上**右键**打开菜单（透明空白处是点击穿透的）

> ⚠️ 别只解压 exe —— `assets\` 目录必须和 exe 在同一层，否则立绘不显示。

压缩包由 GitHub Actions 在 `windows-latest` 上从源码自动构建，见 [`.github/workflows/build.yml`](.github/workflows/build.yml)，产物可复现。想自己编译见 [第三节](#三构建与部署)。

> 没有 Release 时，也可以在 [Actions 页面](https://github.com/ClutchBear/workbuddy-whale-musume/actions/workflows/build.yml) 找最近一次成功构建，下载 artifact（需登录 GitHub）。

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
exe 图标通过 `build.rs` 链接 `tools/app.res` 实现。**`app.res` 已入库**，所以即便没装 Windows SDK 也能正常构建（否则全新 clone / CI 会因链接器报 `LNK1181` 直接失败）。

流程：改 `tools/pet.ico` 或 `tools/app.rc` → 用 SDK 的 `rc.exe` **手动重新生成 `app.res`** → 重新编译。

```powershell
# 1) 换图标源图：从立绘生成 pet.ico（或直接替换 tools/pet.ico）
cargo run --release --example gen_icon   # 从 dist/assets/idle-cute.webp 重新生成 tools/pet.ico

# 2) 重新编译资源（需要 Windows SDK 的 rc.exe；版本号按本机实际路径调整）
& "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\rc.exe" /nologo /fo tools\app.res tools\app.rc

# 3) 重新构建
cargo build --release
```
> `build.rs` 只负责把 `app.res` 路径传给链接器，**不会**替你编译 `.rc` —— 漏了第 2 步图标不会更新。
> 万一 `app.res` 被删，构建会给出 cargo 警告并降级为「无图标的 exe」，不会直接失败。

### 4. 重新生成任务/成就数据（可选）
`src/data.rs` 由 `tools/gen-data.mjs`（Node）从原版数据生成；日常改动直接编辑 `src/data.rs` 即可，不跑 Node。

### 5. 自动构建与发布（GitHub Actions）
`.github/workflows/build.yml` 会在推送到 `main` 或手动触发时编译打包，并上传为 artifact。推一个 `v*` 标签（如 `v0.3.0`）则会额外把 zip 挂到 Release 上：

```powershell
git tag v0.3.0
git push origin v0.3.0
```

打出的 zip 结构等于 `dist/` 布局，外加许可声明与使用说明：

```
workbuddy-whale-musume-v0.3.0-win64.zip
├── wb-pet.exe
├── assets/                      (92 立绘 + 236 emoji)
├── 使用说明.txt
├── LICENSE / LICENSE-upstream / THIRD-PARTY-LICENSES.md
```

发布前会自动做三重校验：exe 体积与立绘/emoji 数量是否正常、有无运行期产物（`whale-state.json` / `*.log`）混入、zip 条目名是否全为正斜杠。

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

**Q：一定要自己装 Rust 编译吗？**
不用。到 [Releases](https://github.com/ClutchBear/workbuddy-whale-musume/releases) 下载现成的 zip，解压双击即可，见[开头的下载说明](#直接下载免编译)。

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
├── main.rs         # 只做装配：crate 属性 + mod 声明 + fn main()
├── log.rs          # 日志（写 exe 同目录 workbuddy-pet.log）
├── platform/       # 平台原语（Win32），**不认识 App**
│   ├── mod.rs      #   时间 / 显示器工作区 / 默认落点
│   ├── menu.rs     #   右键菜单与菜单项 id（M_*）
│   └── tray.rs     #   托盘图标（添加 / 更新 / 移除 / HICON 生成）
├── app/            # 应用层（App 定义在 mod.rs）
│   ├── mod.rs      #   App 结构体、消息常量、全局槽、子模块装配
│   ├── boot.rs     #   单实例 / 建窗 / 起线程 / 开场与跨日
│   ├── sample.rs   #   WorkBuddy 会话库只读采样
│   ├── speech.rs   #   气泡打字机 / 台词挑选 / 姿势与情绪
│   ├── events.rs   #   事件 → 成长值 / 任务 / 成就 / 主动行为
│   ├── fx.rs       #   粒子与头顶大表情
│   ├── interact.rs #   点击吉祥物 / 菜单命令 / 键盘与文本编辑
│   ├── pointer.rs  #   拖动 / 甩动惯性 / 点击分发 / 窗口显隐
│   ├── game.rs     #   小游戏
│   ├── tick.rs     #   主循环（状态机推进与各项定时）
│   ├── paint.rs    #   重绘 / 贴屏（UpdateLayeredWindow）/ 列表页滚动上限
│   ├── physics.rs  #   速度采样 / 缓动 / 松手回弹
│   └── wndproc.rs  #   窗口过程与消息分发（含同线程重入守卫）
├── render.rs       # Canvas（预乘 alpha 软光栅）/ 主题 / 各页面绘制 / 天气特效
├── core.rs         # 动画状态机（姿态 / 情绪 / 气泡 / 换装时序）
├── assets.rs       # 立绘 / emoji 资源加载
├── state.rs        # 成长数值存档（serde JSON）
├── data.rs         # 任务 / 成就 / 称号静态数据
├── weather.rs      # Open-Meteo 客户端（WinHTTP，5s/10s 超时，后台线程）
tools/              # 图标资源、自测脚本、数据生成
examples/           # gen_icon.rs（ico 生成）、alpha_check.rs（alpha 通道验证）
```

分层约定：`platform/` 只依赖 Win32，不依赖应用状态；`app/` 的各子模块都是
`App` 定义处的**后代模块**——按 Rust 的可见性规则天然能访问它的私有字段，
所以 `App` 的 71 个字段不必逐个 `pub`，字段访问路径保持扁平（`app.pose` 而非
`app.anim.pose`）。收益是：改气泡动画时，`speech.rs` / `paint.rs` 里的函数
不再和拖拽惯性、成就计算挤在同一个 2531 行文件里。

## 七、许可与致谢

本项目以 **MIT 许可**发布，见 [`LICENSE`](LICENSE)。这是一个**非官方移植版**，与上游作者无隶属关系。

**上游**：[dsh-whale-musume](https://github.com/Sutera-Diffusus/dsh-whale-musume)（MIT，Copyright (c) 2026 Sutera-Diffusus）。

本项目复用了上游的以下内容，其原始版权声明已完整保留于 [`LICENSE-upstream`](LICENSE-upstream)：

| 复用什么 | 位置 |
|---|---|
| 全套立绘（92 张 WEBP，字节级同源） | `assets/*.webp`、`_selftest/assets/*.webp`（自测夹具）、`dist/assets/*`（运行时副本，不入库） |
| 预览截图（`preview.png` 实拍自本项目运行画面，画面中的立绘版权归上游） | `preview.png` |
| 任务 / 成就 / 称号 / 台词数据（由 `tools/gen-data.mjs` 从上游 `whale-moe-core.js` 原样导出） | `src/data.rs`（文件头已标注上游仓库地址） |
| 状态机语义与交互设计（Rust 重实现） | `src/core.rs` |

**其他第三方素材**：

- emoji 图标（236 张 PNG）来自 [Twemoji](https://github.com/jdecked/twemoji)（MIT）。
- exe 图标（`tools/pet.ico`）由上游立绘 `assets/idle-cute.webp` 裁剪生成。

**依赖库**：`windows` / `image` / `serde` / `serde_json`（MIT OR Apache-2.0）、`rusqlite` / `libsqlite3-sys`（MIT，内含 Public Domain 的 SQLite）。全部与 MIT 兼容，无 copyleft 传染。

**完整依赖许可清单**（69 个 crate 逐项列出，含运行时/构建期分类）见 [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md)。


