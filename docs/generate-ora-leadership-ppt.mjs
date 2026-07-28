/**
 * Ora 领导汇报 PPT 生成脚本
 * 严格依据 docs/ora-leadership-demo-ppt-brief.md
 * 配色遵循华为 VI：华为红点缀（一点红）+ 浅色商务底 + 灰度文字
 */
import PptxGenJS from "pptxgenjs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.join(__dirname, "Ora-领导汇报-第一阶段成果.pptx");

/** 华为规范倾向色板：红只作强调，不大面积铺底 */
const C = {
  red: "CE0E2D",
  redSoft: "F8E8EB",
  black: "231815",
  text: "2D2D2D",
  gray1: "4A4A4A",
  gray2: "6B6B6B",
  gray3: "8C8C8C",
  line: "D9D9D9",
  bg: "FFFFFF",
  bgSoft: "F5F5F5",
  bgCard: "FAFAFA",
  layer1: "FFF5F6",
  layer2: "F0F4F8",
  layer3: "F5F7FA",
  layer4: "F7F7F7",
  layer5a: "EEF6F2",
  layer5b: "F3F0F7",
  warn: "E67E22",
  ok: "2E7D32",
  white: "FFFFFF",
  blueSoft: "E8EEF5",
};

const FONT = "Microsoft YaHei";
const W = 13.333;
const H = 7.5;

const pptx = new PptxGenJS();
pptx.defineLayout({ name: "WIDE", width: W, height: H });
pptx.layout = "WIDE";
pptx.author = "Ora Team";
pptx.title = "Ora —— 面向 AI Agent 的 IDE｜第一阶段成果汇报";
pptx.subject = "华为内部领导汇报";

function addSlide() {
  return pptx.addSlide();
}

/** 页眉红线 + 页码 */
function chrome(slide, pageNum, total = 24) {
  slide.addShape(pptx.shapes.RECTANGLE, {
    x: 0,
    y: 0,
    w: W,
    h: 0.06,
    fill: { color: C.red },
    line: { color: C.red },
  });
  slide.addText(`${pageNum} / ${total}`, {
    x: W - 1.4,
    y: H - 0.38,
    w: 1.1,
    h: 0.28,
    fontSize: 10,
    fontFace: FONT,
    color: C.gray3,
    align: "right",
  });
}

function pageTitle(slide, title, y = 0.28) {
  slide.addText(title, {
    x: 0.55,
    y,
    w: 11.5,
    h: 0.45,
    fontSize: 24,
    fontFace: FONT,
    bold: true,
    color: C.black,
  });
  slide.addShape(pptx.shapes.RECTANGLE, {
    x: 0.55,
    y: y + 0.48,
    w: 0.55,
    h: 0.05,
    fill: { color: C.red },
    line: { color: C.red },
  });
}

function bodyText(slide, text, opts = {}) {
  slide.addText(text, {
    fontFace: FONT,
    color: C.text,
    fontSize: 14,
    ...opts,
  });
}

function bullet(slide, items, opts) {
  slide.addText(
    items.map((t) => ({
      text: t,
      options: { breakLine: true },
    })),
    {
      fontFace: FONT,
      color: C.text,
      fontSize: 14,
      paraSpacing: 8,
      bullet: { type: "bullet" },
      ...opts,
    },
  );
}

function card(slide, x, y, w, h, fill = C.bgCard) {
  slide.addShape(pptx.shapes.ROUNDED_RECTANGLE, {
    x,
    y,
    w,
    h,
    fill: { color: fill },
    line: { color: C.line, width: 1 },
    rectRadius: 0.08,
  });
}

function quoteBar(slide, text, x, y, w) {
  slide.addShape(pptx.shapes.RECTANGLE, {
    x,
    y,
    w: 0.08,
    h: 0.55,
    fill: { color: C.red },
    line: { color: C.red },
  });
  slide.addText(text, {
    x: x + 0.2,
    y,
    w: w - 0.2,
    h: 0.55,
    fontFace: FONT,
    fontSize: 13,
    italic: true,
    color: C.red,
    valign: "middle",
  });
}

// ═══════════════════════════════════════════════════════════
// P01 封面
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  s.addShape(pptx.shapes.RECTANGLE, {
    x: 0,
    y: 0,
    w: W,
    h: H,
    fill: { color: C.bg },
    line: { color: C.bg },
  });
  s.addShape(pptx.shapes.RECTANGLE, {
    x: 0,
    y: 0,
    w: 0.18,
    h: H,
    fill: { color: C.red },
    line: { color: C.red },
  });
  s.addText("第一阶段成果汇报", {
    x: 0.9,
    y: 1.5,
    w: 10,
    h: 0.35,
    fontFace: FONT,
    fontSize: 14,
    color: C.red,
    bold: true,
  });
  s.addText("Ora —— 面向 AI Agent 的 IDE", {
    x: 0.9,
    y: 2.0,
    w: 11,
    h: 0.7,
    fontFace: FONT,
    fontSize: 36,
    bold: true,
    color: C.black,
  });
  s.addText("打破 Agent 孤岛，打通华为研发全链路", {
    x: 0.9,
    y: 2.75,
    w: 11,
    h: 0.4,
    fontFace: FONT,
    fontSize: 20,
    color: C.gray1,
  });
  s.addText("书同文 · 车同轨 · 万物皆可插件 · 欢迎加入并行世界", {
    x: 0.9,
    y: 3.4,
    w: 11,
    h: 0.35,
    fontFace: FONT,
    fontSize: 14,
    color: C.gray2,
  });
  s.addShape(pptx.shapes.RECTANGLE, {
    x: 0.9,
    y: 4.0,
    w: 2.2,
    h: 0.04,
    fill: { color: C.red },
    line: { color: C.red },
  });
  s.addText(
    "桌面端 · Web ·（移动端可扩展）｜Rust 内核｜TypeScript 插件｜一插件一进程",
    {
      x: 0.9,
      y: 4.3,
      w: 11,
      h: 0.35,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray2,
    },
  );
  s.addText("华为内部业务 / 技术领导汇报 ｜ 约 20–25 分钟", {
    x: 0.9,
    y: 6.6,
    w: 11,
    h: 0.3,
    fontFace: FONT,
    fontSize: 12,
    color: C.gray3,
  });
}

