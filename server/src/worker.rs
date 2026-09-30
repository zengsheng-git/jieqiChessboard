use std::sync::PoisonError;
use std::thread;
use std::time::Duration;

use tauri::async_runtime::block_on;
use tauri::AppHandle;
use tauri::Emitter as _;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;
use xcap::image::ImageBuffer;
use xcap::image::Rgba;

use crate::chess;
use crate::common;
use crate::engine::DeviationCost;
use crate::engine::QueryResult;
use crate::listen::ListenWindow;
use crate::listen::Window;
use crate::yolo::predict;
use crate::yolo::IMAGE_HEIGHT;
use crate::yolo::IMAGE_WIDTH;
use crate::SHARED_STATE;

// 棋盘分析结果
struct BoardAnalysisResult {
    expect_move: chess::Changed,
    expect_board: [[char; 9]; 10],
}

// 定义不同的棋盘状态
#[derive(PartialEq)]
enum ChessboardState {
    Initial,      // 初始状态，没有进行任何分析
    StartPos,     // 初始棋盘状态
    OurTurn,      // 我方行棋
    OpponentTurn, // 对方行棋
    Invalid,      // 无效状态
}

// 分析上下文，保存分析状态和共享数据
struct AnalysisContext {
    app: AppHandle,
    window: ListenWindow,
    last_board: [[char; 9]; 10],
    expect_move: chess::Changed,
    expect_board: [[char; 9]; 10],
    expect_score: isize,                    // 上次分析的评分(行棋方视角)
    expect_camp: chess::Camp,               // 上次分析的行棋方
    pending_deviation: Option<chess::Camp>, // 刚发生非预期走子的一方, 下次分析时计算代价
    invalid_change_count: usize,
    last_hint_version: u32, // 上次分析时的提示档位版本号, 用于检测切档后立即重算
    account: chess::PieceAccount, // 子力账目: 双方被吃棋子与未翻暗子池
}

unsafe impl Send for AnalysisContext {}
unsafe impl Sync for AnalysisContext {}

impl AnalysisContext {
    fn new(app: AppHandle, window: ListenWindow) -> Self {
        Self {
            app,
            // state_for_thread: state,
            window,
            last_board: [[' '; 9]; 10],
            expect_move: chess::Changed::default(),
            expect_board: [[' '; 9]; 10],
            expect_score: 0,
            expect_camp: chess::Camp::None,
            pending_deviation: None,
            invalid_change_count: 0,
            last_hint_version: crate::hint_version(),
            account: chess::PieceAccount::default(),
        }
    }

    // 检查是否需要终止分析线程
    fn should_stop(&self) -> bool {
        let state = SHARED_STATE.get().unwrap();
        state.listen_thread.lock().map(|slot| slot.is_none()).unwrap_or(true)
    }

    // 确认棋盘状态是否稳定
    fn confirm_board(&self, board: [[char; 9]; 10]) -> bool {
        thread::sleep(Duration::from_millis(100));
        let Some(conf_image) = self.window.capture() else { return false };
        if let Some((_, conf_board)) = get_board(conf_image) {
            return conf_board == board;
        }
        false
    }

    // 分析棋盘并返回结果
    fn analyze_board(&mut self, camp: &chess::Camp, board: [[char; 9]; 10]) -> Option<BoardAnalysisResult> {
        // 池按当前盘面刷新后再生成 FEN, 保证被吃暗子已从未翻池扣除
        self.account.refresh_pool(board);
        let fen = chess::board_fen_account(camp, board, &self.account);
        // 先克隆配置再释放读锁: 搜索可能耗时数秒(云库超时+引擎),
        // 期间不能阻塞设置界面的配置写入命令, 否则改参数/切档位会卡住
        let engine_config = {
            let config = SHARED_STATE.get().unwrap().config.read().unwrap();
            config.engine
        };
        let state = SHARED_STATE.get().unwrap();
        let mut engine = state.engine.lock().unwrap();
        let result = block_on(engine.search(&fen, &engine_config));
        // 本次分析已按当前档位完成, 记录档位版本号, 避免下一拍重复重算
        self.last_hint_version = crate::hint_version();
        result.as_ref()?;

        // 上一步为非预期走子时, 携带当时的预期评分用于计算偏离代价
        let prev = self.pending_deviation.take().and_then(|dev_camp| {
            if self.expect_camp == dev_camp { Some((dev_camp, self.expect_score)) } else { None }
        });

        let result = result.unwrap();
        let score = result.score;
        let (expect_move, expect_board) = analyse(&self.app, result, board, camp, prev, self.account.clone())?;
        // 记录本次预期评分与行棋方, 供下一步偏离对比
        self.expect_score = score;
        self.expect_camp = camp.clone();
        Some(BoardAnalysisResult { expect_move, expect_board })
    }

