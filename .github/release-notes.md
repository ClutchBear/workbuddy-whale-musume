## 下载

**`workbuddy-whale-musume-*-win64.zip`** —— 解压即用，不需要装 Rust、不需要编译器。

1. 解压到任意位置（例如 `D:\wb-pet\`）
2. 双击 `wb-pet.exe`
3. 立绘上**右键**打开菜单（透明空白处是点击穿透的）

> ⚠️ 别只解压 exe —— `assets\` 目录必须和 exe 放在同一层，否则立绘不显示。

压缩包里还附了 `使用说明.txt`，含常见问题与故障排查。

## 系统要求

Windows 10 / 11 64 位。无需 .NET、无需 WebView2、无需任何运行时。

## 这个包是怎么来的

由 GitHub Actions 从本 tag 的源码在 `windows-latest` 上自动构建：

```
cargo build --release --locked
```

`wb-pet.exe` + `assets/` + `LICENSE` 打包成 zip。产物可复现，源码见仓库。

## 许可与归属

MIT。这是**非官方移植版**，与上游作者无隶属关系。

- 上游：[dsh-whale-musume](https://github.com/Sutera-Diffusus/dsh-whale-musume)（MIT，Copyright (c) 2026 Sutera-Diffusus）——全套立绘与任务/成就/台词数据来自上游
- emoji 图标：[Twemoji](https://github.com/jdecked/twemoji)（MIT）

完整归属见仓库 [README](https://github.com/ClutchBear/workbuddy-whale-musume#七许可与致谢)。