// ═══════════════════════════════════════════════════════════
// P02 目录
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 2);
  pageTitle(s, "目录");
  const items = [
    ["01", "当前 AI 落地困境", "Agent 孤岛 · 系统林立 · 流程鸿沟 · 万国造 · 界面与并行"],
    ["02", "Ora 怎么解", "五招破局：三体世界 · IPD · 纳管 · Beauty · 并行车道"],
    ["03", "Ora 是什么 & 架构", "产品能力 · 华为规范五层架构 · 领域模型"],
    ["04", "两个演示场景", "插件/Skill 纳管 · 华为开发上库闭环"],
    ["05", "总结与支持诉求", "第一阶段成果回扣 · 请领导支持"],
  ];
  items.forEach((it, i) => {
    const y = 1.15 + i * 1.05;
    s.addText(it[0], {
      x: 0.7,
      y,
      w: 1.0,
      h: 0.55,
      fontFace: FONT,
      fontSize: 28,
      bold: true,
      color: C.red,
    });
    s.addText(it[1], {
      x: 1.9,
      y,
      w: 9,
      h: 0.35,
      fontFace: FONT,
      fontSize: 18,
      bold: true,
      color: C.black,
    });
    s.addText(it[2], {
      x: 1.9,
      y: y + 0.35,
      w: 10,
      h: 0.3,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray2,
    });
  });
}

// ═══════════════════════════════════════════════════════════
// P03 困境总览
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 3);
  pageTitle(s, "困境总览：天顶星科技，还在拼刺刀");
  s.addText("不是模型不够强，是协同方式、流程与工具仍在「拼刺刀」。", {
    x: 0.55,
    y: 0.95,
    w: 12,
    h: 0.35,
    fontFace: FONT,
    fontSize: 14,
    color: C.gray1,
  });
  const pains = [
    ["1", "Agent 孤岛", "协同断裂"],
    ["2", "系统林立", "学习成本高"],
    ["3", "流程鸿沟", "水土不服"],
    ["4", "万国造 Skill/MCP", "难复用难治理"],
    ["5", "Agent 工具负担", "配置成负担"],
    ["6", "原始界面", "硬核≠好用"],
    ["7", "工作方式落后", "Token 跑不满"],
  ];
  pains.forEach((p, i) => {
    const col = i % 4;
    const row = Math.floor(i / 4);
    const x = 0.55 + col * 3.1;
    const y = 1.5 + row * 2.0;
    card(s, x, y, 2.9, 1.7, i === 6 ? C.redSoft : C.bgCard);
    s.addText(p[0], {
      x: x + 0.2,
      y: y + 0.25,
      w: 0.5,
      h: 0.4,
      fontFace: FONT,
      fontSize: 22,
      bold: true,
      color: C.red,
    });
    s.addText(p[1], {
      x: x + 0.2,
      y: y + 0.7,
      w: 2.5,
      h: 0.35,
      fontFace: FONT,
      fontSize: 15,
      bold: true,
      color: C.black,
    });
    s.addText(p[2], {
      x: x + 0.2,
      y: y + 1.1,
      w: 2.5,
      h: 0.3,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray2,
    });
  });
  quoteBar(s, "明明是天顶星科技，还在拼刺刀", 0.55, 6.55, 11);
}

// ═══════════════════════════════════════════════════════════
// P04 Agent 孤岛（上）
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 4);
  pageTitle(s, "困境：Agent 孤岛（上）—— 开发连贯性被强制打断");
  const points = [
    {
      t: "任务杂、场景多",
      d: "单一 Agent 难以覆盖复杂场景，需要多种异构 Agent 协同工作。",
    },
    {
      t: "百花齐放却无法协同",
      d: "做一个功能，开发者在多个 Agent 之间来回拷贝数据 → 开发连贯性被强制打断。",
    },
    {
      t: "共享靠污染",
      d: "多个 Agent 共用同一工作目录才能共享 → 极易造成目录污染。",
    },
  ];
  points.forEach((p, i) => {
    const y = 1.1 + i * 1.45;
    card(s, 0.55, y, 7.2, 1.3);
    s.addShape(pptx.shapes.OVAL, {
      x: 0.75,
      y: y + 0.4,
      w: 0.45,
      h: 0.45,
      fill: { color: C.red },
      line: { color: C.red },
    });
    s.addText(String(i + 1), {
      x: 0.75,
      y: y + 0.45,
      w: 0.45,
      h: 0.35,
      fontFace: FONT,
      fontSize: 14,
      bold: true,
      color: C.white,
      align: "center",
    });
    s.addText(p.t, {
      x: 1.4,
      y: y + 0.2,
      w: 6,
      h: 0.35,
      fontFace: FONT,
      fontSize: 16,
      bold: true,
      color: C.black,
    });
    s.addText(p.d, {
      x: 1.4,
      y: y + 0.6,
      w: 6,
      h: 0.5,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray1,
    });
  });
  // 右侧示意：开发者在 A/B/C 间拷贝
  card(s, 8.05, 1.1, 4.7, 5.0, C.bgSoft);
  s.addText("示意图：窗口间拷贝", {
    x: 8.25,
    y: 1.3,
    w: 4.3,
    h: 0.35,
    fontFace: FONT,
    fontSize: 13,
    bold: true,
    color: C.gray1,
  });
  ["Agent A", "Agent B", "Agent C"].forEach((name, i) => {
    const y = 1.9 + i * 1.2;
    s.addShape(pptx.shapes.ROUNDED_RECTANGLE, {
      x: 8.5,
      y,
      w: 3.8,
      h: 0.7,
      fill: { color: C.white },
      line: { color: C.line },
      rectRadius: 0.06,
    });
    s.addText(name, {
      x: 8.5,
      y: y + 0.18,
      w: 3.8,
      h: 0.35,
      fontFace: FONT,
      fontSize: 14,
      align: "center",
      color: C.black,
    });
    if (i < 2) {
      s.addText("↕ 拷贝上下文 / 粘贴产物", {
        x: 8.5,
        y: y + 0.75,
        w: 3.8,
        h: 0.3,
        fontFace: FONT,
        fontSize: 11,
        align: "center",
        color: C.red,
      });
    }
  });
  s.addText("开发连贯性被强制打断", {
    x: 8.25,
    y: 5.55,
    w: 4.3,
    h: 0.35,
    fontFace: FONT,
    fontSize: 13,
    bold: true,
    color: C.red,
    align: "center",
  });
}

// ═══════════════════════════════════════════════════════════
// P05 Agent 孤岛（下）
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 5);
  pageTitle(s, "困境：Agent 孤岛（下）—— 自闭症大脑 + 人被异化为手脚");
  card(s, 0.55, 1.15, 5.9, 4.3);
  s.addText("上下文异构 · 记忆异构", {
    x: 0.8,
    y: 1.4,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 16,
    bold: true,
    color: C.black,
  });
  bullet(
    s,
    [
      "你的记忆不是我的记忆",
      "我的语言你一知半解",
      "Agent 都是「自闭症」",
      "异构上下文无法自然互通",
    ],
    { x: 0.9, y: 2.0, w: 5.2, h: 2.8, fontSize: 14 },
  );
  card(s, 6.8, 1.15, 5.9, 4.3, C.redSoft);
  s.addText("有大脑、没手脚", {
    x: 7.05,
    y: 1.4,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 16,
    bold: true,
    color: C.black,
  });
  bullet(
    s,
    [
      "无法对接各系统、不会用合适工具",
      "必须开发人员参与补位",
      "人被异化为 Agent 的手脚",
      "开发连贯性再次被打断",
    ],
    { x: 7.15, y: 2.0, w: 5.2, h: 2.8, fontSize: 14 },
  );
  quoteBar(
    s,
    "孤岛的本质 = 无法协同 + 无法触达系统 + 人沦为胶水",
    0.55,
    5.8,
    12,
  );
}

