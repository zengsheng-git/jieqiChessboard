use std::cmp::Ordering;
use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;
use tracing::warn;

pub const BOARD_MAP: [[&str; 9]; 10] = [
    ["a9", "b9", "c9", "d9", "e9", "f9", "g9", "h9", "i9"],
    ["a8", "b8", "c8", "d8", "e8", "f8", "g8", "h8", "i8"],
    ["a7", "b7", "c7", "d7", "e7", "f7", "g7", "h7", "i7"],
    ["a6", "b6", "c6", "d6", "e6", "f6", "g6", "h6", "i6"],
    ["a5", "b5", "c5", "d5", "e5", "f5", "g5", "h5", "i5"],
    ["a4", "b4", "c4", "d4", "e4", "f4", "g4", "h4", "i4"],
    ["a3", "b3", "c3", "d3", "e3", "f3", "g3", "h3", "i3"],
    ["a2", "b2", "c2", "d2", "e2", "f2", "g2", "h2", "i2"],
    ["a1", "b1", "c1", "d1", "e1", "f1", "g1", "h1", "i1"],
    ["a0", "b0", "c0", "d0", "e0", "f0", "g0", "h0", "i0"],
];

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct Position {
    pub piece: char,
    pub pos: String,
}

#[derive(Debug)]
pub enum BoardChangeState {
    // 变化了一个棋子
    One,
    // 暗子翻开(单格由 X/x 变为明子)
    Flip,
    // 正常一步棋移动
    Move,
    // 未知多个变化
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Default, Clone, Serialize)]
pub enum Camp {
    #[default]
    None,
    Red,
    Black,
}

impl Camp {
    pub fn to_char(&self) -> char {
        match self {
            Camp::None => '0',
            Camp::Red => 'w',
            Camp::Black => 'b',
        }
    }

    pub fn from_piece(p: char) -> Self {
        if p > 'Z' {
            Self::Black
        } else {
            Self::Red
        }
    }

    pub fn is_black(&self) -> bool { Camp::Black.eq(self) }

    pub fn opposite(&self) -> Self {
        match self {
            Camp::Red => Camp::Black,
            Camp::Black => Camp::Red,
            Camp::None => Camp::None,
        }
    }
}

const BLACK_VERTICALS: [char; 9] = ['1', '2', '3', '4', '5', '6', '7', '8', '9'];
const RED_VERTICALS: [char; 9] = ['九', '八', '七', '六', '五', '四', '三', '二', '一'];

// 揭棋开局: 帅/将明示, 其余 15 子暗置 (X=红暗子, x=黑暗子)
const JIEQI_STARTPOS: [[char; 9]; 10] = [
    ['x', 'x', 'x', 'x', 'k', 'x', 'x', 'x', 'x'],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    [' ', 'x', ' ', ' ', ' ', ' ', ' ', 'x', ' '],
    ['x', ' ', 'x', ' ', 'x', ' ', 'x', ' ', 'x'],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    ['X', ' ', 'X', ' ', 'X', ' ', 'X', ' ', 'X'],
    [' ', 'X', ' ', ' ', ' ', ' ', ' ', 'X', ' '],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    ['X', 'X', 'X', 'X', 'K', 'X', 'X', 'X', 'X'],
];

