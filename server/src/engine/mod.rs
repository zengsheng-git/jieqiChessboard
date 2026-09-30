use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::ChildStdin;
use std::sync::Mutex;

use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;

pub(crate) mod command;

// 全局 stop 写入口: 每次引擎管道创建(启动/重载/自愈重建)时自动刷新,
// 供演练模块在不持有引擎锁的情况下中断搜索
static STOP_WRITER: Mutex<Option<ChildStdin>> = Mutex::new(None);

// 中断当前引擎搜索; 引擎空闲时该命令无副作用
pub fn interrupt_search() {
    if let Ok(mut writer) = STOP_WRITER.lock()
        && let Some(w) = writer.as_mut()
    {
        let _ = w.write_all(b"stop\n");
        let _ = w.flush();
    }
}

#[derive(Debug, serde::Serialize, Default, Clone)]
pub struct QueryResult {
    pub depth: usize,            // 深度
    pub score: isize,            // 得分
    pub time: usize,             // 时间
    pub pvs: Vec<String>,        // 最优线完整着法(iccs)
    pub moves: Vec<String>,      // 最优线完整着法(chinese)
    pub alternatives: Vec<String>, // 次优候选首着(iccs)
    pub alt_scores: Vec<isize>,  // 次优候选与最优的分差, 负值=未知
    pub alt_moves: Vec<String>,  // 次优候选首着(chinese)
    pub board: Vec<crate::chess::Position>, // 分析时的局面, 供前端主线预演
    pub winrate: Option<usize>,  // 行棋方胜率(千分比, 仅引擎来源提供)
    pub deviation: Option<DeviationCost>, // 上一步非预期走子的代价
    pub account: crate::chess::PieceAccount, // 子力账目: 双方被吃棋子与未翻暗子池
    pub source: String,          // 来源
    pub camp: char,              // 行棋方阵营 'w'/'b'
}

// 上一步偏离预期的代价
#[derive(Debug, serde::Serialize, Clone, Copy)]
pub struct DeviationCost {
    pub camp: char,  // 偏离预期的一方 'w'/'b'
    pub loss: isize, // 相对预期的分差(偏离方视角, 正=亏损)
}

const SOURCE_ENGINE: &str = "引擎";

// 引擎单行 info 解析结果
#[derive(Default)]
struct InfoLine {
    multipv: Option<usize>,
    pvs: Vec<String>,
    depth: usize,
    score: isize,
    time: usize,
    winrate: Option<usize>,
}

#[derive(Debug, serde::Serialize, Clone, serde::Deserialize, Copy)]
pub struct EngineConfig {
    pub depth: usize,
    pub time: usize,
    pub threads: usize,
    pub hash: usize,
    pub multipv: usize,
    pub show_wdl: bool,
    #[serde(default = "default_alt_score_gap")]
    pub alt_score_gap: isize,
    // 提示强度档位: 0=不限棋力(跟随深度/时间设置并优先云库); >0 时用固定低深度搜索, 每一手都是一致的中低水平
    #[serde(default)]
    pub hint_level: i32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            depth: 20,
            time: 5000,
            threads: 4,
            hash: 64,
            multipv: 3,
            show_wdl: true,
            alt_score_gap: 300,
            hint_level: 0,
        }
    }
}

fn default_alt_score_gap() -> isize {
    300
}

// 提示档位对应的固定搜索深度: 统一浅深度出招, 保证整体一致的较低水平(而非概率放水)
// 自对弈实测: 深度2 约15回合被深度12将杀, 深度4 约27回合落败, 档位以此为梯度
fn hint_depth(level: i32) -> Option<usize> {
    match level {
        1 => Some(2),   // 新手: 会有明显漏着
        2 => Some(4),   // 入门: 不送子但缺战术远见
        3 => Some(6),   // 初级
        4 => Some(8),   // 中级
        5 => Some(12),  // 高级: 接近满强度但残局精度不足
        _ => None,
    }
}