// ═══════════════════════════════════════════════════════════
// P06 系统林立 + 流程鸿沟
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 6);
  pageTitle(s, "困境：系统林立 + 流程鸿沟");
  card(s, 0.55, 1.2, 5.9, 4.0);
  s.addText("系统林立", {
    x: 0.8,
    y: 1.45,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 18,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "华为内部项目 / 开发 / 测试相关系统众多且逻辑复杂",
      "开发者学习成本高",
      "注意力被系统切换吞噬",
    ],
    { x: 0.9, y: 2.1, w: 5.2, h: 2.5, fontSize: 14 },
  );
  card(s, 6.8, 1.2, 5.9, 4.0);
  s.addText("流程鸿沟", {
    x: 7.05,
    y: 1.45,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 18,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "Spec 种类繁多，缺少适配华为 IPD 流程的 Spec",
      "通用 AI 工具无法理解华为内部语境",
      "水土不服：进得了对话，进不了公司流程",
    ],
    { x: 7.15, y: 2.1, w: 5.2, h: 2.5, fontSize: 14 },
  );
  quoteBar(s, "Agent 进得了对话，进不了公司流程", 0.55, 5.6, 12);
}

// ═══════════════════════════════════════════════════════════
// P07 万国造 + 工具负担
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 7);
  pageTitle(s, "困境：「万国造」Skill/MCP + Agent 工具负担");
  card(s, 0.55, 1.2, 5.9, 4.0);
  s.addText("万国造 Skill / MCP", {
    x: 0.8,
    y: 1.45,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 17,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "管理标准不统一",
      "本地 Skill/MCP 到处散落",
      "缺少统一线上管理",
      "知识难复用，质量无法保证",
    ],
    { x: 0.9, y: 2.1, w: 5.2, h: 2.6, fontSize: 14 },
  );
  card(s, 6.8, 1.2, 5.9, 4.0);
  s.addText("Agent 工具负担", {
    x: 7.05,
    y: 1.45,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 17,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "磨刀不误砍柴工，柴刀多了也误工",
      "多 Agent 并行时，管理与配置本身成为负担",
      "能力越多，治理越乱",
      "最后谁都不敢用",
    ],
    { x: 7.15, y: 2.1, w: 5.2, h: 2.6, fontSize: 14 },
  );
  quoteBar(s, "能力越多，治理越乱，最后谁都不敢用", 0.55, 5.6, 12);
}

// ═══════════════════════════════════════════════════════════
// P08 原始界面
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 8);
  pageTitle(s, "困境：原始界面 —— 硬核不代表好用");
  card(s, 0.55, 1.2, 5.9, 4.2, C.bgSoft);
  s.addText("现状：Agent CLI / TUI", {
    x: 0.8,
    y: 1.45,
    w: 5.4,
    h: 0.35,
    fontFace: FONT,
    fontSize: 15,
    bold: true,
    color: C.gray1,
  });
  bullet(
    s,
    [
      "界面原始，信息密度不高",
      "展示杂乱，关键路径难找",
      "信息获取效率低",
      "直接阻碍开发效率",
    ],
    { x: 0.9, y: 2.0, w: 5.2, h: 2.8, fontSize: 14 },
  );
  card(s, 6.8, 1.2, 5.9, 4.2, C.layer2);
  s.addText("期待：IDE 级信息架构", {
    x: 7.05,
    y: 1.45,
    w: 5.4,
    h: 0.35,
    fontFace: FONT,
    fontSize: 15,
    bold: true,
    color: C.gray1,
  });
  bullet(
    s,
    [
      "高密度但不杂乱",
      "任务 / Session / 产物一目了然",
      "桌面端 + Web 同源体验",
      "Beauty does matter",
    ],
    { x: 7.15, y: 2.0, w: 5.2, h: 2.8, fontSize: 14 },
  );
  quoteBar(
    s,
    "丢掉硬核的思想包袱 —— Beauty does matter",
    0.55,
    5.75,
    12,
  );
}

// ═══════════════════════════════════════════════════════════
// P09 工作方式落后
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 9);
  pageTitle(s, "困境：工作方式落后 —— Token 再多也用不出去");
  bullet(
    s,
    [
      "随便找我司员工，能完整用过或说出 Worktree 概念的，不到一半",
      "开发方式落后、工作习惯老旧；各 Agent CLI 仍在沿用这种落后方式",
      "工作负载跑不满：有并行潜力，却堵在一条命令行 / 一个目录里",
      "明明是天顶星科技，还在拼刺刀",
    ],
    { x: 0.7, y: 1.2, w: 12, h: 2.8, fontSize: 16, paraSpacing: 14 },
  );
  card(s, 0.55, 4.3, 12.2, 1.7, C.redSoft);
  s.addText("过渡", {
    x: 0.85,
    y: 4.5,
    w: 2,
    h: 0.3,
    fontFace: FONT,
    fontSize: 12,
    bold: true,
    color: C.red,
  });
  s.addText(
    "困境清楚了 → Ora 如何系统性解题：统一度量衡、打通流程、纳管资产、升级体验、并行车道。",
    {
      x: 0.85,
      y: 4.95,
      w: 11.5,
      h: 0.7,
      fontFace: FONT,
      fontSize: 15,
      color: C.black,
    },
  );
}

// ═══════════════════════════════════════════════════════════
// P10 五招破局
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 10);
  pageTitle(s, "解题总纲：五招破局");
  const moves = [
    ["1", "书同文车同轨，统一度量衡", "Agent 共享记忆与上下文，打造三体世界"],
    ["2", "自研 IPD 工作流", "打通开发全链路，把人从系统中解放"],
    ["3", "本地规则 + Skill/MCP 市场", "本地云上一把抓"],
    ["4", "Beauty does matter", "给 Agent 更好的手脚与面孔"],
    ["5", "Worktree = 开发车道", "多 Agent 并排行驶，欢迎加入并行世界"],
  ];
  moves.forEach((m, i) => {
    const y = 1.05 + i * 1.05;
    s.addShape(pptx.shapes.ROUNDED_RECTANGLE, {
      x: 0.55,
      y,
      w: 12.2,
      h: 0.9,
      fill: { color: i % 2 === 0 ? C.bgCard : C.bgSoft },
      line: { color: C.line },
      rectRadius: 0.06,
    });
    s.addShape(pptx.shapes.OVAL, {
      x: 0.8,
      y: y + 0.2,
      w: 0.5,
      h: 0.5,
      fill: { color: C.red },
      line: { color: C.red },
    });
    s.addText(m[0], {
      x: 0.8,
      y: y + 0.28,
      w: 0.5,
      h: 0.35,
      fontFace: FONT,
      fontSize: 16,
      bold: true,
      color: C.white,
      align: "center",
    });
    s.addText(m[1], {
      x: 1.55,
      y: y + 0.12,
      w: 10.8,
      h: 0.35,
      fontFace: FONT,
      fontSize: 16,
      bold: true,
      color: C.black,
    });
    s.addText(m[2], {
      x: 1.55,
      y: y + 0.48,
      w: 10.8,
      h: 0.3,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray1,
    });
  });
}

