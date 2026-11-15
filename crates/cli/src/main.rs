//! `teyvat-gacha` 命令行入口：抓取、入库、统计、终端界面。
//!
//! 这个 CLI 同时也是桌面端的回归验证工具：统计逻辑与数据库完全共用，
//! 不装 Tauri 也能验证全部核心功能。

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use chrono::{Duration as ChronoDuration, NaiveDateTime};
use gacha_core::{analyze, AccountStats, PullRecord, PoolKind, POOLS, PRIMOGEM_PER_PULL, PROBE_CANDIDATE_TYPES};
use gacha_fetch::{best_local_url, scan_local_cache, GachaClient, GachaUrl};
use gacha_store::{default_db_path, Store};
use ratatui::crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Parser)]
#[command(name = "teyvat-gacha", version, about = "提瓦特祈愿记录分析（全程本地运行）")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 扫描本地缓存，列出可用的祈愿链接
    Scan,
    /// 探测未知池类型（例如千星奇域）的 gacha_type
    Probe {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        auto: bool,
    },
    /// 抓取并写入本地数据库
    Sync {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        auto: bool,
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// 打印统计摘要
    Stats {
        #[arg(long)]
        uid: Option<String>,
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// 终端图形界面
    Tui {
        #[arg(long)]
        uid: Option<String>,
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// 生成演示数据，用于在拿不到有效 authkey 时验证统计与界面
    Demo {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long, default_value_t = 4483)]
        pulls: u32,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Scan) => run_scan(),
        Some(Command::Probe { url, auto }) => run_probe(url, auto),
        Some(Command::Sync { url, auto, db }) => run_sync(url, auto, db),
        Some(Command::Stats { uid, db }) => run_stats(uid, db),
        Some(Command::Tui { uid, db }) => run_tui(uid, db),
        Some(Command::Demo { db, pulls }) => run_demo(db, pulls),
        None => {
            println!("用法：teyvat-gacha <scan|probe|sync|stats|tui>，用 --help 查看详情");
            return;
        }
    };

    if let Err(message) = result {
        eprintln!("错误：{message}");
        std::process::exit(1);
    }
}

fn resolve_url(url_arg: Option<String>, auto: bool) -> Result<GachaUrl, String> {
    if let Some(raw) = url_arg {
        return GachaUrl::parse(&raw).map_err(|err| err.to_string());
    }
    if !auto {
        return Err("需要 --url <祈愿链接> 或 --auto（从本地缓存自动提取）".to_string());
    }
    match best_local_url() {
        Ok(hit) => {
            println!("本地缓存命中：{}", hit.cache_path.display());
            GachaUrl::parse(&hit.url).map_err(|err| err.to_string())
        }
        Err(err) => Err(err.to_string()),
    }
}

fn run_scan() -> Result<(), String> {
    let hits = scan_local_cache().map_err(|err| err.to_string())?;
    if hits.is_empty() {
        println!("没有在本地缓存中找到祈愿链接。请先在游戏内打开一次「祈愿 → 祈愿记录」。");
        return Ok(());
    }
    println!("找到 {} 条候选链接：", hits.len());
    for (index, hit) in hits.iter().enumerate() {
        let parsed = GachaUrl::parse(&hit.url).ok();
        let masked = parsed
            .as_ref()
            .map(|url| url.masked_authkey())
            .unwrap_or_else(|| "****".to_string());
        let gacha_type = extract_query(&hit.url, "gacha_type").unwrap_or_else(|| "?".to_string());
        println!(
            "  [{}] score={} gacha_type={} authkey={} \n      {}",
            index + 1,
            hit.score,
            gacha_type,
            masked,
            hit.cache_path.display()
        );
    }
    Ok(())
}

