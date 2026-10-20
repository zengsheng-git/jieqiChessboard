// 招法提示箭头几何计算: 将着法起止坐标换算为棋盘上的箭头线段与箭头三角形
// 镜像盘(#chessboard)与演练盘(#practiceboard)共用同一几何: 360x400 内容区 + 10px 内边距, 40px 格子

export type ArrowKind = "red" | "blue" | "alt";

export interface ArrowHint {
    from: string; // 起点坐标, 如 "e4"
    to: string; // 落点坐标
    kind: ArrowKind;
}

export interface ArrowShape {
    x1: number;
    y1: number;
    x2: number;
    y2: number;
    head: string; // 箭头三角形顶点, "x1,y1 x2,y2 x3,y3"
    color: string;
    width: number;
}

const CELL = 40;
const PAD = 10;

const STYLES: Record<ArrowKind, { color: string; width: number; headLen: number; headHalf: number }> = {
    red: { color: "#e74c3c", width: 5, headLen: 14, headHalf: 8 },
    blue: { color: "#3498db", width: 5, headLen: 14, headHalf: 8 },
    alt: { color: "#9b59b6", width: 3.5, headLen: 11, headHalf: 6.5 },
};

// 坐标 -> 格子中心(相对棋盘元素左上角); 镜像视角下行列均取反, 与两块棋盘的格子排列一致
function center(id: string, mirror: boolean): { x: number; y: number } | null {
    const x = "abcdefghi".indexOf(id[0]);
    const y = id.length > 1 ? Number(id[1]) : NaN;
    if (x < 0 || Number.isNaN(y) || y < 0 || y > 9) return null;
    const row = mirror ? y : 9 - y;
    const col = mirror ? 8 - x : x;
    return { x: PAD + col * CELL + CELL / 2, y: PAD + row * CELL + CELL / 2 };
}

export function computeArrows(hints: ArrowHint[], mirror: boolean): ArrowShape[] {
    const shapes: ArrowShape[] = [];
    for (const hint of hints) {
        const from = center(hint.from, mirror);
        const to = center(hint.to, mirror);
        if (!from || !to) continue;
        const dx = to.x - from.x;
        const dy = to.y - from.y;
        const len = Math.hypot(dx, dy);
        if (len < CELL / 2) continue; // 起落点重合或过近, 不绘制
        const ux = dx / len;
        const uy = dy / len;
        const style = STYLES[hint.kind];
        // 箭尾从起点棋子边缘让开, 箭尖直达落点中心(盖在落点棋子上, 吃子与去向一目了然)
        const tail = { x: from.x + ux * 12, y: from.y + uy * 12 };
        const tip = to;
        const headBase = { x: tip.x - ux * style.headLen, y: tip.y - uy * style.headLen };
        const shaftEnd = { x: tip.x - ux * (style.headLen - 2), y: tip.y - uy * (style.headLen - 2) };
        const px = -uy;
        const py = ux;
        const head = [
            `${tip.x},${tip.y}`,
            `${headBase.x + px * style.headHalf},${headBase.y + py * style.headHalf}`,
            `${headBase.x - px * style.headHalf},${headBase.y - py * style.headHalf}`,
        ].join(" ");
        shapes.push({
            x1: tail.x, y1: tail.y, x2: shaftEnd.x, y2: shaftEnd.y,
            head, color: style.color, width: style.width,
        });
    }
    return shapes;
}