// ═══════════════════════════════════════════════════════════
// P11 招式 1–2
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 11);
  pageTitle(s, "招式 1–2：三体世界 + IPD 工作流");
  card(s, 0.55, 1.15, 5.9, 5.0);
  s.addText("① 书同文车同轨", {
    x: 0.8,
    y: 1.4,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 17,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "统一协议与对象模型",
      "异构 Agent 共享上下文、记忆与产物",
      "不再靠拷贝粘贴或污染同目录协同",
      "打造 Agent 的三体世界",
    ],
    { x: 0.9, y: 2.0, w: 5.2, h: 3.5, fontSize: 14 },
  );
  card(s, 6.8, 1.15, 5.9, 5.0);
  s.addText("② 自研 IPD 工作流", {
    x: 7.05,
    y: 1.4,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 17,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "把华为开发语境与门禁写进工作流 / Spec / Skill",
      "打通：编码 → 测试 → 上库 → MR → 流水线 → 合入 → 回调",
      "人从繁杂系统切换中解放",
      "从「Agent 的手脚」变回「决策者」",
    ],
    { x: 7.15, y: 2.0, w: 5.2, h: 3.5, fontSize: 14 },
  );
}

// ═══════════════════════════════════════════════════════════
// P12 招式 3–4
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 12);
  pageTitle(s, "招式 3–4：统一纳管 + Beauty");
  card(s, 0.55, 1.15, 5.9, 5.0);
  s.addText("③ 本地规则 + Skill/MCP 市场", {
    x: 0.8,
    y: 1.4,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 16,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "本地有规则，云上有市场",
      "Skill/MCP 可导入项目统一纳管",
      "也可选择后给 Agent 统一安装",
      "本地云上一把抓；告别霰弹式修改",
    ],
    { x: 0.9, y: 2.0, w: 5.2, h: 3.5, fontSize: 14 },
  );
  card(s, 6.8, 1.15, 5.9, 5.0);
  s.addText("④ Beauty does matter", {
    x: 7.05,
    y: 1.4,
    w: 5.4,
    h: 0.4,
    fontFace: FONT,
    fontSize: 16,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "IDE 级信息架构：高密度但不杂乱",
      "桌面端 + Web 同源体验",
      "丢掉硬核的思想包袱",
      "硬核不代表好用",
    ],
    { x: 7.15, y: 2.0, w: 5.2, h: 3.5, fontSize: 14 },
  );
}

// ═══════════════════════════════════════════════════════════
// P13 Worktree 开发车道
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 13);
  pageTitle(s, "招式 5：把 Worktree 变成开发车道");
  const lanes = [
    ["任务分流", "任务自动分流到隔离 Worktree（像车道）"],
    ["进度可视", "进度可视化管理，一目了然"],
    ["并排行驶", "多个 Agent 并排行驶，不再堵在一条命令行里"],
    ["创建极简", "创建 Worktree 像呼吸一样简单"],
  ];
  lanes.forEach((l, i) => {
    const x = 0.55 + i * 3.15;
    card(s, x, 1.3, 3.0, 3.2);
    s.addShape(pptx.shapes.RECTANGLE, {
      x: x + 0.3,
      y: 1.6,
      w: 2.4,
      h: 0.08,
      fill: { color: C.red },
      line: { color: C.red },
    });
    s.addText(`车道 ${i + 1}`, {
      x: x + 0.2,
      y: 1.9,
      w: 2.6,
      h: 0.35,
      fontFace: FONT,
      fontSize: 12,
      color: C.gray2,
      align: "center",
    });
    s.addText(l[0], {
      x: x + 0.2,
      y: 2.4,
      w: 2.6,
      h: 0.45,
      fontFace: FONT,
      fontSize: 18,
      bold: true,
      color: C.black,
      align: "center",
    });
    s.addText(l[1], {
      x: x + 0.25,
      y: 3.1,
      w: 2.5,
      h: 1.1,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray1,
      align: "center",
    });
  });
  quoteBar(
    s,
    "欢迎加入并行世界 —— 让 Token 真正跑满负载",
    0.55,
    5.0,
    12,
  );
}

