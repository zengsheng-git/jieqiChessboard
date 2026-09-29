use tracing::trace;

use crate::chess;
use crate::yolo;

// detections_bound 获取截图的边界
pub fn detections_bound(
    origin_width: u32, origin_height: u32, detections: &[yolo::Detection],
) -> Result<(u32, u32, u32, u32), String> {
    // 棋盘框优先; 无边框界面(如棋谱复盘)用棋子中心点范围推算
    let (bd_x0, bd_y0, bd_x1, bd_y1) = match detections.iter().find(|d| d.label == '0') {
        Some(d) => (d.x0, d.y0, d.x1, d.y1),
        None => piece_center_bound(detections)?,
    };

    // 计算模型图到原图的缩放
    let scale_x = origin_width as f32 / yolo::IMAGE_WIDTH as f32;
    let scale_y = origin_height as f32 / yolo::IMAGE_HEIGHT as f32;

    // 模型坐标 → 原图坐标
    let bx0 = (bd_x0 * scale_x).max(0.0);
    let by0 = (bd_y0 * scale_y).max(0.0);
    let bx1 = (bd_x1 * scale_x).min(origin_width as f32);
    let by1 = (bd_y1 * scale_y).min(origin_height as f32);

    // 计算原图下的“半格”尺寸
    let board_w = bx1 - bx0;
    let board_h = by1 - by0;
    let half_cell_x = board_w / 9.0 / 2.0;
    let half_cell_y = board_h / 10.0 / 2.0;

    // 计算裁剪框左上
    let crop_x = (bx0 - half_cell_x).max(0.0) as u32;
    let crop_y = (by0 - half_cell_y).max(0.0) as u32;

    // 计算裁剪框右下，在原图范围内
    let x1p = (bx1 + half_cell_x).min(origin_width as f32);
    let y1p = (by1 + half_cell_y).min(origin_height as f32);

    // 宽高 = 右下 - 左上
    let width = (x1p - crop_x as f32) as u32;
    let height = (y1p - crop_y as f32) as u32;

    Ok((crop_x, crop_y, width, height))
}

// piece_center_bound 用棋子中心点范围推算棋盘框(模型坐标)
// 棋子中心落在交叉点上, 四角有子时范围即最外圈交叉线, 与棋盘框几何一致
fn piece_center_bound(detections: &[yolo::Detection]) -> Result<(f32, f32, f32, f32), String> {
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for det in detections.iter().filter(|d| d.label.is_ascii_alphabetic()) {
        let cx = (det.x0 + det.x1) / 2.0;
        let cy = (det.y0 + det.y1) / 2.0;
        bounds = Some(match bounds {
            Some((x0, y0, x1, y1)) => (x0.min(cx), y0.min(cy), x1.max(cx), y1.max(cy)),
            None => (cx, cy, cx, cy),
        });
    }
    let Some((x0, y0, x1, y1)) = bounds else {
        return Err("未识别到棋盘".to_string());
    };
    // 排除同行或同列棋子推出的退化范围
    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
        return Err("未识别到棋盘".to_string());
    }
    Ok((x0, y0, x1, y1))
}

const MODEL_CELL_W: f32 = yolo::IMAGE_WIDTH as f32 / 9.0;
const MODEL_CELL_H: f32 = yolo::IMAGE_HEIGHT as f32 / 10.0;

// detections_to_board 识别结果转换为棋盘结构
pub fn detections_to_board(detections: &[yolo::Detection]) -> Result<(chess::Camp, [[char; 9]; 10]), String> {
    let mut camp = chess::Camp::None;
    let mut board = [[' '; 9]; 10];

    // 只取棋子类标签: '0' 为棋盘框, ' ' 为边框数字标记等非棋子检测
    // 监听截图已按棋盘对齐裁剪, 棋盘框缺失(无边框界面)时固定网格映射同样成立
    let mut found = false;
    for det in detections.iter().filter(|d| d.label.is_ascii_alphabetic()) {
        found = true;
        // 中心点
        let cx = (det.x0 + det.x1) / 2.0;
        let cy = (det.y0 + det.y1) / 2.0;
        // 行列：x 轴分成 9 格，y 轴分成 10 格
        let col = (cx / MODEL_CELL_W).floor() as usize; // 0–8
        let row = (cy / MODEL_CELL_H).floor() as usize; // 0–9
        trace!("{} row={} col={}", det.label, row, col);

        // 边界处理
        if !(0..=8).contains(&col) || !(0..=9).contains(&row) {
            continue;
        }

        // 构建board: 未辨方暗子按半场归类(下半=红X/上半=黑x, 与前端规则一致)
        let label = if det.label == 'D' {
            if row >= 5 { 'X' } else { 'x' }
        } else {
            det.label
        };
        board[row][col] = label;

        // 判断阵营
        if camp == chess::Camp::None && (3..=5).contains(&col) && row >= 7 {
            match label {
                'k' => camp = chess::Camp::Black,
                'K' => camp = chess::Camp::Red,
                _ => {}
            }
        }
    }
    if !found {
        return Err("not board".to_string());
    }
    Ok((camp, board))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_fallback_from_piece_centers() {
        // 无棋盘框: 四角棋子中心位于模型坐标列线100..500、行线100..496
        let dets = vec![
            yolo::Detection::of(' ', 20.0, 20.0, 30.0, 30.0), // 边框数字应被忽略
            yolo::Detection::of('K', 80.0, 80.0, 120.0, 120.0),
            yolo::Detection::of('a', 480.0, 80.0, 520.0, 120.0),
            yolo::Detection::of('a', 80.0, 476.0, 120.0, 516.0),
            yolo::Detection::of('k', 480.0, 476.0, 520.0, 516.0),
        ];
        // 原图1280x1280(缩放2): 框200,200..1000,992, 外扩半格后裁剪 155,160 889x871
        assert_eq!(detections_bound(1280, 1280, &dets).unwrap(), (155, 160, 889, 871));
    }

    #[test]
    fn bound_fallback_degenerate_fails() {
        let dets = vec![
            yolo::Detection::of(' ', 20.0, 20.0, 30.0, 30.0),
            yolo::Detection::of('a', 100.0, 100.0, 120.0, 120.0),
            yolo::Detection::of('a', 100.0, 500.0, 120.0, 520.0), // 同列棋子, y 范围退化
        ];
        assert!(detections_bound(1280, 1280, &dets).is_err());
    }

    #[test]
    fn to_board_maps_without_board_frame() {
        let dets = vec![
            // 中心(125,122)→row1 col1; 中心(320,544)→row8 col4(帅位, 判黑方行棋)
            yolo::Detection::of('K', 105.0, 102.0, 145.0, 142.0),
            yolo::Detection::of('k', 300.0, 524.0, 340.0, 564.0),
        ];
        let (camp, board) = detections_to_board(&dets).unwrap();
        assert_eq!(camp, chess::Camp::Black);
        assert_eq!(board[1][1], 'K');
        assert_eq!(board[8][4], 'k');
        assert_eq!(board[0][0], ' ');
    }
}