    // 更新UI显示
    fn update_ui(&self, camp: &chess::Camp, board: [[char; 9]; 10]) {
        let board_map = chess::board_map(board);
        self.app.emit("mirror", camp.is_black()).unwrap();
        self.app.emit("position", &board_map).unwrap();
    }

    // 处理移动事件
    fn handle_move(&mut self, changed: &chess::Changed) { self.app.emit("move", changed).unwrap(); }

    // 按当前档位重算给定行棋方的提示并更新预期(用于切档后棋盘未变时的立即刷新)
    fn reanalyze(&mut self, camp: &chess::Camp) {
        if let Some(result) = self.analyze_board(camp, self.last_board) {
            self.expect_move = result.expect_move;
            self.expect_board = result.expect_board;
        }
    }

    // 分析指定行棋方并更新预期，返回新状态
    fn analyze_and_set_state(&mut self, turn: chess::Camp, board: [[char; 9]; 10], camp: &chess::Camp) -> ChessboardState {
        if let Some(result) = self.analyze_board(&turn, board) {
            self.expect_move = result.expect_move;
            self.expect_board = result.expect_board;
        }
        if turn.eq(camp) {
            ChessboardState::OurTurn
        } else {
            ChessboardState::OpponentTurn
        }
    }

    // 处理一次走棋：更新UI、分析下一行动方并返回新状态
    fn handle_move_and_next(&mut self, changed: &chess::Changed, board: [[char; 9]; 10], camp: &chess::Camp) -> ChessboardState {
        self.last_board = board;
        // 记录本步吃子, 更新子力账目
        if let Some(captured) = changed.captured {
            self.account.record(captured);
        }
        self.handle_move(changed);
        // 本步为非预期走子(与引擎预测不符), 下次分析时计算偏离代价
        self.pending_deviation = Some(changed.camp.clone());
        self.analyze_and_set_state(changed.camp.opposite(), board, camp)
    }

    // 处理错误变化计数
    fn handle_invalid_change(
        &mut self, last_board: [[char; 9]; 10], board: [[char; 9]; 10], camp: &chess::Camp,
    ) -> ChessboardState {
        if self.invalid_change_count < 3 {
            self.invalid_change_count += 1;
            let last_fen = chess::board_fen(camp, last_board);
            let current = chess::board_fen(camp, board);
            debug!("OneChanged last {}", last_fen);
            debug!("OneChanged current {}", current);
            ChessboardState::Invalid
        } else {
            // 如果出现次数超过3次，重置为初始状态
            debug!("OneChanged count=3, reload");
            self.invalid_change_count = 0;
            ChessboardState::Initial
        }
    }
}

pub fn get_board(image: ImageBuffer<Rgba<u8>, Vec<u8>>) -> Option<(chess::Camp, [[char; 9]; 10])> {
    let data = predict(image).unwrap();
    if let Ok((camp, mut board)) = common::detections_to_board(&data) {
        chess::board_fix(&camp, &mut board);
        Some((camp, board))
    } else {
        None
    }
}