fn extract_query(url: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=");
    let start = url.find(&pattern)? + pattern.len();
    let rest = &url[start..];
    let end = rest.find('&').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn run_probe(url_arg: Option<String>, auto: bool) -> Result<(), String> {
    let url = resolve_url(url_arg, auto)?;
    println!("开始探测候选 gacha_type（间隔 400ms，请耐心等待）");
    let client = GachaClient::new(url);
    let hits = client.probe_pool_types(PROBE_CANDIDATE_TYPES);
    if hits.is_empty() {
        println!("候选列表全部无记录。可扩大 PROBE_CANDIDATE_TYPES 后重试。");
        return Ok(());
    }
    for (gacha_type, count) in hits {
        println!("  gacha_type={gacha_type} 命中 {count} 条");
    }
    Ok(())
}

fn run_sync(url_arg: Option<String>, auto: bool, db: Option<PathBuf>) -> Result<(), String> {
    let url = resolve_url(url_arg, auto)?;
    println!(
        "区服：{}（{}），authkey：{}",
        url.region,
        url.api_base(),
        url.masked_authkey()
    );

    let client = GachaClient::new(url.clone());
    let mut all: Vec<PullRecord> = Vec::new();
    let mut stdout = std::io::stdout();

    let db_path = db.unwrap_or_else(default_db_path);
    let mut store = Store::open(&db_path).map_err(|err| err.to_string())?;

    // 增量同步：本地已有记录 id 用于提前终止翻页
    let known = store.all_existing_ids().map_err(|err| err.to_string())?;
    if !known.is_empty() {
        println!("本地已有 {} 条记录，本次按增量方式抓取", known.len());
    }

    for def in POOLS {
        if def.gacha_types.is_empty() {
            continue;
        }
        for &gacha_type in def.gacha_types {
            print!("抓取 {} 池（gacha_type={gacha_type}）… ", def.label);
            let _ = stdout.flush();
            match client.fetch_pool_incremental(gacha_type, &known, |count| {
                print!("\r抓取 {} 池（gacha_type={gacha_type}）… {count} 条", def.label);
                let _ = std::io::stdout().flush();
            }) {
                Ok(outcome) => {
                    println!(
                        "\r抓取 {} 池（gacha_type={gacha_type}）… 新增 {} 条，翻页 {} 次{}",
                        def.label,
                        outcome.records.len(),
                        outcome.pages,
                        if outcome.stopped_early { "（命中已有记录，提前停止）" } else { "" }
                    );
                    all.extend(outcome.records);
                }
                Err(err) => {
                    println!("\r抓取 {} 池（gacha_type={gacha_type}）失败：{err}", def.label);
                }
            }
        }
    }

    if all.is_empty() {
        println!("\n没有新记录，本地数据已是最新。");
        return Ok(());
    }

    let uid = all
        .first()
        .map(|record| record.uid.clone())
        .filter(|uid| !uid.is_empty())
        .ok_or_else(|| "记录中缺少 UID，无法关联账号".to_string())?;

    store
        .upsert_account(&gacha_core::Account {
            uid: uid.clone(),
            nickname: None,
            region: url.region.clone(),
            game_biz: url.game_biz.clone(),
        })
        .map_err(|err| err.to_string())?;

    let log_id = store.start_sync_log(&uid).map_err(|err| err.to_string())?;
    let added = store.insert_pulls(&all).map_err(|err| err.to_string())?;
    store.mark_synced(&uid).map_err(|err| err.to_string())?;
    store
        .finish_sync_log(log_id, added, "ok", "")
        .map_err(|err| err.to_string())?;

    println!(
        "\n完成：抓取 {} 条，新增 {} 条，合计 {} 条",
        all.len(),
        added,
        store.count_pulls(&uid).map_err(|err| err.to_string())?
    );
    println!("数据库：{}", db_path.display());
    Ok(())
}

fn load_stats(uid: Option<String>, db: Option<PathBuf>) -> Result<(AccountStats, PathBuf), String> {
    let db_path = db.unwrap_or_else(default_db_path);
    let store = Store::open(&db_path).map_err(|err| err.to_string())?;
    let accounts = store.list_accounts().map_err(|err| err.to_string())?;
    if accounts.is_empty() {
        return Err(format!("数据库为空：{}", db_path.display()));
    }

    let target = match uid {
        Some(uid) => accounts
            .into_iter()
            .find(|account| account.uid == uid)
            .ok_or_else(|| "数据库中没有该 UID".to_string())?,
        None => accounts.into_iter().next().unwrap_or_else(|| unreachable!()),
    };

    let records = store.load_pulls(&target.uid).map_err(|err| err.to_string())?;
    Ok((analyze(&target.uid, &records), db_path))
}

fn fmt_opt(value: Option<f64>, suffix: &str) -> String {
    match value {
        Some(number) => format!("{number:.1}{suffix}"),
        None => "—".to_string(),
    }
}

fn run_stats(uid: Option<String>, db: Option<PathBuf>) -> Result<(), String> {
    let (stats, _) = load_stats(uid, db)?;
    println!("UID {}", mask_uid(&stats.uid));
    println!(
        "欧皇度：{}（{:.1} 分）{}",
        stats.rating.label,
        stats.rating.score,
        if stats.rating.sample_sufficient {
            ""
        } else {
            "（样本不足 100 抽，仅供参考）"
        }
    );
    println!("总抽数：{}", stats.total_pulls);
    println!(
        "小保底不歪率：{}",
        match stats.not_off_rate {
            Some(rate) => format!("{:.1}%", rate * 100.0),
            None => "—".to_string(),
        }
    );
    println!("UP 角色平均：{}", fmt_opt(stats.avg_up_character_pity, " 抽"));
    println!("限定武器平均：{}", fmt_opt(stats.avg_limited_weapon_pity, " 抽"));
    println!("最多连 UP：{} / 最多连歪：{}", stats.max_up_streak, stats.max_off_streak);
    println!("最久未抽卡持续：{} 天", stats.longest_gap_days);
    println!(
        "平均每个 UP 角色花费：{} 原石",
        match stats.cost_per_up_character {
            Some(cost) => cost.to_string(),
            None => "—".to_string(),
        }
    );

    println!("\n分池明细：");
    for pool in &stats.pools {
        if pool.total == 0 {
            continue;
        }
        println!(
            "  {}：{} 抽 / {} 金 / 已垫 {} 抽",
            pool.label, pool.total, pool.five_star_count, pool.current_pity
        );
        for entry in pool.entries.iter().rev().take(5) {
            let tag = match entry.is_up {
                Some(true) => "UP",
                Some(false) => "歪",
                None => "—",
            };
            println!(
                "     {}抽 {:<12} {}  {}",
                entry.pity,
                entry.name,
                tag,
                entry.time.format("%Y-%m-%d")
            );
        }
    }

    if !stats.unknown_types.is_empty() {
        println!("\n注意：存在未识别的池类型 {:?}，其记录未计入分池统计。", stats.unknown_types);
    }
    println!("\n一抽按 {} 原石折算。", PRIMOGEM_PER_PULL);
    Ok(())
}

fn mask_uid(uid: &str) -> String {
    let chars: Vec<char> = uid.chars().collect();
    if chars.len() <= 4 {
        return "****".to_string();
    }
    let head: String = chars[..chars.len() - 3].iter().collect();
    format!("{head}***")
}

fn pity_color(pity: u32) -> Color {
    if pity >= 74 {
        Color::Red
    } else if pity >= 41 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn pity_bar(pity: u32, width: usize) -> String {
    let filled = ((pity as f64 / 90.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let mut bar = String::new();
    for _ in 0..filled {
        bar.push('█');
    }
    for _ in filled..width {
        bar.push('░');
    }
    bar
}

fn draw(frame: &mut Frame, stats: &AccountStats) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(4),
        Constraint::Min(6),
    ])
    .split(area);

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("提瓦特祈愿分析  ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("UID {}", mask_uid(&stats.uid))),
        ]),
        Line::from(vec![
            Span::raw("欧皇度 "),
            Span::styled(
                stats.rating.label,
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "  {:.1} 分   总抽数 {}   最久 {} 天未抽",
                stats.rating.score, stats.total_pulls, stats.longest_gap_days
            )),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title("概览"));
    frame.render_widget(header, chunks[0]);

    let metrics = Paragraph::new(vec![
        Line::from(format!(
            "不歪率 {:<8} UP角色平均 {:<10} 限定武器平均 {:<10} 连UP {} / 连歪 {}",
            match stats.not_off_rate {
                Some(rate) => format!("{:.1}%", rate * 100.0),
                None => "—".to_string(),
            },
            fmt_opt(stats.avg_up_character_pity, ""),
            fmt_opt(stats.avg_limited_weapon_pity, ""),
            stats.max_up_streak,
            stats.max_off_streak
        )),
        Line::from(format!(
            "平均每个 UP 角色 {} 原石    按 Q 退出",
            stats
                .cost_per_up_character
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string())
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title("指标"));
    frame.render_widget(metrics, chunks[1]);

    let character = stats.pools.iter().find(|pool| pool.kind == PoolKind::Character);
    let mut lines: Vec<Line> = Vec::new();
    if let Some(pool) = character {
        lines.push(Line::from(format!(
            "角色池 {} 抽 / {} 金 / 已垫 {} 抽",
            pool.total, pool.five_star_count, pool.current_pity
        )));
        lines.push(Line::from(""));
        for entry in pool.entries.iter().rev().take(18) {
            let tag = match entry.is_up {
                Some(true) => "UP",
                Some(false) => "歪",
                None => "  ",
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:>3}抽 ", entry.pity),
                    Style::default().fg(pity_color(entry.pity)),
                ),
                Span::styled(pity_bar(entry.pity, 24), Style::default().fg(pity_color(entry.pity))),
                Span::raw(format!(" {:<12} {}", entry.name, tag)),
            ]));
        }
    } else {
        lines.push(Line::from("角色池暂无记录"));
    }

    let history = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("出金历史"))
        .wrap(Wrap { trim: true });
    frame.render_widget(history, chunks[2]);
}