// 引擎的底层管道(进程/输入/输出), 引擎故障时整体替换以实现自愈
struct EnginePipe {
    stdin: Box<dyn Write>,
    stdout: Box<dyn BufRead>,
    child: Child,
}

// 引擎进程启动后立即下发的基础选项, 重建时按此恢复
#[derive(Clone, Copy)]
struct BaseOptions {
    show_wdl: bool,
    hash: usize,
    threads: usize,
}

impl BaseOptions {
    fn apply(&self, pipe: &mut EnginePipe, libs: &Path) {
        let nnue = libs.join("pikajieqi.nnue");
        let _ = pipe.setoption("EvalFile", nnue.display().to_string());
        let _ = pipe.setoption("UCI_ShowWDL", self.show_wdl);
        let _ = pipe.setoption("Hash", self.hash);
        let _ = pipe.setoption("Threads", self.threads);
    }
}

// 引擎交互层: 管道 + 库路径/基础配置(用于故障时自动重建子进程)
pub struct Engine {
    pipe: Option<EnginePipe>,
    libs: PathBuf,
    base: BaseOptions,
    last_level: i32,
}

unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl EnginePipe {
    fn spawn(libs: &Path) -> std::io::Result<Self> {
        let (mut child, stdin, stop) = command::new(libs)?;
        // stop 写入口移入全局槽位, 供演练模块在不持有引擎锁的情况下中断搜索;
        // 每次 spawn(启动/重载/自愈重建)都会刷新, 保证始终指向存活进程
        if let Ok(mut guard) = STOP_WRITER.lock() {
            *guard = stop;
        }
        Ok(Self {
            stdin: Box::new(stdin),
            stdout: Box::new(BufReader::new(child.stdout.take().unwrap())),
            child,
        })
    }

    fn write_command<A: std::fmt::Display>(&mut self, args: A) -> std::io::Result<()> {
        writeln!(self.stdin, "{}", args)?;
        self.stdin.flush()?;
        debug!("{}", args);
        Ok(())
    }

    // 与引擎同步: 发送 isready 并等待 readyok, 保证此前命令(setoption/ucinewgame)已处理完毕
    fn sync(&mut self) -> Option<()> {
        self.write_command("isready").ok()?;
        loop {
            let line = self.read_line()?;
            if line == "readyok" {
                return Some(());
            }
        }
    }

    fn setoption<T: std::fmt::Display>(&mut self, name: &str, value: T) -> std::io::Result<()> {
        self.write_command(format!("setoption name {} value {}", name, value))
    }

    // 读取一行引擎输出; 引擎进程退出(EOF)或管道错误时返回 None
    // 注意: 绝不能在 EOF 时继续等待, 否则将永远等不到 bestmove 造成线程死循环
    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) => {
                error!("引擎输出流已关闭(进程可能已退出)");
                None
            }
            Ok(_) => {
                trace!("line::{}", line);
                Some(line.trim().to_string())
            }
            Err(e) => {
                error!("读取引擎输出失败: {}", e);
                None
            }
        }
    }

    // 执行引擎搜索，返回解析后的结果（含多候选）; 引擎故障返回 None
    fn bestmove(&mut self, depth: usize, time: usize, multipv: usize, alt_score_gap: isize) -> Option<QueryResult> {
        self.setoption("MultiPV", multipv).ok()?;
        self.write_command(format!("go depth {} movetime {}", depth, time)).ok()?;

        let mut result = QueryResult::default();
        result.source = SOURCE_ENGINE.to_string();

        // multipv编号 -> 该候选的完整pv序列与分数
        let mut pvs_by_id: std::collections::BTreeMap<usize, (Vec<String>, isize)> = std::collections::BTreeMap::new();
        let mut last_pvs: Vec<String> = Vec::new();

        loop {
            let line = self.read_line()?;
            if line.starts_with("bestmove") {
                break;
            }
            if !line.starts_with("info") {
                continue;
            }
            let info = parse_info(&line);
            if info.pvs.is_empty() {
                continue;
            }
            last_pvs = info.pvs.clone();
            match info.multipv {
                Some(id) => {
                    pvs_by_id.insert(id, (info.pvs, info.score));
                    if id == 1 {
                        result.depth = info.depth;
                        result.score = info.score;
                        result.time = info.time;
                        result.winrate = info.winrate;
                    }
                }
                None => {
                    // 无 multipv 字段，视为单候选
                    result.depth = info.depth;
                    result.score = info.score;
                    result.time = info.time;
                    result.winrate = info.winrate;
                }
            }
        }

        // 最优线
        if let Some((pv1, _)) = pvs_by_id.remove(&1) {
            result.pvs = pv1;
        } else {
            result.pvs = last_pvs;
        }
        // 次优候选首着（只保留分数接近最优的好招）
        for (pvs, sc) in pvs_by_id.values() {
            if let Some(first) = pvs.first()
                && *sc >= result.score - alt_score_gap
            {
                result.alternatives.push(first.clone());
                result.alt_scores.push(result.score - sc);
            }
        }
        Some(result)
    }
}

