use std::sync::Mutex;

use serde::Serialize;
use tauri::async_runtime::block_on;

use crate::attack;
use crate::chess::{self, Camp, Position};
use crate::engine::interrupt_search;
use crate::engine::QueryResult;
use crate::worker::prepare_result;
use crate::SHARED_STATE;

pub type Board = [[char; 9]; 10];

// 演练会话: 保存起始局面与已走着法, 当前局面按着法重放推导
pub struct PracticeSession {
    start_board: Board,
    start_camp: Camp,
    history: Vec<String>,
}

impl PracticeSession {
    // 重放着法得到当前局面与行棋方
    fn state(&self) -> (Board, Camp) {
        let mut board = self.start_board;
        let mut camp = self.start_camp.clone();
        for mv in &self.history {
            board = chess::board_move(board, mv);
            camp = camp.opposite();
        }
        (board, camp)
    }
}

static PRACTICE: Mutex<Option<PracticeSession>> = Mutex::new(None);

#[derive(Serialize)]
pub struct PracticeState {
    pub board: Vec<Position>,   // 当前局面
    pub camp: char,             // 当前行棋方
    pub moves: Vec<String>,     // 当前行棋方合法着法(iccs)
    pub history: Vec<String>,   // 已走着法(iccs)
}

#[derive(Serialize)]
pub struct MoveOutcome {
    pub notation: String,       // 本步中文记谱
    pub state: PracticeState,
}

fn practice_state(session: &PracticeSession) -> PracticeState {
    let (board, camp) = session.state();
    PracticeState {
        board: chess::board_map(board),
        camp: camp.to_char(),
        moves: attack::legal_moves(&board, &camp),
        history: session.history.clone(),
    }
}

fn with_session<T>(f: impl FnOnce(&PracticeSession) -> T) -> Result<T, String> {
    let session = PRACTICE.lock().unwrap();
    let session = session.as_ref().ok_or("尚未开始演练")?;
    Ok(f(session))
}

fn board_from_positions(positions: &[Position]) -> Result<Board, String> {
    let mut board = [[' '; 9]; 10];
    for p in positions {
        let bytes = p.pos.as_bytes();
        if bytes.len() != 2 {
            return Err("坐标格式错误".to_string());
        }
        let x = (bytes[0] as char) as usize - 97;
        let y = 57 - (bytes[1] as char) as usize;
        if x > 8 || y > 9 {
            return Err("坐标越界".to_string());
        }
        board[y][x] = p.piece;
    }
    Ok(board)
}

#[tauri::command]
pub fn practice_start(board: Vec<Position>, camp: String) -> Result<PracticeState, String> {
    let start_board = board_from_positions(&board)?;
    let start_camp = match camp.as_str() {
        "w" => Camp::Red,
        "b" => Camp::Black,
        _ => return Err("阵营错误".to_string()),
    };
    let session = PracticeSession { start_board, start_camp, history: Vec::new() };
    let state = practice_state(&session);
    *PRACTICE.lock().unwrap() = Some(session);
    Ok(state)
}

#[tauri::command]
pub fn practice_moves() -> Result<Vec<String>, String> {
    with_session(|session| {
        let (board, camp) = session.state();
        attack::legal_moves(&board, &camp)
    })
}

#[tauri::command]
pub fn practice_do_move(from: String, to: String, interrupt: Option<bool>) -> Result<MoveOutcome, String> {
    // 引擎思考中走子: 中断当前搜索, 让位给新局面的分析
    if interrupt.unwrap_or(false) {
        interrupt_search();
    }
    let mv = format!("{from}{to}");
    let mut session = PRACTICE.lock().unwrap();
    let session = session.as_mut().ok_or("尚未开始演练")?;
    let (board, camp) = session.state();
    let legal = attack::legal_moves(&board, &camp);
    if !legal.contains(&mv) {
        return Err("非法着法".to_string());
    }
    let notation = chess::board_move_chinese(board, &mv);
    session.history.push(mv);
    Ok(MoveOutcome { notation, state: practice_state(session) })
}

#[tauri::command]
pub fn practice_undo() -> Result<PracticeState, String> {
    let mut session = PRACTICE.lock().unwrap();
    let session = session.as_mut().ok_or("尚未开始演练")?;
    if session.history.is_empty() {
        return Err("没有可撤销的着法".to_string());
    }
    session.history.pop();
    Ok(practice_state(session))
}

#[tauri::command]
pub fn practice_reset() -> Result<PracticeState, String> {
    let mut session = PRACTICE.lock().unwrap();
    let session = session.as_mut().ok_or("尚未开始演练")?;
    session.history.clear();
    Ok(practice_state(session))
}

// 对演练盘当前局面做一次云库优先的分析, 返回行棋方视角的完整提示数据
#[tauri::command]
pub async fn practice_analyze() -> Result<QueryResult, String> {
    let state = SHARED_STATE.get().unwrap();
    let config = state.config.read().unwrap().engine.clone();
    let (fen, board, camp) = with_session(|session| {
        let (board, camp) = session.state();
        (chess::board_fen(&camp, board), board, camp)
    })?;
    let engine = state.engine.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut engine = engine.lock().map_err(|_| "引擎锁不可用".to_string())?;
        block_on(engine.search(&fen, &config)).ok_or_else(|| "分析失败".to_string())
    })
    .await
    .map_err(|e| format!("分析任务失败: {e}"))??;
    match prepare_result(result, board, &camp, None) {
        Some((result, _, _)) => Ok(result),
        None => Err("分析结果缺少着法".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_replay() {
        let positions = vec![
            Position { piece: 'k', pos: "e9".to_string() },
            Position { piece: 'K', pos: "e0".to_string() },
            Position { piece: 'R', pos: "a0".to_string() },
        ];
        let start_board = board_from_positions(&positions).unwrap();
        let mut session = PracticeSession {
            start_board,
            start_camp: Camp::Red,
            history: Vec::new(),
        };
        let (board, camp) = session.state();
        assert_eq!(camp, Camp::Red);
        assert_eq!(board[9][0], 'R');
        // 车 a0 -> a5 -> a1 两步, 走完后轮到红方
        session.history.push("a0a5".to_string());
        session.history.push("a5a1".to_string());
        let (board, camp) = session.state();
        assert_eq!(camp, Camp::Red);
        assert_eq!(board[9][0], ' ');
        assert_eq!(board[8][0], 'R');
    }
}