fn run_tui(uid: Option<String>, db: Option<PathBuf>) -> Result<(), String> {
    let (stats, _) = load_stats(uid, db)?;

    let mut terminal = ratatui::init();
    let loop_result = (|| -> Result<(), String> {
        loop {
            terminal
                .draw(|frame| {
                    draw(frame, &stats);
                })
                .map_err(|err| err.to_string())?;

            if event::poll(Duration::from_millis(250)).map_err(|err| err.to_string())? {
                if let Event::Key(key) = event::read().map_err(|err| err.to_string())? {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        return Ok(());
                    }
                }
            }
        }
    })();

    ratatui::restore();
    loop_result
}

// ---------------------------------------------------------------------------
// 演示数据生成：没有有效 authkey 时也能验证统计口径与界面渲染。
// 数据是确定性的（固定种子），同一个 --pulls 每次结果一致。
// ---------------------------------------------------------------------------

const DEMO_UP_CHARACTERS: &[&str] = &[
    "纳西妲",
    "芙宁娜",
    "雷电将军",
    "胡桃",
    "夜兰",
    "那维莱特",
    "仆人",
    "玛拉妮",
];

const DEMO_UP_WEAPONS: &[&str] = &[
    "薙草之稻光",
    "静水流涌之辉",
    "最初的大魔术",
    "万世流涌大典",
    "白雨心弦",
];

