# 第三方许可清单（THIRD-PARTY LICENSES）

本文件列出 `workbuddy-whale-musume` 直接与间接使用的全部 Rust 依赖及其许可证。
数据由 `cargo metadata` 生成，版本以 `Cargo.lock` 为准。

## 汇总

| 许可证 | 数量 |
|---|---|
| `MIT OR Apache-2.0` | 47 |
| `MIT` | 6 |
| `MIT/Apache-2.0` | 4 |
| `Unlicense OR MIT` | 2 |
| `Zlib` | 2 |
| `MIT OR Zlib OR Apache-2.0` | 2 |
| `BSD-3-Clause OR Apache-2.0` | 2 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `Apache-2.0 OR MIT` | 1 |
| `Zlib OR Apache-2.0 OR MIT` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |
| **合计** | **69** |

**结论：全部为宽松许可（MIT / Apache-2.0 / BSD-3-Clause / Zlib / 0BSD / Unlicense），
无 GPL / AGPL / SSPL / CC-BY-NC 等 copyleft 或禁止商用条款，与本项目的 MIT 许可完全兼容。**

## 运行时依赖（会被编进二进制）

| 包 | 版本 | 许可证 |
|---|---|---|
| `adler2` | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| `bitflags` | 2.13.2 | MIT OR Apache-2.0 |
| `bytemuck` | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| `byteorder-lite` | 0.1.0 | Unlicense OR MIT |
| `cfg-if` | 1.0.5 | MIT OR Apache-2.0 |
| `crc32fast` | 1.5.2 | MIT OR Apache-2.0 |
| `fallible-iterator` | 0.3.0 | MIT/Apache-2.0 |
| `fallible-streaming-iterator` | 0.1.9 | MIT/Apache-2.0 |
| `fdeflate` | 0.3.7 | MIT OR Apache-2.0 |
| `flate2` | 1.1.10 | MIT OR Apache-2.0 |
| `foldhash` | 0.2.0 | Zlib |
| `hashbrown` | 0.16.1 | MIT OR Apache-2.0 |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 |
| `hashlink` | 0.12.2 | MIT OR Apache-2.0 |
| `image` | 0.25.10 | MIT OR Apache-2.0 |
| `image-webp` | 0.2.4 | MIT OR Apache-2.0 |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 |
| `libsqlite3-sys` | 0.38.2 | MIT |
| `memchr` | 2.8.3 | Unlicense OR MIT |
| `miniz_oxide` | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| `miniz_oxide` | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| `moxcms` | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| `num-traits` | 0.2.19 | MIT OR Apache-2.0 |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 |
| `png` | 0.18.1 | MIT OR Apache-2.0 |
| `pxfm` | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| `rsqlite-vfs` | 0.1.1 | MIT |
| `rusqlite` | 0.40.2 | MIT |
| `serde` | 1.0.229 | MIT OR Apache-2.0 |
| `serde_core` | 1.0.229 | MIT OR Apache-2.0 |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 |
| `simd-adler32` | 0.3.10 | MIT |
| `smallvec` | 1.16.1 | MIT OR Apache-2.0 |
| `sqlite-wasm-rs` | 0.5.5 | MIT |
| `windows` | 0.61.3 | MIT OR Apache-2.0 |
| `windows-collections` | 0.2.0 | MIT OR Apache-2.0 |
| `windows-core` | 0.61.2 | MIT OR Apache-2.0 |
| `windows-future` | 0.2.1 | MIT OR Apache-2.0 |
| `windows-implement` | 0.60.2 | MIT OR Apache-2.0 |
| `windows-interface` | 0.59.3 | MIT OR Apache-2.0 |
| `windows-link` | 0.1.3 | MIT OR Apache-2.0 |
| `windows-numerics` | 0.2.0 | MIT OR Apache-2.0 |
| `windows-result` | 0.3.4 | MIT OR Apache-2.0 |
| `windows-strings` | 0.4.2 | MIT OR Apache-2.0 |
| `windows-threading` | 0.1.0 | MIT OR Apache-2.0 |
| `zmij` | 1.0.23 | MIT |

## 构建期依赖（仅在编译时使用）

| 包 | 版本 | 许可证 |
|---|---|---|
| `autocfg` | 1.5.1 | Apache-2.0 OR MIT |
| `bumpalo` | 3.20.3 | MIT OR Apache-2.0 |
| `cc` | 1.4.7 | MIT OR Apache-2.0 |
| `find-msvc-tools` | 0.1.13 | MIT OR Apache-2.0 |
| `js-sys` | 0.3.105 | MIT OR Apache-2.0 |
| `pkg-config` | 0.3.34 | MIT OR Apache-2.0 |
| `proc-macro2` | 1.0.107 | MIT OR Apache-2.0 |
| `quick-error` | 2.0.1 | MIT/Apache-2.0 |
| `quote` | 1.0.47 | MIT OR Apache-2.0 |
| `rustversion` | 1.0.23 | MIT OR Apache-2.0 |
| `serde_derive` | 1.0.229 | MIT OR Apache-2.0 |
| `shlex` | 2.0.1 | MIT OR Apache-2.0 |
| `syn` | 2.0.119 | MIT OR Apache-2.0 |
| `syn` | 3.0.6 | MIT OR Apache-2.0 |
| `thiserror` | 2.0.20 | MIT OR Apache-2.0 |
| `thiserror-impl` | 2.0.20 | MIT OR Apache-2.0 |
| `unicode-ident` | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `vcpkg` | 0.2.15 | MIT/Apache-2.0 |
| `wasm-bindgen` | 0.2.128 | MIT OR Apache-2.0 |
| `wasm-bindgen-macro` | 0.2.128 | MIT OR Apache-2.0 |
| `wasm-bindgen-macro-support` | 0.2.128 | MIT OR Apache-2.0 |
| `wasm-bindgen-shared` | 0.2.128 | MIT OR Apache-2.0 |
| `zlib-rs` | 0.6.8 | Zlib |

## 关于 SQLite

`rusqlite` 通过 `libsqlite3-sys` 的 `bundled` 特性把 SQLite 源码一并编入二进制。
SQLite 本体处于 **Public Domain**（见 <https://www.sqlite.org/copyright.html>），
`libsqlite3-sys` crate 本身为 MIT 许可。

## 非 Rust 素材

| 素材 | 来源 | 许可证 |
|---|---|---|
| 立绘 92 张 WEBP、任务/成就/台词数据 | [dsh-whale-musume](https://github.com/Sutera-Diffusus/dsh-whale-musume) | MIT（Copyright (c) 2026 Sutera-Diffusus，全文见 `LICENSE-upstream`） |
| emoji 图标 236 张 PNG | [Twemoji](https://github.com/jdecked/twemoji) | MIT |

## 重新生成

```bash
cargo metadata --format-version 1 --all-features > _tmp/meta.json
# 然后按本文件的脚本逻辑重新渲染（或直接审阅 meta.json 的 packages[].license 字段）
```