// 将引擎结果整理为完整展示数据(中文主线/次优/局面/偏离等), 镜像与推演共用
pub fn prepare_result(
    mut result: QueryResult, board: [[char; 9]; 10], camp: &chess::Camp, prev: Option<(chess::Camp, isize)>,
    mut account: chess::PieceAccount,
) -> Option<(QueryResult, chess::Changed, [[char; 9]; 10])> {
    // 引擎可能返回空着法, 跳过本次展示, 避免监听线程 panic
    let Some(best_pv) = result.pvs.first().cloned() else {
        warn!("分析结果缺少着法, 跳过展示");
        return None;
    };

    // 上一步为非预期走子时, 计算相对预期的亏损(前后评分均为行棋方视角)
    if let Some((dev_camp, prev_score)) = prev
        && dev_camp == camp.opposite()
    {
        result.deviation = Some(DeviationCost { camp: dev_camp.to_char(), loss: prev_score + result.score });
    }
    let best_move = chess::board_move_chinese(board, &best_pv);
    let expect_board = chess::board_move(board, &best_pv);
    let expect_move = chess::Changed::from_pv(&best_pv, board);
    // 携带分析时局面, 供前端主线预演
    result.board = chess::board_map(board);

    let mut tmp_board = expect_board;
    result.moves.push(best_move);
    for pv in result.pvs.iter().skip(1) {
        let mv = chess::board_move_chinese(tmp_board, pv);
        result.moves.push(mv);
        tmp_board = chess::board_move(tmp_board, pv);
    }
    // 次优候选首着（与最优并列，均从当前局面出发）
    for alt in result.alternatives.clone() {
        result.alt_moves.push(chess::board_move_chinese(board, &alt));
    }
    // 标记本次分析的行棋方阵营
    result.camp = camp.to_char();
    // 携带子力账目(按当前盘面刷新未翻池, 保证与局面一致)
    account.refresh_pool(board);
    result.account = account;
    Some((result, expect_move, expect_board))
}

pub fn analyse(app: &AppHandle, result: QueryResult, board: [[char; 9]; 10], camp: &chess::Camp, prev: Option<(chess::Camp, isize)>, account: chess::PieceAccount) -> Option<(chess::Changed, [[char; 9]; 10])> {
    let (result, expect_move, expect_board) = prepare_result(result, board, camp, prev, account)?;
    // 把结果发送给前端
    info!("分析结果 {:?}", result);
    app.emit("analyse", result).unwrap();

    // 返回一个预期move和预期board
    Some((expect_move, expect_board))
}