// ═══════════════════════════════════════════════════════════
// P14 架构图（华为五层）
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 14);
  pageTitle(s, "Ora 应用架构", 0.18);
  s.addText("华为规范分层｜体验 / 应用 / 领域 / 基础设施 / 插件生态与外部系统", {
    x: 0.55,
    y: 0.72,
    w: 12,
    h: 0.25,
    fontFace: FONT,
    fontSize: 11,
    color: C.gray2,
  });

  const layers = [
    {
      name: "L1 体验层 Experience",
      fill: C.layer1,
      lines: [
        "Desktop（Tauri） ｜ Web Client ｜ Mobile（规划）",
        "AppShell：Project / Task / Session / Chat / Settings·Atoms",
      ],
      h: 0.85,
    },
    {
      name: "L2 应用层 Application",
      fill: C.layer2,
      lines: [
        "Web Server（Rust/Axum）｜ 用例编排 ｜ Chat/ACP 客户端",
        "插件宿主：发现 / 安装 / 生命周期 / 一插件一进程",
      ],
      h: 0.85,
    },
    {
      name: "L3 领域层 Domain",
      fill: C.layer3,
      lines: [
        "Project · Task · Worktree · Session · AgentDefinition · Skill",
        "统一契约 Contracts（含 ACP：会话 / 权限 / FS / 终端 / MCP…）",
      ],
      h: 0.85,
    },
    {
      name: "L4 基础设施层 Infrastructure",
      fill: C.layer4,
      lines: [
        "SQLite ｜ Git 运行时（多 Worktree）｜ 进程管理 ｜ PTY ｜ 日志",
        "Plugin SDK（TypeScript / Bun · JSON-RPC）",
      ],
      h: 0.85,
    },
  ];

  let y = 1.05;
  layers.forEach((L) => {
    s.addShape(pptx.shapes.ROUNDED_RECTANGLE, {
      x: 0.45,
      y,
      w: 12.4,
      h: L.h,
      fill: { color: L.fill },
      line: { color: C.line, width: 1 },
      rectRadius: 0.05,
    });
    s.addText(L.name, {
      x: 0.65,
      y: y + 0.08,
      w: 12,
      h: 0.28,
      fontFace: FONT,
      fontSize: 12,
      bold: true,
      color: C.red,
    });
    s.addText(L.lines.join("\n"), {
      x: 0.65,
      y: y + 0.35,
      w: 12,
      h: 0.45,
      fontFace: FONT,
      fontSize: 11,
      color: C.text,
    });
    y += L.h + 0.08;
  });

  // L5a / L5b
  s.addShape(pptx.shapes.ROUNDED_RECTANGLE, {
    x: 0.45,
    y: 4.75,
    w: 6.0,
    h: 1.55,
    fill: { color: C.layer5a },
    line: { color: C.ok, width: 1.25, dashType: "dash" },
    rectRadius: 0.05,
  });
  s.addText("L5a 插件生态（可插拔）", {
    x: 0.65,
    y: 4.85,
    w: 5.6,
    h: 0.28,
    fontFace: FONT,
    fontSize: 12,
    bold: true,
    color: C.ok,
  });
  s.addText(
    "Agent / UI / Workbench / 工作流 / IM /「同事插件」…\nTS + Bun · 一插件一进程 · Agent 矮化为工具",
    {
      x: 0.65,
      y: 5.2,
      w: 5.6,
      h: 0.9,
      fontFace: FONT,
      fontSize: 11,
      color: C.text,
    },
  );

  s.addShape(pptx.shapes.ROUNDED_RECTANGLE, {
    x: 6.85,
    y: 4.75,
    w: 6.0,
    h: 1.55,
    fill: { color: C.layer5b },
    line: { color: "6B5B95", width: 1.25, dashType: "dash" },
    rectRadius: 0.05,
  });
  s.addText("L5b 外部系统（可插拔）", {
    x: 7.05,
    y: 4.85,
    w: 5.6,
    h: 0.28,
    fontFace: FONT,
    fontSize: 12,
    bold: true,
    color: "6B5B95",
  });
  s.addText(
    "Claude Code / Codex / OpenCode\nCodeHub / MR / 流水线 / 内部系统  ···  ACP / 工具对接",
    {
      x: 7.05,
      y: 5.2,
      w: 5.6,
      h: 0.9,
      fontFace: FONT,
      fontSize: 11,
      color: C.text,
    },
  );

  s.addText(
    "标注：后端 Rust ｜ 前端 Desktop+Web（Mobile 规划）｜ Task↔Worktree 隔离并行",
    {
      x: 0.55,
      y: 6.5,
      w: 12,
      h: 0.3,
      fontFace: FONT,
      fontSize: 11,
      color: C.gray2,
    },
  );
  s.addNotes(
    "口播：上边壳、中间度量衡、底下车道（Worktree）与手脚（插件/进程）；右边 Agent 与企业系统都是可插拔外部能力。",
  );
}

// ═══════════════════════════════════════════════════════════
// P15 核心能力
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 15);
  pageTitle(s, "Ora 核心能力");
  const caps = [
    ["万物皆可插件化", "集成 Agent、IM、UI、Workbench、工作流；甚至把同事打包为插件。"],
    [
      "Agent 无缝切换",
      "共享异构上下文、记忆与产物；一个应用完成所有操作，告别窗口间拷贝。",
    ],
    [
      "并行世界",
      "呼吸一样创建 Worktree；隔离任务；可视化进度；多 Agent 并发释放效率。",
    ],
    [
      "统一纳管",
      "统一纳管本地 Agent 与配置；统一对接内部 Agent 市场；告别霰弹式修改。",
    ],
  ];
  caps.forEach((c, i) => {
    const col = i % 2;
    const row = Math.floor(i / 2);
    const x = 0.55 + col * 6.35;
    const y = 1.15 + row * 2.55;
    card(s, x, y, 6.1, 2.35);
    s.addShape(pptx.shapes.RECTANGLE, {
      x,
      y,
      w: 0.12,
      h: 2.35,
      fill: { color: C.red },
      line: { color: C.red },
    });
    s.addText(c[0], {
      x: x + 0.4,
      y: y + 0.35,
      w: 5.4,
      h: 0.45,
      fontFace: FONT,
      fontSize: 18,
      bold: true,
      color: C.black,
    });
    s.addText(c[1], {
      x: x + 0.4,
      y: y + 1.0,
      w: 5.4,
      h: 1.0,
      fontFace: FONT,
      fontSize: 14,
      color: C.gray1,
    });
  });
}

// ═══════════════════════════════════════════════════════════
// P16 插件宇宙
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 16);
  pageTitle(s, "插件宇宙：类型与价值");
  const rows = [
    ["插件类型", "价值", "举例"],
    ["Agent", "对接异构大脑", "Claude Code、Codex、OpenCode"],
    ["工作流", "打通 IPD/上库全链路", "华为开发上库流程"],
    ["UI / Workbench", "提升信息获取与专业操作", "可视化面板、评审台"],
    ["IM / 扩展", "通知、协同、审批", "即时通讯、回调"],
    ["「同事」", "人机协同也是插件", "人工审批节点"],
  ];
  rows.forEach((r, i) => {
    const y = 1.1 + i * 0.7;
    const bg = i === 0 ? C.red : i % 2 === 0 ? C.bgSoft : C.white;
    const tc = i === 0 ? C.white : C.text;
    [0, 1, 2].forEach((ci) => {
      const widths = [2.6, 4.2, 5.2];
      const xs = [0.55, 3.15, 7.35];
      s.addShape(pptx.shapes.RECTANGLE, {
        x: xs[ci],
        y,
        w: widths[ci],
        h: 0.65,
        fill: { color: bg },
        line: { color: C.line, width: 0.75 },
      });
      s.addText(r[ci], {
        x: xs[ci] + 0.15,
        y: y + 0.15,
        w: widths[ci] - 0.3,
        h: 0.4,
        fontFace: FONT,
        fontSize: i === 0 ? 13 : 13,
        bold: i === 0 || ci === 0,
        color: tc,
        valign: "middle",
      });
    });
  });
  s.addText(
    "技术点：TypeScript + Bun；一插件一进程；进程可完整回收。　彩蛋：你甚至可以把同事打包为插件。",
    {
      x: 0.55,
      y: 5.5,
      w: 12.2,
      h: 0.4,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray1,
    },
  );
}

