# Security Policy

## 数据与隐私

本程序 **无遥测、无上报、无后端**。所有状态只保存在本机。

| 数据 | 位置 | 说明 |
|---|---|---|
| 宠物养成存档 | exe 同目录 `whale-state.json` | 心情 / 好感 / 饱食 / 签到 / 成就等游戏数值，不含个人信息 |
| 运行日志 | exe 同目录 `workbuddy-pet.log` | 每次启动重写；设 `WORKBUDDY_PET_TRACE=1` 会输出更详细的诊断信息 |

## 读取的本机文件（只读）

- `%USERPROFILE%\.workbuddy\workbuddy.db` → `sessions` 表，用于判断「有没有任务在跑」。
  - 以 `SQLITE_OPEN_READ_ONLY` 打开；连不上时降级为 `immutable=1`。
  - **绝不写入**该数据库。可用环境变量 `WORKBUDDY_DB` 指向别处。

## 网络请求

- **默认零联网**：天气城市留空时不发起任何请求。
- 填写城市后，仅请求 [Open-Meteo](https://open-meteo.com) 两个免费接口：
  - `geocoding-api.open-meteo.com`（城市名 → 经纬度）
  - `api.open-meteo.com`（当前天气）
- 可选填入 Open-Meteo API Key（设置页），存在本地 `whale-state.json`，不上传到任何第三方。

## 凭据处理

仓库代码、脚本、配置中 **不包含任何 API Key、Token 或密码**。CI / 构建流程也不需要密钥。

## 漏洞报告

请通过本仓库的 GitHub Security Advisory 或 Issue 报告，尽量附上 Windows 版本、复现步骤和截图。