// 解析一行 info
fn parse_info(line: &str) -> InfoLine {
    let mut iter = line.split_whitespace();
    // 跳过 "info"
    iter.next();
    let mut info = InfoLine::default();
    loop {
        let Some(key) = iter.next() else { break };
        match key {
            "depth" => info.depth = iter.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "time" => info.time = iter.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "multipv" => info.multipv = iter.next().and_then(|v| v.parse().ok()),
            "wdl" => {
                // wdl 为胜/和/负千分比, 取行棋方胜率
                let win: usize = iter.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let _draw: usize = iter.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let _loss: usize = iter.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                info.winrate = Some(win);
            }
            "score" => match iter.next().unwrap_or("") {
                "cp" => info.score = iter.next().and_then(|v| v.parse().ok()).unwrap_or(0),
                "mate" => {
                    let round: isize = iter.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    info.score = if round > 0 { 30000 - round } else { -(30000 + round) };
                }
                _ => {}
            },
            "pv" => {
                // pv 是 info 行最后一个字段，收集剩余所有着法
                info.pvs.extend(iter.by_ref().map(|s| s.to_string()));
                break;
            }
            _ => {}
        }
    }
    info
}

impl Engine {
    // 启动引擎子进程; 失败(引擎文件缺失/被杀毒拦截)时返回 Err
    pub fn new(libs: &Path) -> std::io::Result<Self> {
        let pipe = EnginePipe::spawn(libs)?;
        Ok(Self {
            pipe: Some(pipe),
            libs: libs.to_path_buf(),
            base: BaseOptions { show_wdl: false, hash: 64, threads: 1 },
            last_level: -1,
        })
    }

    // 检查引擎进程存活; 已退出则自动重建并恢复基础选项
    // 返回 false 表示重建也失败, 本次搜索放弃
    fn ensure_alive(&mut self) -> bool {
        if let Some(pipe) = self.pipe.as_mut() {
            match pipe.child.try_wait() {
                // 进程存活
                Ok(None) => return true,
                Ok(Some(status)) => info!("引擎进程已退出({status}), 尝试自动重建"),
                Err(e) => info!("引擎进程状态获取失败({e}), 尝试自动重建"),
            }
            let _ = pipe.child.wait();
            self.pipe = None;
        }
        match EnginePipe::spawn(&self.libs) {
            Ok(mut pipe) => {
                self.base.apply(&mut pipe, &self.libs);
                self.pipe = Some(pipe);
                info!("引擎进程已重建");
                true
            }
            Err(e) => {
                error!("引擎进程重建失败: {}", e);
                false
            }
        }
    }