// ═══════════════════════════════════════════════════════════
// P17 领域对象
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 17);
  pageTitle(s, "领域对象：人不再当胶水");
  card(s, 0.55, 1.2, 7.5, 4.5, C.bgSoft);
  s.addText("一句话模型", {
    x: 0.85,
    y: 1.45,
    w: 6.8,
    h: 0.35,
    fontFace: FONT,
    fontSize: 14,
    bold: true,
    color: C.gray2,
  });
  const tree = [
    "Project",
    "  └── Task  ──► Worktree（隔离车道）",
    "        └── Session ──► Agent（可切换的执行器）",
    "",
    "平台资产：Agent 定义 / Skill / MCP",
    "           （Atoms + 市场）",
  ];
  s.addText(tree.join("\n"), {
    x: 1.0,
    y: 2.0,
    w: 6.5,
    h: 3.2,
    fontFace: "Consolas",
    fontSize: 16,
    color: C.black,
  });
  card(s, 8.3, 1.2, 4.4, 4.5);
  s.addText("设计意图", {
    x: 8.55,
    y: 1.5,
    w: 3.9,
    h: 0.35,
    fontFace: FONT,
    fontSize: 15,
    bold: true,
    color: C.red,
  });
  bullet(
    s,
    [
      "人以 Project/Task 为中心",
      "而不是以某个 CLI 为中心",
      "Agent 可以换",
      "流程与资产留下",
    ],
    { x: 8.55, y: 2.1, w: 3.9, h: 3.0, fontSize: 14 },
  );
  quoteBar(s, "Agent 可以换，流程与资产留下", 0.55, 6.0, 12);
}

// ═══════════════════════════════════════════════════════════
// P18 场景导览
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 18);
  pageTitle(s, "场景导览：从「能聊天」到「能交付」");
  card(s, 0.55, 1.4, 5.9, 3.8);
  s.addText("场景 ①", {
    x: 0.85,
    y: 1.7,
    w: 5.3,
    h: 0.35,
    fontFace: FONT,
    fontSize: 14,
    color: C.red,
    bold: true,
  });
  s.addText("插件、Agent、Skill/MCP\n统一纳管", {
    x: 0.85,
    y: 2.2,
    w: 5.3,
    h: 1.0,
    fontFace: FONT,
    fontSize: 20,
    bold: true,
    color: C.black,
  });
  s.addText("识别本地 Agent 与资产 → 导入项目或统一安装 → 在 Ora 内对话", {
    x: 0.85,
    y: 3.5,
    w: 5.3,
    h: 1.0,
    fontFace: FONT,
    fontSize: 14,
    color: C.gray1,
  });
  card(s, 6.8, 1.4, 5.9, 3.8, C.redSoft);
  s.addText("场景 ②", {
    x: 7.1,
    y: 1.7,
    w: 5.3,
    h: 0.35,
    fontFace: FONT,
    fontSize: 14,
    color: C.red,
    bold: true,
  });
  s.addText("给 OpenCode / Claude Code\n装上 Skill，跑通华为上库闭环", {
    x: 7.1,
    y: 2.2,
    w: 5.3,
    h: 1.0,
    fontFace: FONT,
    fontSize: 18,
    bold: true,
    color: C.black,
  });
  s.addText("编码 → 测试 → CodeHub → MR → 流水线 → 修复回环 → 合入 → 回调", {
    x: 7.1,
    y: 3.5,
    w: 5.3,
    h: 1.0,
    fontFace: FONT,
    fontSize: 14,
    color: C.gray1,
  });
  quoteBar(s, "从「能聊天」到「能交付」", 0.55, 5.6, 12);
}

// ═══════════════════════════════════════════════════════════
// P19 场景① 故事
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 19);
  pageTitle(s, "场景①：插件 / Agent / Skill / MCP 纳管（故事）");
  bullet(
    s,
    [
      "安装对应 Agent 插件之后，识别本地已安装 Agent",
      "识别其 Skill、MCP、配置等",
      "导入到项目目录统一纳管",
      "或选择 Skill/MCP 给 Agent 统一安装",
      "在 Ora 内选择插件 → 与本地 Agent 对话",
    ],
    { x: 0.7, y: 1.2, w: 12, h: 3.2, fontSize: 16, paraSpacing: 12 },
  );
  card(s, 0.55, 4.6, 12.2, 1.5, C.bgSoft);
  s.addText("价值点", {
    x: 0.85,
    y: 4.8,
    w: 3,
    h: 0.3,
    fontFace: FONT,
    fontSize: 13,
    bold: true,
    color: C.red,
  });
  s.addText(
    "本地云上一把抓；配置不再霰弹式修改；知识可复用、可治理。",
    {
      x: 0.85,
      y: 5.2,
      w: 11.5,
      h: 0.5,
      fontFace: FONT,
      fontSize: 15,
      color: C.black,
    },
  );
}

// helper for flow boxes
function flowBox(slide, x, y, w, h, text, opts = {}) {
  const fill = opts.fill || C.white;
  const line = opts.line || C.line;
  const shape = opts.diamond
    ? pptx.shapes.DIAMOND
    : pptx.shapes.ROUNDED_RECTANGLE;
  slide.addShape(shape, {
    x,
    y,
    w,
    h,
    fill: { color: fill },
    line: { color: line, width: 1.25 },
    rectRadius: opts.diamond ? undefined : 0.06,
  });
  slide.addText(text, {
    x,
    y: y + (opts.diamond ? 0.25 : 0.12),
    w,
    h: h - (opts.diamond ? 0.35 : 0.2),
    fontFace: FONT,
    fontSize: opts.fontSize || 11,
    color: opts.color || C.text,
    align: "center",
    valign: "middle",
    bold: !!opts.bold,
  });
}

function arrowDown(slide, x, y) {
  slide.addText("▼", {
    x: x - 0.15,
    y,
    w: 0.4,
    h: 0.25,
    fontFace: FONT,
    fontSize: 10,
    color: C.gray1,
    align: "center",
  });
}