// 处理循环逻辑的主函数
// 包一层捕获 panic: 线程内任何未预期错误(截图/识别/引擎异常)只终止本次监听,
// 不让 join 方再因线程 panic 而连锁 panic 卡死界面
fn process_analysis_loop_guarded(context: AnalysisContext) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        process_analysis_loop(context);
    }));
    if let Err(e) = result {
        let msg = e.downcast_ref::<String>().map(|s| s.as_str())
            .or_else(|| e.downcast_ref::<&str>().copied())
            .unwrap_or("未知错误");
        error!("分析线程异常退出: {}", msg);
    }
}
fn process_analysis_loop(mut context: AnalysisContext) {
    let mut current_state = ChessboardState::Initial;
    let mut capture_failures = 0usize;

    loop {
        // 检查是否需要停止监听
        if context.should_stop() {
            debug!("listen stopped");
            break;
        }

        // 获取等待间隔
        let interval = SHARED_STATE.get().unwrap().config.read().unwrap().timer_interval;
        thread::sleep(Duration::from_millis(interval));

        // 截图失败多为目标窗口已关闭, 连续失败达到上限则通知前端并结束监听
        let Some(image) = context.window.capture() else {
            capture_failures += 1;
            if capture_failures >= 3 {
                error!("目标窗口连续截图失败, 可能已关闭, 停止监听");
                let _ = context.app.emit("listen_stopped", "目标窗口可能已关闭, 监听已停止");
                break;
            }
            continue;
        };
        capture_failures = 0;

        // 捕获并分析棋盘
        let board_result = get_board(image);
        if board_result.is_none() {
            continue;
        }

        let (camp, board) = board_result.unwrap();
        trace!("{:?} {:?}", camp, board);

        // 根据不同状态处理棋盘
        current_state = match current_state {
            ChessboardState::Initial => {
                // 初始状态，做第一次分析
                debug!("首次启动，立即分析");

                // 设置前端棋盘
                context.update_ui(&camp, board);

                // 确定初始局面轮到谁：初始棋盘红方先手，否则按玩家阵营近似
                let turn = if chess::startpos(board) {
                    chess::Camp::Red
                } else if camp.eq(&chess::Camp::Red) {
                    chess::Camp::Red
                } else {
                    chess::Camp::Black
                };

                // 分析当前行棋方
                if let Some(result) = context.analyze_board(&turn, board) {
                    context.expect_move = result.expect_move;
                    context.expect_board = result.expect_board;
                }

                context.last_board = board;

                // 如果是初始棋盘，进入初始状态，否则进入一般状态
                if chess::startpos(board) {
                    ChessboardState::StartPos
                } else if turn.eq(&camp) {
                    ChessboardState::OurTurn
                } else {
                    ChessboardState::OpponentTurn
                }
            }

            ChessboardState::StartPos => {
                // 判断棋盘是否仍然是初始棋盘
                if !chess::startpos(board) {
                    // 不再是初始棋盘，处理正常的棋局变化
                    if board == context.last_board {
                        ChessboardState::StartPos // 没有变化
                    } else {
                        // 有变化，更新UI并分析
                        let (changed, board_state) = chess::board_diff(context.last_board, board);

                        match board_state {
                            chess::BoardChangeState::Move | chess::BoardChangeState::Flip => {
                                context.handle_move_and_next(&changed, board, &camp)
                            }
                            chess::BoardChangeState::One => {
                                context.handle_invalid_change(context.last_board, board, &camp)
                            }
                            chess::BoardChangeState::Unknown => {
                                debug!("棋局变化未知，重置上下文");
                                context.update_ui(&camp, board);
                                context.last_board = board;
                                // 棋局失联后事件账目不可信, 清空回到减法反推
                                context.account = chess::PieceAccount::default();
                                ChessboardState::Initial
                            }
                        }
                    }
                } else if chess::Camp::Red.eq(&camp) {
                    // 仍然是初始棋盘，且我方先手
                    if context.last_board == board {
                        // 防止重复分析; 但档位已切换时按新档位重算开局提示
                        let version = crate::hint_version();
                        if version != context.last_hint_version {
                            context.last_hint_version = version;
                            info!("提示档位已切换, 按新档位重算当前局面");
                            context.reanalyze(&camp);
                        }
                        ChessboardState::StartPos
                    } else {
                        // 设置前端棋盘
                        context.last_board = board;
                        context.update_ui(&camp, board);

                        // 调用引擎查询
                        if let Some(result) = context.analyze_board(&camp, board) {
                            context.expect_move = result.expect_move;
                            context.expect_board = result.expect_board;
                        }

                        ChessboardState::OurTurn
                    }
                } else {
                    // 对方先手，跳过分析
                    debug!("对方先手，跳过分析");
                    context.last_board = board;
                    context.update_ui(&camp, board);
                    ChessboardState::OpponentTurn
                }
            }

            ChessboardState::OurTurn | ChessboardState::OpponentTurn => {
                // 判断棋盘是否未发生变化
                if board == context.last_board {
                    // 棋盘未变但档位已切换: 立即按新档位重算当前局面提示
                    let version = crate::hint_version();
                    if version != context.last_hint_version {
                        context.last_hint_version = version;
                        info!("提示档位已切换, 按新档位重算当前局面");
                        context.reanalyze(&camp);
                    } else {
                        debug!("棋盘未发生变化，跳过分析");
                    }
                    current_state // 保持当前状态
                } else if board == context.expect_board {
                    // 符合预期棋盘，分析下一行动方
                    debug!("棋盘为预期棋盘，分析下一行动方");
                    let expect_move = context.expect_move.clone();
                    let expect_board = context.expect_board;
                    // 预期着法成真同样可能吃子: 按走子前盘面的落点记录(原地翻子不算)
                    let pre_board = context.last_board;
                    let to_cell = &expect_move.to[..expect_move.to.len().min(2)];
                    if to_cell != expect_move.from
                        && let Some(captured) = chess::piece_at(pre_board, to_cell)
                    {
                        context.account.record(captured);
                    }
                    context.last_board = expect_board;
                    context.handle_move(&expect_move);
                    context.analyze_and_set_state(expect_move.camp.opposite(), expect_board, &camp)
                } else {
                    // 确认棋盘变化是否稳定
                    if !context.confirm_board(board) {
                        debug!("棋盘延迟确认失败");
                        let confirm_interval = SHARED_STATE.get().unwrap().config.read().unwrap().confirm_interval;
                        thread::sleep(Duration::from_millis(confirm_interval));
                        current_state // 保持当前状态
                    } else if !chess::board_check(board) {
                        // 检测棋盘是否有效
                        let debug_fen = chess::board_fen(&camp, board);
                        debug!("棋盘识别无效: {}", debug_fen);
                        current_state // 保持当前状态
                    } else {
                        // 处理正常棋盘变化
                        let (changed, board_state) = chess::board_diff(context.last_board, board);

                        match board_state {
                            chess::BoardChangeState::Move | chess::BoardChangeState::Flip => {
                                context.handle_move_and_next(&changed, board, &camp)
                            }
                            chess::BoardChangeState::One => {
                                context.handle_invalid_change(context.last_board, board, &camp)
                            }
                            chess::BoardChangeState::Unknown => {
                                debug!("棋局变化未知，重置上下文");
                                context.update_ui(&camp, board);
                                context.last_board = board;
                                // 棋局失联后事件账目不可信, 清空回到减法反推
                                context.account = chess::PieceAccount::default();
                                ChessboardState::Initial
                            }
                        }
                    }
                }
            }

            ChessboardState::Invalid => {
                // 复位到初始状态，等待下一次有效的变化
                ChessboardState::Initial
            }
        };
    }
}