    fn kill_current(&mut self) {
        if let Some(mut pipe) = self.pipe.take() {
            let _ = pipe.child.kill();
            let _ = pipe.child.wait();
        }
    }

    pub fn shutdown(&mut self) {
        self.kill_current();
    }

    // 重建引擎子进程并应用完整配置
    pub fn reload(&mut self, libs: &Path, config: &EngineConfig) {
        self.kill_current();
        match EnginePipe::spawn(libs) {
            Ok(mut pipe) => {
                self.libs = libs.to_path_buf();
                self.base = BaseOptions { show_wdl: config.show_wdl, hash: config.hash, threads: config.threads };
                self.base.apply(&mut pipe, &self.libs);
                self.pipe = Some(pipe);
            }
            Err(e) => {
                error!("引擎重载失败: {}", e);
                self.pipe = None;
            }
        }
    }

    pub fn set_show_wdl(&mut self, open: bool) {
        self.base.show_wdl = open;
        if let Some(pipe) = self.pipe.as_mut() {
            let _ = pipe.setoption("UCI_ShowWDL", open);
        }
    }

    pub fn set_threads(&mut self, num: usize) {
        self.base.threads = num;
        if let Some(pipe) = self.pipe.as_mut() {
            let _ = pipe.setoption("Threads", num);
        }
    }

    pub fn set_hash(&mut self, size: usize) {
        self.base.hash = size;
        if let Some(pipe) = self.pipe.as_mut() {
            let _ = pipe.setoption("Hash", size);
        }
    }

    pub async fn search(&mut self, fen: &str, params: &EngineConfig) -> Option<QueryResult> {
        // 档位模式: 用固定浅深度搜索, 每一手都是一致的中低水平
        let mut params = params.clone();
        if let Some(d) = hint_depth(params.hint_level) {
            params.depth = d;
        }

        // 引擎可用性前置检查: 已退出则自动重建, 重建失败(引擎文件缺失等)放弃本次搜索
        // 此前引擎意外退出会让 bestmove 循环永远等不到 bestmove, 监听线程死循环并卡死停止操作
        if !self.ensure_alive() {
            return None;
        }

        // 档位切换时清空置换表(0=不限深度也参与比较): 深档/不限留下的TT条目会让浅档搜索
        // 直接命中深层旧结论, 出招仍然过强("记住强招"的元凶)
        // 注意: 条件必须是 last_level != hint_level, 若只比较 hint_level>0 的档位,
        // "不限(0) <-> 小白(1)"来回切换时 last_level 不更新, 清表将被完全跳过
        if self.last_level != params.hint_level {
            let pipe = self.pipe.as_mut()?;
            if pipe.write_command("ucinewgame").is_ok() && pipe.sync().is_some() {
                info!("检测到档位切换({} -> {}), 已清空引擎置换表", self.last_level, params.hint_level);
            }
            self.last_level = params.hint_level;
        }

        let pipe = self.pipe.as_mut()?;
        pipe.write_command(format!("position fen {}", fen)).ok()?;
        pipe.bestmove(params.depth, params.time, params.multipv, params.alt_score_gap)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.kill_current();
    }
}

#[cfg(test)]
mod tests {
    use std::path;

    use tracing::info;
    use tracing::Level;

    use super::*;
    use crate::logger;

    #[tokio::test]
    async fn test_engine() {
        logger::init_tracer(Level::TRACE, &std::path::PathBuf::from("."));
        let fen = "xxxxkxxxx/9/1x5x1/x1x1x1x1x/9/9/X1X1X1X1X/1X5X1/9/XXXXKXXXX w R2A2C2P5N2B2r2a2c2p5n2b2 0 1";
        let libs = path::PathBuf::from("../libs/pikajieqi");
        let mut eng = Engine::new(&libs).unwrap();
        let records = eng.search(fen, &Default::default()).await;
        info!("{:?}", records);
    }
}