// ═══════════════════════════════════════════════════════════
// P20 场景① 流程图
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 20);
  pageTitle(s, "场景①：Skill / MCP 纳管流程图", 0.18);
  // compact vertical-ish flow with branch
  const cx = 6.3;
  flowBox(s, cx - 1.5, 0.95, 3.0, 0.45, "开始：在 Ora 安装 Agent 插件", {
    fill: C.redSoft,
    bold: true,
  });
  arrowDown(s, cx, 1.4);
  flowBox(s, cx - 1.7, 1.6, 3.4, 0.45, "插件探测本地 Agent 安装与配置");
  arrowDown(s, cx, 2.05);
  flowBox(s, cx - 1.7, 2.25, 3.4, 0.45, "识别本地 Skill / MCP 列表");
  arrowDown(s, cx, 2.7);
  flowBox(s, cx - 1.5, 2.9, 3.0, 0.7, "导入纳管\nor\n安装到 Agent？", {
    diamond: true,
    fill: "FFF8E7",
    line: C.warn,
    fontSize: 10,
  });

  // branch A left
  s.addText("◀ A", {
    x: 1.8,
    y: 3.1,
    w: 1.2,
    h: 0.3,
    fontFace: FONT,
    fontSize: 11,
    color: C.ok,
    bold: true,
  });
  flowBox(s, 0.5, 3.55, 3.6, 0.55, "A. 导入项目目录 → 统一纳管\n（版本 / 启用 / 复用）", {
    fill: C.layer5a,
    fontSize: 11,
  });

  // branch B right
  s.addText("B ▶", {
    x: 10.2,
    y: 3.1,
    w: 1.2,
    h: 0.3,
    fontFace: FONT,
    fontSize: 11,
    color: C.ok,
    bold: true,
  });
  flowBox(
    s,
    9.2,
    3.55,
    3.6,
    0.55,
    "B. 勾选 Skill/MCP →\n向目标 Agent 统一安装/同步",
    { fill: C.layer2, fontSize: 11 },
  );

  // merge
  s.addText("▼ 汇合", {
    x: cx - 0.5,
    y: 4.2,
    w: 1.2,
    h: 0.25,
    fontFace: FONT,
    fontSize: 11,
    color: C.gray1,
    align: "center",
  });
  flowBox(s, cx - 2.0, 4.45, 4.0, 0.45, "Atoms / 项目视图确认纳管结果（统一可见）", {
    fill: C.blueSoft,
    bold: true,
  });
  arrowDown(s, cx, 4.9);
  flowBox(
    s,
    cx - 2.3,
    5.1,
    4.6,
    0.55,
    "选择 Agent 插件 → 创建 Task（自动 Worktree）→ Session 对话",
    { fill: C.redSoft, fontSize: 11 },
  );
  flowBox(s, cx - 0.8, 5.85, 1.6, 0.4, "结束", {
    fill: C.black,
    color: C.white,
    bold: true,
  });

  s.addText("箭头表示流程方向；菱形为分支判断；两支汇合后进入对话。", {
    x: 0.55,
    y: 6.5,
    w: 12,
    h: 0.3,
    fontFace: FONT,
    fontSize: 11,
    color: C.gray2,
  });
}

// ═══════════════════════════════════════════════════════════
// P21 场景② 上库总流程
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 21);
  pageTitle(s, "场景②：Skill 驱动的华为开发上库自动驾驶", 0.15);

  // Row 1: prep + develop
  const steps1 = [
    ["1", "选 Agent\n插件"],
    ["2", "挂载上库\nSkill/MCP"],
    ["3", "Agent\n编码"],
    ["4", "本地\n测试"],
    ["5", "提交到\nCodeHub"],
    ["6", "创建\nMR"],
  ];
  steps1.forEach((st, i) => {
    const x = 0.35 + i * 2.15;
    flowBox(s, x, 1.0, 1.95, 0.85, `${st[0]}. ${st[1]}`, {
      fill: i >= 4 ? C.layer5b : C.white,
      fontSize: 11,
      bold: true,
    });
    if (i < steps1.length - 1) {
      s.addText("→", {
        x: x + 1.85,
        y: 1.25,
        w: 0.35,
        h: 0.35,
        fontFace: FONT,
        fontSize: 16,
        color: C.gray1,
        align: "center",
      });
    }
  });

  s.addText("↓", {
    x: 12.2,
    y: 1.9,
    w: 0.4,
    h: 0.3,
    fontFace: FONT,
    fontSize: 14,
    color: C.gray1,
    align: "center",
  });

  // Row 2: pipeline + decision
  flowBox(s, 10.5, 2.25, 2.4, 0.7, "7. 跑 MR 合并流水线", {
    fill: C.layer5b,
    fontSize: 11,
    bold: true,
  });
  s.addText("←", {
    x: 9.9,
    y: 2.4,
    w: 0.5,
    h: 0.35,
    fontFace: FONT,
    fontSize: 16,
    color: C.gray1,
  });
  flowBox(s, 7.2, 2.15, 2.6, 0.9, "8. 扫描是否通过？", {
    diamond: true,
    fill: "FFF8E7",
    line: C.warn,
    fontSize: 11,
    bold: true,
  });

  // Fail loop
  s.addText("否（格式等问题）", {
    x: 4.0,
    y: 2.15,
    w: 2.8,
    h: 0.25,
    fontFace: FONT,
    fontSize: 11,
    color: C.warn,
    bold: true,
  });
  flowBox(s, 3.6, 2.45, 3.2, 0.7, "Agent 修改代码 → 重新提交", {
    fill: "FDF2E9",
    line: C.warn,
    fontSize: 11,
    bold: true,
  });
  s.addText("↻ 回到第 7 步（回环）", {
    x: 3.6,
    y: 3.25,
    w: 3.2,
    h: 0.3,
    fontFace: FONT,
    fontSize: 12,
    color: C.warn,
    bold: true,
    align: "center",
  });

  // Yes path
  s.addText("是 ↓", {
    x: 7.9,
    y: 3.15,
    w: 1.2,
    h: 0.3,
    fontFace: FONT,
    fontSize: 12,
    color: C.ok,
    bold: true,
    align: "center",
  });

  const steps3 = [
    ["9", "通知 Committer\n合并代码"],
    ["10", "代码合并"],
    ["11", "消息回调\n状态回写 Ora"],
  ];
  steps3.forEach((st, i) => {
    const x = 4.5 + i * 2.8;
    flowBox(s, x, 3.55, 2.55, 0.85, `${st[0]}. ${st[1]}`, {
      fill: C.layer5a,
      fontSize: 12,
      bold: true,
    });
    if (i < steps3.length - 1) {
      s.addText("→", {
        x: x + 2.45,
        y: 3.8,
        w: 0.4,
        h: 0.35,
        fontFace: FONT,
        fontSize: 16,
        color: C.gray1,
      });
    }
  });

  // Legend
  card(s, 0.45, 4.7, 12.4, 1.7, C.bgSoft);
  s.addText("图例与说明", {
    x: 0.7,
    y: 4.9,
    w: 4,
    h: 0.3,
    fontFace: FONT,
    fontSize: 13,
    bold: true,
    color: C.black,
  });
  s.addText(
    [
      "• 主路径：粗箭头「→」按 1→11 顺序前进",
      "• 失败回环：橙色路径 —— 扫描不通过 → Agent 修复 → 重新提交 → 回到第 7 步",
      "• 紫色底节点：CodeHub / MR / 流水线等企业系统能力",
      "• 本流程符合华为开发上库门禁习惯：先本地验证，再上库建 MR，再合入回调",
    ].join("\n"),
    {
      x: 0.7,
      y: 5.25,
      w: 11.8,
      h: 1.0,
      fontFace: FONT,
      fontSize: 12,
      color: C.gray1,
    },
  );
}