// 起始位名义类型: 暗子未翻开时按初始排列的类型记谱
const NOMINAL_STARTPOS: [[char; 9]; 10] = [
    ['r', 'n', 'b', 'a', 'k', 'a', 'b', 'n', 'r'],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    [' ', 'c', ' ', ' ', ' ', ' ', ' ', 'c', ' '],
    ['p', ' ', 'p', ' ', 'p', ' ', 'p', ' ', 'p'],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    ['P', ' ', 'P', ' ', 'P', ' ', 'P', ' ', 'P'],
    [' ', 'C', ' ', ' ', ' ', ' ', ' ', 'C', ' '],
    [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
    ['R', 'N', 'B', 'A', 'K', 'A', 'B', 'N', 'R'],
];

pub fn get_verticals(p: char) -> [char; 9] {
    if p > 'Z' {
        BLACK_VERTICALS
    } else {
        RED_VERTICALS
    }
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct Changed {
    pub piece: char,
    pub camp: Camp,
    pub from: String,
    pub to: String,
    pub captured: Option<char>, // 被吃子: None=未吃子, X/x=未翻暗子被吃(类型未知), 其余=被吃明子
}

impl Changed {
    pub fn from_pv(pv: &str, board: [[char; 9]; 10]) -> Self {
        let (from, to) = pv.split_at(2);
        let mut cs = pv.chars();
        let from_x = cs.next().unwrap() as usize - 97;
        let from_y = 57 - cs.next().unwrap() as usize;
        let piece = board[from_y][from_x];
        Self {
            piece,
            camp: Camp::from_piece(piece),
            from: from.to_string(),
            to: to.to_string(),
            captured: None,
        }
    }
}

// 对比棋盘, 返回值是发生变化的索引
pub fn board_diff(old_board: [[char; 9]; 10], board: [[char; 9]; 10]) -> (Changed, BoardChangeState) {
    let mut changed = Changed::default();
    let mut count = 0;
    let mut flipped = false;
    let mut old_to = ' '; // 落点格原有棋子, 用于判定吃子
    let mut to_new = ' '; // 落点格新棋子, 走子后即为最终形态(暗子吃子强制翻开)
    for y in 0..10 {
        for x in 0..9 {
            if old_board[y][x] != board[y][x] {
                count += 1;
                match board[y][x] {
                    ' ' => {
                        changed.piece = old_board[y][x];
                        changed.from = BOARD_MAP[y][x].to_string();
                        changed.camp = Camp::from_piece(changed.piece);
                    }
                    new_piece => {
                        changed.to = BOARD_MAP[y][x].to_string();
                        old_to = old_board[y][x];
                        to_new = new_piece;
                        // 暗子翻转为明子: 记为翻子变化
                        if old_board[y][x] == 'X' || old_board[y][x] == 'x' {
                            flipped = true;
                            changed.piece = new_piece;
                            changed.camp = Camp::from_piece(new_piece);
                        }
                    }
                }
            }
        }
    }

    match count {
        1 => {
            if flipped {
                // 原地翻子: 起终点为同一格
                changed.from = changed.to.clone();
                (changed, BoardChangeState::Flip)
            } else {
                (changed, BoardChangeState::One)
            }
        }
        2 => {
            if changed.from.is_empty() || changed.to.is_empty() {
                (changed, BoardChangeState::One)
            } else {
                // 走子且落点原有棋子即为被吃子(X/x 为未翻暗子)
                if old_to != ' ' {
                    changed.captured = Some(old_to);
                }
                // 落点棋子即走子后的最终形态, 保证吃子翻子时前端按翻开后的类型落子
                changed.piece = to_new;
                changed.camp = Camp::from_piece(to_new);
                (changed, BoardChangeState::Move)
            }
        }
        _ => (changed, BoardChangeState::Unknown),
    }
}

pub struct Move {
    from_x: usize,
    from_y: usize,
    to_x: usize,
    to_y: usize,
}

impl Move {
    pub fn new(iccs: &str) -> Self {
        let mut cs = iccs.chars();
        let from_x = cs.next().unwrap() as usize - 97;
        let from_y = 57 - cs.next().unwrap() as usize;
        let to_x = cs.next().unwrap() as usize - 97;
        let to_y = 57 - cs.next().unwrap() as usize;
        Self { from_x, from_y, to_x, to_y }
    }
}

pub fn board_move(board: [[char; 9]; 10], iccs: &str) -> [[char; 9]; 10] {
    let mv = Move::new(iccs);
    let mut new_board = board;
    let mut p = new_board[mv.from_y][mv.from_x];
    // 揭棋翻子着法(iccs 第5字符为翻开后的棋子类型): 用真实类型落盘
    if let Some(flip) = iccs.chars().nth(4) {
        p = flip;
    }
    new_board[mv.to_y][mv.to_x] = p;
    // 原地翻子(from==to)时不清除源格
    if mv.from_x != mv.to_x || mv.from_y != mv.to_y {
        new_board[mv.from_y][mv.from_x] = ' ';
    }
    new_board
}

// 未翻暗子池的兵种顺序与初始数量
// 顺序与 Pikafish jieqi StartFEN 一致: 红方 RACPNB 在前, 黑方 racpnb 在后, 数量为 0 省略
const POOL_ORDER: [char; 6] = ['R', 'A', 'C', 'P', 'N', 'B'];
const POOL_INIT: [usize; 6] = [2, 2, 2, 5, 2, 2];

// 子力账目: 跟踪双方被吃棋子与未翻开的暗子池
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct PieceAccount {
    pub captured_red: Vec<char>,   // 红方被吃的明子
    pub captured_black: Vec<char>, // 黑方被吃的明子
    pub red_hidden_lost: usize,    // 红方未翻即被吃的暗子数(类型未知)
    pub black_hidden_lost: usize,  // 黑方未翻即被吃的暗子数(类型未知)
    pub pool_red: Vec<(char, usize)>,   // 红方未翻暗子池(已扣除暗损)
    pub pool_black: Vec<(char, usize)>, // 黑方未翻暗子池
}

impl PieceAccount {
    // 记录一颗被吃子: X/x 为未翻暗子(类型未知), 其余按明子记账
    pub fn record(&mut self, captured: char) {
        match captured {
            'X' => self.red_hidden_lost += 1,
            'x' => self.black_hidden_lost += 1,
            p if p.is_ascii_uppercase() => self.captured_red.push(p),
            p => self.captured_black.push(p),
        }
    }

    // 未翻暗子池 = 初始配置 - 盘上已翻明子 - 未翻即被吃的暗子
    pub fn refresh_pool(&mut self, board: [[char; 9]; 10]) {
        let mut red = POOL_INIT;
        let mut black = POOL_INIT;
        for row in &board {
            for &piece in row {
                if let Some(i) = POOL_ORDER.iter().position(|&u| u == piece) {
                    red[i] = red[i].saturating_sub(1);
                } else if let Some(i) = POOL_ORDER.iter().position(|&u| u.to_ascii_lowercase() == piece) {
                    black[i] = black[i].saturating_sub(1);
                }
            }
        }
        deduct_pool(&mut red, self.red_hidden_lost);
        deduct_pool(&mut black, self.black_hidden_lost);
        self.pool_red = POOL_ORDER.into_iter().zip(red).filter(|&(_, n)| n > 0).collect();
        self.pool_black = POOL_ORDER.into_iter().zip(black).filter(|&(_, n)| n > 0).collect();
    }

    // 池转 FEN 片段, 风格如 R2A2C2P5N2B2r2a2c2p5n2b2
    pub fn pool_fen(&self) -> String {
        let mut fen = String::new();
        for (piece, count) in self.pool_red.iter() {
            fen.push(*piece);
            fen.push_str(&count.to_string());
        }
        for (piece, count) in self.pool_black.iter() {
            fen.push(piece.to_ascii_lowercase());
            fen.push_str(&count.to_string());
        }
        fen
    }
}

// 从池中扣减 n 颗未知暗子: 优先扣剩余量最多的兵种(兵初始 5 颗概率最高, 同量按池顺序)
fn deduct_pool(pool: &mut [usize; 6], n: usize) {
    for _ in 0..n {
        let mut best = 0;
        for (i, &v) in pool.iter().enumerate() {
            if v > pool[best] {
                best = i;
            }
        }
        if pool[best] == 0 {
            break;
        }
        pool[best] -= 1;
    }
}

// 取棋盘坐标上的棋子(pos 取前两字符, 形如 "b2"), 空格返回 None
pub fn piece_at(board: [[char; 9]; 10], pos: &str) -> Option<char> {
    let mut cs = pos.chars();
    let x = (cs.next()? as usize).checked_sub(97)?;
    let y = 57 - cs.next()? as usize;
    if x >= 9 || y >= 10 {
        return None;
    }
    let piece = board[y][x];
    if piece == ' ' {
        None
    } else {
        Some(piece)
    }
}

// 棋盘转换FEN逻辑 (揭棋格式: board camp restPieces rule40 fullmove, 与 Pikafish jieqi StartFEN 一致)
// 无账目时退化为减法反推(初始 - 盘上明子), 供演练等无吃子跟踪的场景
pub fn board_fen(camp: &Camp, board: [[char; 9]; 10]) -> String {
    let mut account = PieceAccount::default();
    account.refresh_pool(board);
    board_fen_account(camp, board, &account)
}

// 带子力账目的 FEN: 被吃暗子已从未翻池扣除, 修正引擎对暗子储备的高估
// 注意: account 的池需先按当前盘面 refresh_pool, 否则池为空
pub fn board_fen_account(camp: &Camp, board: [[char; 9]; 10], account: &PieceAccount) -> String {
    let mut fen = String::new();
    for row in &board {
        let mut empty = 0;
        for &piece in row {
            if piece == ' ' {
                empty += 1;
            } else {
                if empty > 0 {
                    fen.push_str(&empty.to_string());
                    empty = 0;
                }
                fen.push(piece);
            }
        }
        if empty > 0 {
            fen.push_str(&empty.to_string());
        }
        fen.push('/');
    }
    fen.pop();
    fen.push(' ');
    fen.push(camp.to_char());
    fen.push(' ');
    fen.push_str(&account.pool_fen());
    // 步数无历史, 用引擎 StartFEN 默认值
    fen.push_str(" 0 1");
    fen
}

// 检测棋盘是否合法 (揭棋: 暗子可翻成任意子力, 位置不限; 仅帅/将仍限九宫)
pub fn board_check(board: [[char; 9]; 10]) -> bool {
    let mut bk = 0;
    let mut ba = 0;
    let mut bb = 0;
    let mut bc = 0;
    let mut bp = 0;
    let mut br = 0;
    let mut bn = 0;
    let mut rk = 0;
    let mut ra = 0;
    let mut rb = 0;
    let mut rc = 0;
    let mut rp = 0;
    let mut rr = 0;
    let mut rn = 0;

    for (y, row) in board.iter().enumerate() {
        for (x, &col) in row.iter().enumerate() {
            match col {
                'k' => {
                    bk += 1;
                    if y > 2 || !(3..=5).contains(&x) {
                        warn!("黑方'将'不在合法位置内");
                        return false;
                    }
                }
                'a' => {
                    ba += 1;
                }
                'b' => {
                    bb += 1;
                }
                'c' => {
                    bc += 1;
                }
                'p' => {
                    bp += 1;
                }
                'r' => {
                    br += 1;
                }
                'n' => {
                    bn += 1;
                }
                'K' => {
                    rk += 1;
                    if y < 7 || !(3..=5).contains(&x) {
                        warn!("红方'将'不在合法位置内, ({}行{}列)", y, x);
                        return false;
                    }
                }
                'A' => {
                    ra += 1;
                }
                'B' => {
                    rb += 1;
                }
                'C' => {
                    rc += 1;
                }
                'P' => {
                    rp += 1;
                }
                'R' => {
                    rr += 1;
                }
                'N' => {
                    rn += 1;
                }
                _ => {}
            }
        }
    }

    if bk != 1 || rk != 1 {
        warn!("黑方或红方'将'超出合法数量(红:{}, 黑:{})", rk, bk);
        return false;
    }
    if ba > 2 || ra > 2 {
        warn!("黑方或红方'士'超出合法数量(红:{}, 黑:{})", ra, ba);
        return false;
    }
    if bb > 2 || rb > 2 {
        warn!("黑方或红方'象'超出合法数量(红:{}, 黑:{})", rb, bb);
        return false;
    }
    if bc > 2 || rc > 2 {
        warn!("黑方或红方'炮'超出合法数量(红:{}, 黑:{})", rc, bc);
        return false;
    }
    if br > 2 || rr > 2 {
        warn!("黑方或红方'车'超出合法数量(红:{}, 黑:{})", rr, br);
        return false;
    }
    if bn > 2 || rn > 2 {
        warn!("黑方或红方'马'超出合法数量(红:{}, 黑:{})", rn, bn);
        return false;
    }
    if bp > 5 || rp > 5 {
        warn!("黑方或红方'兵'超出合法数量(红:{}, 黑:{})", rp, bp);
        return false;
    }
    true
}

pub const fn get_piece_name(piece: char) -> char {
    match piece {
        'K' => '帅',
        'k' => '将',
        'A' => '仕',
        'a' => '士',
        'B' => '相',
        'b' => '象',
        'N' => '马',
        'n' => '马',
        'R' => '车',
        'r' => '车',
        'C' => '炮',
        'c' => '炮',
        'P' => '兵',
        'p' => '卒',
        _ => ' ',
    }
}

pub fn startpos(board: [[char; 9]; 10]) -> bool { board == JIEQI_STARTPOS }

pub fn board_fix(camp: &Camp, board: &mut [[char; 9]; 10]) {
    if Camp::Black.eq(camp) {
        board.reverse();
        for i in board {
            i.reverse()
        }
    }
}

// board_to_map 棋盘数组转换为坐标模式
pub fn board_map(board: [[char; 9]; 10]) -> Vec<Position> {
    let mut position = vec![];

    for row in 0..10 {
        for col in 0..9 {
            position.push(Position { piece: board[row][col], pos: BOARD_MAP[row][col].to_string() });
        }
    }
    position
}

fn overlap_piece_y(board: [[char; 9]; 10], x: usize, y: usize, piece: char) -> Vec<usize> {
    let mut other_ys = Vec::new();
    for (i, value) in board.iter().enumerate() {
        if i != y && value[x] == piece {
            other_ys.push(i);
        }
    }
    other_ys
}

fn overlap_piece_xy(board: [[char; 9]; 10], from_x: usize, piece: char) -> Option<HashMap<usize, Vec<usize>>> {
    let mut other_xys = HashMap::new();
    for x in 0..9 {
        if x != from_x {
            let other_ys = overlap_piece_y(board, x, 10, piece); // 注意：这里将y设置为10是一个技巧，因为我们的棋盘只有10行，所以永远不会找到y==10的情况。但这样做并不是最好的方式。
            if other_ys.len() > 1 {
                other_xys.insert(x, other_ys);
                return Some(other_xys);
            }
        }
    }
    None
}

// 棋子坐标移动转中文模式 (揭棋: 暗子按起始位名义类型记谱, 翻/吃以后缀补充)
pub fn board_move_chinese(board: [[char; 9]; 10], iccs: &str) -> String {
    let mut chinese = String::new();
    let mv = Move::new(iccs);
    let raw_piece = board[mv.from_y][mv.from_x];
    let piece = if raw_piece == 'X' || raw_piece == 'x' {
        NOMINAL_STARTPOS[mv.from_y][mv.from_x]
    } else {
        raw_piece
    };
    let verticals = get_verticals(piece);

    // 解析翻子/吃暗子后缀: 第5字符按行棋方大小写区分, 第6字符为吃掉的暗子类型
    let chars: Vec<char> = iccs.chars().collect();
    let red_side = piece.is_ascii_uppercase();
    let (flip, cap) = match chars.len() {
        5 => {
            let l = chars[4];
            if red_side {
                if l.is_ascii_uppercase() { (Some(l), None) } else { (None, Some(l)) }
            } else if l.is_ascii_lowercase() {
                (Some(l), None)
            } else {
                (None, Some(l))
            }
        }
        len if len >= 6 => (Some(chars[4]), Some(chars[5])),
        _ => (None, None),
    };

    match piece {
        'K' => {
            chinese.push(get_piece_name(piece));
            chinese.push(verticals[mv.from_x]);

            match mv.from_y.cmp(&mv.to_y) {
                Ordering::Equal => {
                    // 平
                    chinese.push('平');
                    chinese.push(verticals[mv.to_x]);
                }
                Ordering::Less => {
                    // 退
                    chinese.push('退');
                    chinese.push(verticals[8]);
                }
                Ordering::Greater => {
                    // 进
                    chinese.push('进');
                    chinese.push(verticals[8]);
                }
            }
        }
        'k' => {
            chinese.push(get_piece_name(piece));
            chinese.push(verticals[mv.from_x]);

            match mv.from_y.cmp(&mv.to_y) {
                Ordering::Less => {
                    // 退
                    chinese.push('进');
                    chinese.push(verticals[0]);
                }
                Ordering::Greater => {
                    // 进
                    chinese.push('退');
                    chinese.push(verticals[0]);
                }
                Ordering::Equal => {
                    // 平
                    chinese.push('平');
                    chinese.push(verticals[mv.to_x]);
                }
            }
        }
        'A' | 'B' => {
            chinese.push(get_piece_name(piece));
            chinese.push(verticals[mv.from_x]);
            // 士、象只有进退
            match mv.from_y.cmp(&mv.to_y) {
                Ordering::Less => chinese.push('退'),
                _ => chinese.push('进'),
            }
            chinese.push(verticals[mv.to_x]);
        }
        'a' | 'b' => {
            chinese.push(get_piece_name(piece));
            chinese.push(verticals[mv.from_x]);
            // 士、象只有进退
            match mv.from_y.cmp(&mv.to_y) {
                Ordering::Less => chinese.push('进'),
                _ => chinese.push('退'),
            }
            chinese.push(verticals[mv.to_x]);
        }
        'R' | 'C' => {
            // 判断是否有纵向重叠情况
            let other_ys = overlap_piece_y(board, mv.from_x, mv.from_y, piece);
            if other_ys.is_empty() {
                // 非重叠情况
                chinese.push(get_piece_name(piece));
                chinese.push(verticals[mv.from_x]);
            } else {
                // 重叠, 判断前后
                if mv.from_y > other_ys[0] {
                    chinese.push('后')
                } else {
                    chinese.push('前')
                }
                chinese.push(get_piece_name(piece));
            }
            match mv.from_y.cmp(&mv.to_y) {
                Ordering::Less => {
                    // 退
                    let step = 9 - mv.to_y + mv.from_y;
                    chinese.push('退');
                    chinese.push(verticals[step]);
                }
                Ordering::Greater => {
                    // 进
                    let step = 9 - mv.from_y + mv.to_y;
                    chinese.push('进');
                    chinese.push(verticals[step]);
                }
                Ordering::Equal => {
                    // 平
                    chinese.push('平');
                    chinese.push(verticals[mv.to_x]);
                }
            }
        }
        'r' | 'c' => {
            // 判断是否有纵向重叠情况
            let other_ys = overlap_piece_y(board, mv.from_x, mv.from_y, piece);
            if other_ys.is_empty() {
                // 非重叠情况
                chinese.push(get_piece_name(piece));
                chinese.push(verticals[mv.from_x]);
            } else {
                // 重叠, 判断前后
                if mv.from_y > other_ys[0] {
                    chinese.push('前')
                } else {
                    chinese.push('后')
                }
                chinese.push(get_piece_name(piece));
            }
            match mv.from_y.cmp(&mv.to_y) {
                Ordering::Less => {
                    // 进
                    let step = mv.to_y - mv.from_y - 1;
                    chinese.push('进');
                    chinese.push(verticals[step]);
                }
                Ordering::Greater => {
                    // 退
                    let step = mv.from_y - mv.to_y - 1;
                    chinese.push('退');
                    chinese.push(verticals[step]);
                }
                Ordering::Equal => {
                    // 平
                    chinese.push('平');
                    chinese.push(verticals[mv.to_x]);
                }
            }
        }
        'N' => {
            // 判断是否有纵向重叠情况
            let other_ys = overlap_piece_y(board, mv.from_x, mv.from_y, piece);

            if other_ys.is_empty() {
                chinese.push(get_piece_name(piece));
                chinese.push(verticals[mv.from_x]);
            } else {
                if mv.from_y > other_ys[0] {
                    chinese.push('后');
                } else {
                    chinese.push('前');
                }
                chinese.push(get_piece_name(piece));
            }

            if mv.from_y < mv.to_y {
                chinese.push('退');
            } else {
                chinese.push('进');
            }
            chinese.push(verticals[mv.to_x]);
        }
        'n' => {
            // 判断是否有纵向重叠情况
            let other_ys = overlap_piece_y(board, mv.from_x, mv.from_y, piece);
            if other_ys.is_empty() {
                chinese.push(get_piece_name(piece));
                chinese.push(verticals[mv.from_x]);
            } else {
                if mv.from_y > other_ys[0] {
                    chinese.push('前');
                } else {
                    chinese.push('后');
                }
                chinese.push(get_piece_name(piece));
            }

            if mv.from_y < mv.to_y {
                chinese.push('进')
            } else {
                chinese.push('退')
            }
            chinese.push(verticals[mv.to_x]);
        }
        'P' => {
            // 判断是否有纵向重叠情况
            let mut other_ys = overlap_piece_y(board, mv.from_x, mv.from_y, piece);
            if other_ys.is_empty() {
                chinese.push(get_piece_name(piece));
                chinese.push(verticals[mv.from_x]);
            } else {
                // 判断其他纵线上是否有重叠
                if let Some(other_xys) = overlap_piece_xy(board, mv.from_x, piece) {
                    // 其他纵线有重叠
                    for y in &mut other_ys {
                        *y = (mv.from_x + 10) * 100 - *y;
                    }

                    for (x, ys) in &other_xys {
                        for y in ys {
                            other_ys.push((x + 10) * 100 - y);
                        }
                    }
                    // 降序
                    let value = (mv.from_x + 10) * 100 - mv.from_y;
                    other_ys.push(value);
                    other_ys.sort_by(|a, b| b.cmp(a));
                    let seq = other_ys.iter().position(|&v| v == value).map(|i| i + 1).unwrap();
                    chinese.push(verticals[9 - seq]);
                } else if other_ys.len() > 1 {
                    // 找出当前纵向重叠数量
                    let mut num = 1;
                    for y in other_ys {
                        if mv.from_y < y {
                            break;
                        }
                        num += 1;
                    }
                    chinese.push_str(num.to_string().as_str());
                } else {
                    // 只有前后
                    if mv.from_y > other_ys[0] {
                        chinese.push('后');
                    } else {
                        chinese.push('前');
                    }
                }
                chinese.push(get_piece_name(piece));
            }

            if mv.from_y == mv.to_y {
                // 平
                chinese.push('平');
                chinese.push(verticals[mv.to_x]);
            } else {
                // 进
                chinese.push('进');
                chinese.push(verticals[8]);
            }
        }
        'p' => {
            // 判断是否有纵向重叠情况
            let mut other_ys = overlap_piece_y(board, mv.from_x, mv.from_y, piece);
            if other_ys.is_empty() {
                chinese.push(get_piece_name(piece));
                chinese.push(verticals[mv.from_x]);
            } else {
                // 判断其他纵线上是否有重叠
                if let Some(other_xys) = overlap_piece_xy(board, mv.from_x, piece) {
                    // 其他纵线有重叠
                    for y in &mut other_ys {
                        *y += mv.from_x * 100;
                    }

                    for (x, ys) in &other_xys {
                        for y in ys {
                            other_ys.push(x * 100 + y);
                        }
                    }
                    // 升序
                    let value = mv.from_x * 100 + mv.from_y;
                    other_ys.push(value);
                    other_ys.sort_by(|a, b| b.cmp(a));
                    let seq = other_ys.iter().position(|&v| v == value).unwrap();
                    chinese.push(verticals[seq]);
                } else if other_ys.len() > 1 {
                    // 找出当前纵向重叠数量
                    let mut num = 0;
                    for y in other_ys {
                        if mv.from_y > y {
                            break;
                        }
                        num += 1;
                    }
                    chinese.push_str(num.to_string().as_str());
                } else {
                    // 只有前后
                    if mv.from_y > other_ys[0] {
                        chinese.push('前');
                    } else {
                        chinese.push('后');
                    }
                }
                chinese.push(get_piece_name(piece));
            }

            if mv.from_y == mv.to_y {
                // 平
                chinese.push('平');
                chinese.push(verticals[mv.to_x]);
            } else {
                // 进
                chinese.push('进');
                chinese.push(verticals[0]);
            }
        }
        _ => {}
    }

    // 附加翻子/吃暗子说明
    if let Some(f) = flip {
        chinese.push('翻');
        chinese.push(get_piece_name(f));
    }
    if let Some(c) = cap {
        chinese.push('吃');
        chinese.push(get_piece_name(c));
    }

    chinese
}

#[allow(dead_code)]
pub fn fen_to_board(mut fen: &str) -> [[char; 9]; 10] {
    if fen.contains(' ') {
        fen = fen.split_once(' ').unwrap().0
    }
    let mut board = [[' '; 9]; 10];
    let mut rank = 0;
    let mut file = 0;
    for c in fen.chars() {
        match c {
            '1'..='9' => {
                file += c.to_digit(10).unwrap() as usize;
            }
            '/' => {
                rank += 1;
                file = 0;
            }
            _ => {
                board[rank][file] = c;
                file += 1;
            }
        }
    }
    board
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_red_board_to_fen() {
        // 开局: 暗子池全量, 与引擎 StartFEN 完全一致
        let expected_fen = "xxxxkxxxx/9/1x5x1/x1x1x1x1x/9/9/X1X1X1X1X/1X5X1/9/XXXXKXXXX w R2A2C2P5N2B2r2a2c2p5n2b2 0 1";
        assert_eq!(board_fen(&Camp::Red, JIEQI_STARTPOS), expected_fen);

        // 双方各翻出一车后, 池中车辆计数递减
        let mut board = JIEQI_STARTPOS;
        board[9][0] = 'R';
        board[0][0] = 'r';
        let expected = "rxxxkxxxx/9/1x5x1/x1x1x1x1x/9/9/X1X1X1X1X/1X5X1/9/RXXXKXXXX w R1A2C2P5N2B2r1a2c2p5n2b2 0 1";
        assert_eq!(board_fen(&Camp::Red, board), expected);
    }

    #[test]
    fn test_fen_to_board() {
        let fen = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C2C4/9/RNBAKABNR";
        let board = [
            ['r', 'n', 'b', 'a', 'k', 'a', 'b', 'n', 'r'],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            [' ', 'c', ' ', ' ', ' ', ' ', ' ', 'c', ' '],
            ['p', ' ', 'p', ' ', 'p', ' ', 'p', ' ', 'p'],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            ['P', ' ', 'P', ' ', 'P', ' ', 'P', ' ', 'P'],
            [' ', 'C', ' ', ' ', 'C', ' ', ' ', ' ', ' '],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            ['R', 'N', 'B', 'A', 'K', 'A', 'B', 'N', 'R'],
        ];
        assert_eq!(fen_to_board(fen), board);
    }

    #[test]
    fn test_chinese() {
        // 开局翻子: 暗子按起始位名义车记谱, 翻开类型以后缀补充
        let mut board = JIEQI_STARTPOS;
        for pv in ["a0a1R", "a9a8r", "a1a5", "a8a4"] {
            let notice = board_move_chinese(board, pv);
            board = board_move(board, pv);
            println!("pv: {} => {}", pv, notice);
        }
        assert_eq!(board_move_chinese(board, "e0e1"), "帅五进一");
    }

    #[test]
    fn test_board_check() {
        // 明士出宫、明象过河均合法
        let mut board = JIEQI_STARTPOS;
        board[9][0] = 'A';
        board[7][1] = 'B';
        assert!(board_check(board));

        // 将出宫仍非法
        let mut board = JIEQI_STARTPOS;
        board[9][0] = 'K';
        board[9][4] = 'X';
        assert!(!board_check(board));

        // 士超数量仍非法
        let mut board = JIEQI_STARTPOS;
        board[9][0] = 'A';
        board[9][2] = 'A';
        board[9][8] = 'A';
        assert!(!board_check(board));

        // 暗子翻开的兵/卒可出现在任意暗子位 (回归: 黑卒在行2列4曾被误判非法导致识别卡死)
        let board = fen_to_board(
            "xx1xkx1xx/9/1x2p4/x7x/2r1b4/P1B3A2/4R4/9/9/XXXXKXXXX b R1A1C2P4N2B1r1a2c2p4n2b1 0 1",
        );
        assert!(board_check(board));

        let mut board = JIEQI_STARTPOS;
        board[7][8] = 'P';
        assert!(board_check(board));
    }

    #[test]
    fn test_board_diff() {
        // 暗子翻开: 单格变化判定为 Flip
        let mut new = JIEQI_STARTPOS;
        new[9][0] = 'R';
        let (changed, state) = board_diff(JIEQI_STARTPOS, new);
        assert!(matches!(state, BoardChangeState::Flip));
        assert_eq!(changed.piece, 'R');
        assert_eq!(changed.from, "a0");
        assert_eq!(changed.to, "a0");

        // 正常走子判定为 Move: 暗兵 a3 前进到 a4
        let mut new = JIEQI_STARTPOS;
        new[6][0] = ' ';
        new[5][0] = 'X';
        let (changed, state) = board_diff(JIEQI_STARTPOS, new);
        assert!(matches!(state, BoardChangeState::Move));
        assert_eq!(changed.piece, 'X');
        assert_eq!(changed.from, "a3");
        assert_eq!(changed.to, "a4");

        // 暗子消失(识别异常)判定为 One
        let mut new = JIEQI_STARTPOS;
        new[9][0] = ' ';
        let (_, state) = board_diff(JIEQI_STARTPOS, new);
        assert!(matches!(state, BoardChangeState::One));
    }

    #[test]
    fn test_board_diff_capture() {
        // 明吃明: 红车吃掉黑暗车, 被吃子类型明确
        let mut old = JIEQI_STARTPOS;
        old[9][0] = 'R';
        old[0][0] = 'r';
        let mut new = old;
        new[9][0] = ' ';
        new[0][0] = 'R';
        let (changed, state) = board_diff(old, new);
        assert!(matches!(state, BoardChangeState::Move));
        assert_eq!(changed.captured, Some('r'));

        // 明吃暗: 红车吃掉黑暗子, 类型未知记为 'x'
        let mut old = JIEQI_STARTPOS;
        old[9][0] = 'R';
        let mut new = old;
        new[9][0] = ' ';
        new[0][0] = 'R';
        let (changed, state) = board_diff(old, new);
        assert!(matches!(state, BoardChangeState::Move));
        assert_eq!(changed.captured, Some('x'));

        // 暗吃暗+翻: 红暗子吃掉黑暗子并翻成马
        let mut new = JIEQI_STARTPOS;
        new[6][0] = ' ';
        new[0][0] = 'N';
        let (changed, state) = board_diff(JIEQI_STARTPOS, new);
        assert!(matches!(state, BoardChangeState::Move));
        assert_eq!(changed.captured, Some('x'));
        assert_eq!(changed.piece, 'N');

        // 暗子吃明子+翻: 红暗子吃掉黑暗车并翻开
        let mut old = JIEQI_STARTPOS;
        old[0][0] = 'r';
        let mut new = old;
        new[6][0] = ' ';
        new[0][0] = 'C';
        let (changed, state) = board_diff(old, new);
        assert!(matches!(state, BoardChangeState::Move));
        assert_eq!(changed.captured, Some('r'));

        // 纯翻子与普通暗子移动不吃子
        let mut new = JIEQI_STARTPOS;
        new[9][0] = 'R';
        let (changed, state) = board_diff(JIEQI_STARTPOS, new);
        assert!(matches!(state, BoardChangeState::Flip));
        assert_eq!(changed.captured, None);

        let mut new = JIEQI_STARTPOS;
        new[6][0] = ' ';
        new[5][0] = 'X';
        let (changed, state) = board_diff(JIEQI_STARTPOS, new);
        assert!(matches!(state, BoardChangeState::Move));
        assert_eq!(changed.captured, None);
    }

    #[test]
    fn test_piece_account() {
        // 无账目: 池与开局一致
        let mut account = PieceAccount::default();
        account.refresh_pool(JIEQI_STARTPOS);
        assert_eq!(account.pool_fen(), "R2A2C2P5N2B2r2a2c2p5n2b2");

        // 吃掉一颗黑暗子: 类型未知, 黑池按剩余最多的兵扣减
        account.record('x');
        assert_eq!(account.captured_black, Vec::<char>::new());
        assert_eq!(account.black_hidden_lost, 1);
        account.refresh_pool(JIEQI_STARTPOS);
        assert_eq!(account.pool_fen(), "R2A2C2P5N2B2r2a2c2p4n2b2");

        // 吃掉红明车: 记入红方被吃明子, 池不变(车已翻明不在池中)
        account.record('R');
        assert_eq!(account.captured_red, vec!['R']);
        account.refresh_pool(JIEQI_STARTPOS);
        assert_eq!(account.pool_fen(), "R2A2C2P5N2B2r2a2c2p4n2b2");

        // 翻出红车后池中车辆递减, 黑池暗损保持
        let mut board = JIEQI_STARTPOS;
        board[9][0] = 'R';
        account.refresh_pool(board);
        assert_eq!(account.pool_fen(), "R1A2C2P5N2B2r2a2c2p4n2b2");

        // 红暗损 3 颗: 全部按兵扣减(5->2)
        let mut account = PieceAccount::default();
        account.record('X');
        account.record('X');
        account.record('X');
        account.refresh_pool(JIEQI_STARTPOS);
        assert_eq!(account.pool_fen(), "R2A2C2P2N2B2r2a2c2p5n2b2");
    }

    #[test]
    fn test_board_fen_account() {
        // 带账目的 FEN: 黑暗子被吃一颗后黑池兵数递减
        let mut account = PieceAccount::default();
        account.record('x');
        account.refresh_pool(JIEQI_STARTPOS);
        let expected = "xxxxkxxxx/9/1x5x1/x1x1x1x1x/9/9/X1X1X1X1X/1X5X1/9/XXXXKXXXX w R2A2C2P5N2B2r2a2c2p4n2b2 0 1";
        assert_eq!(board_fen_account(&Camp::Red, JIEQI_STARTPOS, &account), expected);
    }

    #[test]
    fn test_piece_at() {
        assert_eq!(piece_at(JIEQI_STARTPOS, "a3"), Some('X'));
        assert_eq!(piece_at(JIEQI_STARTPOS, "a4"), None);
        assert_eq!(piece_at(JIEQI_STARTPOS, "e9"), Some('k'));
        // 带翻子后缀的坐标只取前两字符
        assert_eq!(piece_at(JIEQI_STARTPOS, "a3R"), Some('X'));
        assert_eq!(piece_at(JIEQI_STARTPOS, "z9"), None);
    }

    #[test]
    fn test_board_map() {
        let mut board = JIEQI_STARTPOS;
        board[9][0] = 'R';
        let positions = board_map(board);
        assert_eq!(positions.len(), 90);
        assert_eq!(positions[0].pos, "a9");
        assert_eq!(positions[0].piece, 'x');
        assert_eq!(positions[81].pos, "a0");
        assert_eq!(positions[81].piece, 'R');
    }

    #[test]
    fn test_board_fix() {
        let mut board: [[char; 9]; 10] = [
            ['X', 'X', 'X', 'X', 'K', 'X', 'X', 'X', 'X'],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            [' ', 'X', ' ', ' ', ' ', ' ', ' ', 'X', ' '],
            ['X', ' ', 'X', ' ', 'X', ' ', 'X', ' ', 'X'],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            ['x', ' ', 'x', ' ', 'x', ' ', 'x', ' ', 'x'],
            [' ', 'x', ' ', ' ', ' ', ' ', ' ', 'x', ' '],
            [' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' '],
            ['x', 'x', 'x', 'x', 'k', 'x', 'x', 'x', 'x'],
        ];
        board_fix(&Camp::Black, &mut board);
        assert_eq!(board[0][4], 'k');
        assert_eq!(board[9][4], 'K');
        assert_eq!(board[0][0], 'x');
        assert_eq!(board[9][0], 'X');
    }
}