const DEMO_FOUR_STARS: &[&str] = &["行秋", "班尼特", "香菱", "菲谢尔", "久岐忍"];

struct Lcg(u64);

impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // 必须取高 32 位：右移 33 位只剩 31 位，值域会塌到 [0, 0.5]，
        // 所有 0.5 以上的概率判定全部失效。
        ((self.0 >> 32) as f64) / (u32::MAX as f64)
    }
}

struct DemoGen {
    uid: String,
    rng: Lcg,
    next_id: u64,
    name_cursor: usize,
    time: NaiveDateTime,
}

impl DemoGen {
    fn new(uid: &str) -> Self {
        let start = NaiveDateTime::parse_from_str("2023-02-13 12:00:00", "%Y-%m-%d %H:%M:%S")
            .expect("演示起始时间必须合法");
        DemoGen {
            uid: uid.to_string(),
            rng: Lcg(0x9E3779B97F4A7C15),
            next_id: 1_700_000_000_000,
            name_cursor: 0,
            time: start,
        }
    }

    fn tick(&mut self) {
        let mut minutes = 30 + (self.rng.unit() * 600.0) as i64;
        // 少数情况下长时间不上线，否则「最久未抽卡持续」永远是 0 天。
        // 概率要压得很低：抽数上千时，3% 的空窗会把时间轴推到好几年后。
        if self.rng.unit() < 0.003 {
            minutes += (((self.rng.unit() * 16.0) as i64) + 5) * 1440;
        }
        self.time += ChronoDuration::minutes(minutes);
    }