fn dump_capture_debug(image: &ImageBuffer<Rgba<u8>, Vec<u8>>, detections: &[crate::yolo::Detection]) {
    let path = std::env::current_dir().unwrap_or_default().join("debug_capture.png");
    match image.save(&path) {
        Ok(()) => warn!("dumped capture to {}", path.display()),
        Err(err) => warn!("dump capture failed: {}", err),
    }
    let mut stats: std::collections::BTreeMap<char, (usize, f32)> = std::collections::BTreeMap::new();
    for det in detections {
        let entry = stats.entry(det.label).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 = entry.1.max(det.confidence);
    }
    warn!("detections total={}, per-label (count,max_conf)={:?}", detections.len(), stats);
}

// 初始化Tauri的command处理
#[tauri::command]
pub async fn start_listen(app: AppHandle, target: Window) -> Result<(), String> {
    trace!("start_listen");
    {
        // 先检查再占用, 整个过程持锁防止并发双开
        let shared_state = SHARED_STATE.get().unwrap();
        let mut listen_slot = shared_state
            .listen_thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if listen_slot.is_some() {
            error!("current listen thread is running, please stop it first");
            return Err("已经在监听中".to_string());
        }

        // 初始化监听窗口模块
        let Some(mut window) = ListenWindow::new(&target, IMAGE_WIDTH, IMAGE_HEIGHT) else {
            return Err("目标窗口不存在".to_string());
        };
        let Some(image) = window.capture() else {
            return Err("无法截取目标窗口画面".to_string());
        };

        let image_h = image.height();
        let image_w = image.width();

        let detections = match predict(image.clone()) {
            Ok(detections) => detections,
            Err(err) => return Err(format!("棋子识别失败: {err}")),
        };

        match common::detections_bound(image_w, image_h, &detections) {
            Ok((x, y, w, h)) => {
                window.set_sub_bound(x, y, w, h); // 设置窗口边界
            }
            Err(e) => {
                dump_capture_debug(&image, &detections);
                return Err(e); // 未识别到棋盘
            }
        }

        // 创建分析上下文
        let context = AnalysisContext::new(app.clone(), window);

        // 启动后台线程进行截图和处理
        let listen_thread = thread::spawn(move || {
            trace!("into thread");
            process_analysis_loop_guarded(context);
        });

        *listen_slot = Some(listen_thread);
    }

    Ok(())
}

#[tauri::command]
pub fn stop_listen() {
    info!("stop listen");
    let shared_state = SHARED_STATE.get().unwrap();
    if let Ok(mut state) = shared_state.listen_thread.lock()
        && let Some(listen_thread) = state.take()
    {
        // 释放锁，停止后台线程
        debug!("释放锁，停止后台线程");
        drop(state);
        // join 移到独立线程: 后台线程最多在下一个循环检查点退出(通常数百毫秒),
        // 但停止命令必须立即返回, 保证界面永不因停止操作而冻结
        thread::spawn(move || {
            if let Err(e) = listen_thread.join() {
                error!("等待监听线程退出失败: {:?}", e);
            }
        });
    }
    debug!("stoped");
}
