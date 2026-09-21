/**
 * 从 dsh-whale-musume 的 whale-moe-core.js 原样导出数据，生成 Rust 源码。
 * 目的：台词库 / 成就 / 关键词这些大表手工抄写必然出错，用 Node 直接加载上游模块导出。
 *
 * 用法： node tools/gen-data.mjs <core.js 路径> <输出 rs 路径>
 */
import { createRequire } from "node:module";
import { writeFileSync, copyFileSync, mkdtempSync } from "node:fs";
import { resolve, join } from "node:path";
import { tmpdir } from "node:os";

const corePath = resolve(process.argv[2]);
const outPath = resolve(process.argv[3]);
const require = createRequire(import.meta.url);

// core.js 是 UMD：只有被当成 CommonJS 加载时才会走 module.exports 分支。
// 上游 package.json 声明了 "type": "module"，直接 require 会被当成 ESM（module 未定义），
// 所以先复制到临时 .cjs 再加载。
const tmpDir = mkdtempSync(join(tmpdir(), "whale-core-"));
const cjsPath = join(tmpDir, "whale-moe-core.cjs");
copyFileSync(corePath, cjsPath);
const core = require(cjsPath);

/** Rust 字符串字面量转义 */
function rs(s) {
  return (
    '"' +
    String(s)
      .replace(/\\/g, "\\\\")
      .replace(/"/g, '\\"')
      .replace(/\n/g, "\\n")
      .replace(/\r/g, "\\r")
      .replace(/\t/g, "\\t") +
    '"'
  );
}

function rsStrArray(name, arr, comment) {
  const lines = arr.map((s) => "    " + rs(s) + ",").join("\n");
  return `${comment ? `/// ${comment}\n` : ""}pub const ${name}: &[&str] = &[\n${lines}\n];\n`;
}

function rsBank(name, bank, comment) {
  const keys = Object.keys(bank);
  let out = `${comment ? `/// ${comment}\n` : ""}pub const ${name}: &[(&str, &[&str])] = &[\n`;
  for (const k of keys) {
    const items = bank[k].map((s) => "        " + rs(s) + ",").join("\n");
    out += `    (${rs(k)}, &[\n${items}\n    ]),\n`;
  }
  out += "];\n";
  return out;
}

let src = "";
src += "// 由 tools/gen-data.mjs 从 dsh-whale-musume/assets/whale-moe-core.js 自动生成，请勿手改。\n";
src += "// 上游仓库：https://github.com/Sutera-Diffusus/dsh-whale-musume\n\n";

/* ---------- LINES（状态机基础台词） ---------- */
let linesOut = "/// 状态机基础台词（core.LINES）\npub const LINES: &[(&str, &[&str])] = &[\n";
for (const k of Object.keys(core.LINES)) {
  const items = core.LINES[k].map((s) => "        " + rs(s) + ",").join("\n");
  linesOut += `    (${rs(k)}, &[\n${items}\n    ]),\n`;
}
linesOut += "];\n";
src += linesOut + "\n";

/* ---------- DIALOGUE（530+ 条） ---------- */
const d = core.DIALOGUE;
src += rsBank("DIALOGUE_DAILY", d.daily, "日常对话组（core.DIALOGUE.daily）") + "\n";
src += rsBank("DIALOGUE_WORK", d.work, "工作状态对话组") + "\n";
src += rsBank("DIALOGUE_INTERACT", d.interact, "互动对话组") + "\n";
src += rsBank("DIALOGUE_KEYWORD", d.keyword, "关键词回应组") + "\n";
src += rsBank("DIALOGUE_MEME", d.meme, "梗对话组") + "\n";
src += rsBank("DIALOGUE_CONTEXT", d.context, "按任务内容分类的贴题对话") + "\n";
src += rsBank("DIALOGUE_WEATHER", d.weather, "天气陪伴对话") + "\n";
src += rsBank("DIALOGUE_GREET", d.greet, "分时问候") + "\n";
src += rsBank("DIALOGUE_BOND", d.bond, "羁绊等级 / 心情分层台词") + "\n";
src += rsBank("DIALOGUE_PROACTIVE", d.proactive, "主动关怀（陪着，不是指挥）") + "\n";

/* ---------- ACHIEVEMENTS ---------- */
let ach = "/// 成就定义（core.ACHIEVEMENTS）。id / 图标 / 名称 / 说明\n";
ach += "pub const ACHIEVEMENTS: &[(&str, &str, &str, &str)] = &[\n";
for (const a of core.ACHIEVEMENTS) {
  ach += `    (${rs(a.id)}, ${rs(a.icon)}, ${rs(a.name)}, ${rs(a.desc)}),\n`;
}
ach += "];\n";
src += ach + "\n";

/* ---------- KEYWORDS ---------- */
let kw = "";
kw += "/// 关键词感知（core.KEYWORDS）：命中即变身表情包 / 给回应\n";
kw += "pub const KEYWORDS: &[(&str, &[&str])] = &[\n";
for (const k of core.KEYWORDS) {
  const items = k.words.map((w) => "        " + rs(w) + ",").join("\n");
  kw += `    (${rs(k.id)}, &[\n${items}\n    ]),\n`;
}
kw += "];\n";
src += kw + "\n";

/* ---------- TASK_TOPICS ----------
   core 没有导出 TASK_TOPICS，按其源码照抄（deploy/bug/data/code/write/research）。 */
src += `/// 任务内容分类（core.TASK_TOPICS）：空闲闲聊时按当前任务贴题
pub const TASK_TOPICS: &[(&str, &[&str])] = &[
    ("deploy", &["部署", "上线", "发布", "deploy", "release", "docker", "kubernetes", "k8s", "服务器", "nginx", "环境"]),
    ("bug", &["报错", "error", "bug", "崩溃", "闪退", "异常", "修复", "fix", "调试", "debug", "失败", "warning", "警告"]),
    ("data", &["数据", "表格", "excel", "csv", "json", "统计", "分析", "图表", "清洗", "数据库", "sql", "可视化"]),
    ("code", &["代码", "函数", "变量", "class", "python", "javascript", "typescript", "react", "vue", "java", "golang", "rust", "算法", "接口", "api", "重构", "编译", "前端", "后端", "组件", "脚本", "npm", "git"]),
    ("write", &["写一", "文案", "文章", "报告", "翻译", "润色", "总结", "邮件", "文档", "周报", "标题", "大纲"]),
    ("research", &["调研", "搜索", "资料", "原理", "是什么", "为什么", "如何", "区别", "比较", "最新", "论文", "介绍一下", "有哪些"]),
];
`;

/* QUEST_POOL 单独处理（含 reward） */
let qp = "/// 每日任务池（core.QUEST_POOL）\n";
qp += "pub const QUEST_POOL: &[(&str, &str, &str, i32, i32, i32, bool)] = &[\n";
for (const q of core.QUEST_POOL) {
  qp += `    (${rs(q.id)}, ${rs(q.desc)}, ${rs(q.metric)}, ${q.target}, ${q.reward.affinity}, ${q.reward.mood}, ${q.always === true}),\n`;
}
qp += "];\n";
src += qp + "\n";

/* ---------- WEATHER_MAP ---------- */
// WEATHER_MAP 没导出，从 weatherText 反推：遍历 0..99 的 WMO 码
let wm = "/// WMO 天气码 → (emoji, 中文, kind)\n";
wm += "pub const WEATHER_MAP: &[(i32, &str, &str, &str)] = &[\n";
const seen = new Map();
for (let code = 0; code <= 99; code += 1) {
  const t = core.weatherText(code);
  if (t.kind === "unknown") continue;
  if (seen.has(String(code))) continue;
  seen.set(String(code), true);
  wm += `    (${code}, ${rs(t.emoji)}, ${rs(t.label)}, ${rs(t.kind)}),\n`;
}
wm += "];\n";
src += wm + "\n";

/* ---------- HIT_ZONES ---------- */
const hz = core.HIT_ZONES;
// Rust 的 f32 字面量必须有小数点，0 / 1 直接输出会被当成整数
const fl = (n) => (Number.isInteger(n) ? n.toFixed(1) : String(n));
let z = "/// 互动分区（core.HIT_ZONES.full）：顺序判定 tail > head > belly，未命中回落 head\n";
z += "pub const HIT_ZONES: &[(&str, f32, f32, f32, f32)] = &[\n";
for (const s of hz.full) {
  z += `    (${rs(s.id)}, ${fl(s.x0)}, ${fl(s.y0)}, ${fl(s.x1)}, ${fl(s.y1)}),\n`;
}
z += "];\n";
src += z + "\n";

/* ---------- 待机小动作 / 工具细分姿态（来自表现层常量，core 未导出） ---------- */
src += rsStrArray(
  "IDLE_ACTION_POOL",
  [
    "daily-eat", "daily-coffee", "daily-stretch", "daily-pajama", "daily-shower",
    "cool-shades", "meme-smug", "daily-picnic", "daily-cooking", "daily-fishing",
    "daily-painting", "daily-gaming", "tail-swing", "meme-music",
  ],
  "待机小动作池（表现层 IDLE_ACTION_POOL）；羁绊 Lv3 解锁后追加 wink"
) + "\n";

src += `/// 工具细分姿态（表现层 TOOL_POSES）：复用已有 work-* 立绘，识别不了回落 running
pub const TOOL_POSES: &[(&str, &str, &[&str])] = &[
    ("deploy", "work-deploy", &["deploy", "rollout", "发布", "上线"]),
    ("test", "work-review", &["test", "vitest", "jest", "pytest", "测试", "跑测试"]),
    ("debug", "work-debug", &["debug", "traceback", "报错", "修 bug", "fix bug"]),
    ("search", "work-idea", &["search", "grep", "glob", "ripgrep", "搜索", "查找"]),
    ("write", "work-meeting", &["write", "edit", "patch", "写入", "编辑", "修改文件"]),
    ("bash", "work-slack-phone", &["bash", "shell", "terminal", "npm ", "pnpm ", "git ", "命令"]),
    ("review", "work-review", &["review", "diff", "评审", "审查"]),
    ("plan", "work-idea", &["plan", "todo", "计划"]),
];
`;

writeFileSync(outPath, src, "utf8");
const total = Object.values(d).reduce((n, g) => n + Object.values(g).reduce((m, a) => m + a.length, 0), 0);
console.log(`已生成 ${outPath}`);
console.log(`  台词：DIALOGUE ${core.dialogueCount()} 条（其中四大组 ${total}）+ LINES ${Object.values(core.LINES).reduce((n, a) => n + a.length, 0)} 条`);
console.log(`  成就 ${core.ACHIEVEMENTS.length} 个 / 关键词 ${core.KEYWORDS.length} 组 / 任务池 ${core.QUEST_POOL.length} 个 / 天气码 ${seen.size} 个`);