    fn pick(&mut self, list: &[&str]) -> String {
        let index = self.name_cursor % list.len();
        self.name_cursor += 1;
        list[index].to_string()
    }

    fn push(&mut self, out: &mut Vec<PullRecord>, gacha_type: u32, rank: u8, name: &str, item_type: &str) {
        self.next_id += 1;
        out.push(PullRecord {
            id: self.next_id.to_string(),
            uid: self.uid.clone(),
            gacha_type,
            item_id: format!("demo{}", self.next_id),
            item_name: name.to_string(),
            item_type: item_type.to_string(),
            rank_type: rank,
            time: self.time,
        });
    }

    fn build(&mut self, gacha_type: u32, count: u32, kind: PoolKind) -> Vec<PullRecord> {
        let mut out: Vec<PullRecord> = Vec::with_capacity(count as usize);
        let mut pity = 0u32;
        let mut made = 0u32;

        while made < count {
            made += 1;
            pity += 1;
            self.tick();

            let mut five_rate = 0.006;
            if pity > 73 {
                five_rate += (pity - 73) as f64 * 0.06;
            }

            if self.rng.unit() < five_rate || pity >= kind.hard_pity() {
                let is_up = kind.has_up() && self.rng.unit() > 0.45;
                let (name, item_type) = if is_up {
                    let list = if kind == PoolKind::Weapon {
                        DEMO_UP_WEAPONS
                    } else {
                        DEMO_UP_CHARACTERS
                    };
                    let picked = self.pick(list);
                    let item_type = if kind == PoolKind::Weapon { "武器" } else { "角色" };
                    (picked, item_type)
                } else if kind == PoolKind::Weapon {
                    let picked = self.pick(gacha_core::pools::STANDARD_FIVE_STAR_WEAPONS);
                    (picked, "武器")
                } else {
                    let picked = self.pick(gacha_core::pools::STANDARD_FIVE_STAR_CHARACTERS);
                    (picked, "角色")
                };
                self.push(&mut out, gacha_type, 5, &name, item_type);
                pity = 0;
            } else if self.rng.unit() < 0.05 {
                let picked = self.pick(DEMO_FOUR_STARS);
                self.push(&mut out, gacha_type, 4, &picked, "角色");
            } else {
                self.push(&mut out, gacha_type, 3, "三星武器", "武器");
            }
        }

        out
    }
}

fn run_demo(db: Option<PathBuf>, pulls: u32) -> Result<(), String> {
    // 不传 --db 时写进应用主库，方便直接验证桌面端显示效果
    let db_path = db.unwrap_or_else(default_db_path);
    let mut store = Store::open(&db_path).map_err(|err| err.to_string())?;

    let uid = "114709792".to_string();
    let mut generator = DemoGen::new(&uid);

    let character_count = (pulls as f64 * 0.55).round() as u32;
    let weapon_count = (pulls as f64 * 0.30).round() as u32;
    let standard_count = pulls.saturating_sub(character_count + weapon_count);

    let mut records: Vec<PullRecord> = Vec::new();
    records.extend(generator.build(301, character_count, PoolKind::Character));
    records.extend(generator.build(302, weapon_count, PoolKind::Weapon));
    records.extend(generator.build(200, standard_count, PoolKind::Standard));

    store
        .upsert_account(&gacha_core::Account {
            uid: uid.clone(),
            nickname: None,
            region: "cn_gf01".to_string(),
            game_biz: "hk4e_cn".to_string(),
        })
        .map_err(|err| err.to_string())?;

    let added = store.insert_pulls(&records).map_err(|err| err.to_string())?;
    store.mark_synced(&uid).map_err(|err| err.to_string())?;

    println!(
        "演示数据已写入 {}：新增 {} 条，合计 {} 条",
        db_path.display(),
        added,
        store.count_pulls(&uid).map_err(|err| err.to_string())?
    );
    Ok(())
}