// ═══════════════════════════════════════════════════════════
// P22 四泳道
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 22);
  pageTitle(s, "场景②：四泳道流向（人从手脚升级为决策者）", 0.15);

  const headers = ["阶段", "开发者", "Ora", "Agent 插件", "CodeHub / 流水线"];
  const widths = [1.5, 2.5, 2.7, 2.7, 2.8];
  let x0 = 0.4;
  headers.forEach((h, i) => {
    s.addShape(pptx.shapes.RECTANGLE, {
      x: x0,
      y: 0.9,
      w: widths[i],
      h: 0.45,
      fill: { color: C.red },
      line: { color: C.red },
    });
    s.addText(h, {
      x: x0,
      y: 0.95,
      w: widths[i],
      h: 0.35,
      fontFace: FONT,
      fontSize: 11,
      bold: true,
      color: C.white,
      align: "center",
    });
    x0 += widths[i];
  });

  const data = [
    ["准备", "选择 Agent、确认 Skill", "加载工作流/纳管资产", "就绪", "—"],
    ["开发", "提需求、关键确认", "Task + Worktree 隔离", "编码、本地测试", "—"],
    ["上库", "可选审批", "编排提交流程", "git push / 建 MR", "CodeHub 收码、建 MR"],
    ["门禁", "旁观", "失败信息回灌 Session", "按 Skill 修复再推", "流水线扫描 / 重跑"],
    ["合入", "Committer 合并", "通知 + 状态展示", "—", "合并 + 回调"],
  ];
  data.forEach((row, ri) => {
    let x = 0.4;
    const y = 1.35 + ri * 0.85;
    row.forEach((cell, ci) => {
      const bg =
        ci === 0 ? C.redSoft : ri % 2 === 0 ? C.white : C.bgSoft;
      s.addShape(pptx.shapes.RECTANGLE, {
        x,
        y,
        w: widths[ci],
        h: 0.85,
        fill: { color: bg },
        line: { color: C.line, width: 0.75 },
      });
      s.addText(cell, {
        x: x + 0.08,
        y: y + 0.2,
        w: widths[ci] - 0.16,
        h: 0.5,
        fontFace: FONT,
        fontSize: 11,
        bold: ci === 0,
        color: C.text,
        align: "center",
        valign: "middle",
      });
      x += widths[ci];
    });
  });

  quoteBar(s, "人从「Agent 的手脚」升级为「决策者」", 0.4, 5.8, 12);
  s.addText("箭头语义：横向为阶段推进；单元格内容为该角色在该阶段的真实动作 / 数据流向。", {
    x: 0.55,
    y: 6.5,
    w: 12,
    h: 0.3,
    fontFace: FONT,
    fontSize: 11,
    color: C.gray2,
  });
}

// ═══════════════════════════════════════════════════════════
// P23 Before / After
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 23);
  pageTitle(s, "双场景价值对照（Before / After）");
  const tbl = [
    ["维度", "Before", "After（Ora）"],
    ["多 Agent 协同", "拷贝数据 / 污染同目录", "共享上下文与产物，Worktree 隔离"],
    ["Skill/MCP", "万国造、到处散落", "项目纳管 + 统一安装 + 市场"],
    ["华为流程", "水土不服、人肉串系统", "IPD/上库工作流 + Skill 自动驾驶"],
    ["界面与效率", "CLI 杂乱、负载跑不满", "IDE 体验 + 多车道并行"],
    ["人的角色", "Agent 的手脚", "决策者"],
  ];
  tbl.forEach((r, i) => {
    const y = 1.05 + i * 0.85;
    const bg = i === 0 ? C.red : i % 2 === 0 ? C.bgSoft : C.white;
    const tc = i === 0 ? C.white : C.text;
    const ws = [2.4, 4.8, 5.0];
    let x = 0.55;
    r.forEach((cell, ci) => {
      s.addShape(pptx.shapes.RECTANGLE, {
        x,
        y,
        w: ws[ci],
        h: 0.8,
        fill: { color: bg },
        line: { color: C.line, width: 0.75 },
      });
      s.addText(cell, {
        x: x + 0.12,
        y: y + 0.2,
        w: ws[ci] - 0.24,
        h: 0.45,
        fontFace: FONT,
        fontSize: 13,
        bold: i === 0 || ci === 0,
        color: i > 0 && ci === 2 ? C.ok : tc,
        valign: "middle",
      });
      x += ws[ci];
    });
  });
}

// ═══════════════════════════════════════════════════════════
// P24 总结 + 支持诉求
// ═══════════════════════════════════════════════════════════
{
  const s = addSlide();
  chrome(s, 24);
  pageTitle(s, "总结：第一阶段成果与支持诉求");
  const five = [
    "三体世界 —— 共享记忆与上下文",
    "IPD 工作流 —— 打通全链路",
    "本地 + 市场 —— Skill/MCP 一把抓",
    "Beauty does matter",
    "并行世界 —— Worktree 开发车道",
  ];
  five.forEach((t, i) => {
    const y = 1.05 + i * 0.55;
    s.addShape(pptx.shapes.OVAL, {
      x: 0.7,
      y: y + 0.05,
      w: 0.35,
      h: 0.35,
      fill: { color: C.red },
      line: { color: C.red },
    });
    s.addText(String(i + 1), {
      x: 0.7,
      y: y + 0.08,
      w: 0.35,
      h: 0.3,
      fontFace: FONT,
      fontSize: 12,
      bold: true,
      color: C.white,
      align: "center",
    });
    s.addText(t, {
      x: 1.25,
      y: y,
      w: 11,
      h: 0.45,
      fontFace: FONT,
      fontSize: 15,
      color: C.black,
    });
  });

  card(s, 0.55, 3.95, 12.2, 2.4, C.redSoft);
  s.addText(
    "我们不是再做一个 Agent，而是做让所有 Agent、流程与人协作的 IDE。",
    {
      x: 0.85,
      y: 4.2,
      w: 11.6,
      h: 0.55,
      fontFace: FONT,
      fontSize: 16,
      bold: true,
      color: C.black,
    },
  );
  s.addText(
    "第一阶段成果（架构与能力底座）：Rust 内核与领域模型、插件宿主（TS+Bun / 一插件一进程）、Worktree 并行底座、Desktop+Web 体验层、ACP/契约与演示场景设计。\n请领导支持：持续投入与资源保障，推进 Agent 插件生态、华为 IPD/上库 Skill、以及 CodeHub 等内部系统对接落地。",
    {
      x: 0.85,
      y: 4.85,
      w: 11.6,
      h: 1.2,
      fontFace: FONT,
      fontSize: 13,
      color: C.gray1,
    },
  );
}

await pptx.writeFile({ fileName: OUT });
console.log(`Wrote: ${OUT}`);
console.log(`Slides: ${pptx.slides.length}`);
