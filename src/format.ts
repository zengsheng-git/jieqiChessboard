// 评分文字解读: 行棋方视角, 正=行棋方占优
export function evalText(score: number): string {
    const abs = Math.abs(score);
    if (score >= 29000) return `${30000 - score}步杀`;
    if (score <= -29000) return `${30000 + score}步被杀`;
    if (abs < 30) return "均势";
    const gradePos = ["略优", "较优", "大优", "胜势"];
    const gradeNeg = ["略差", "较差", "大差", "败势"];
    const idx = abs < 150 ? 0 : abs < 400 ? 1 : abs < 800 ? 2 : 3;
    return score > 0 ? `+${abs} ${gradePos[idx]}` : `-${abs} ${gradeNeg[idx]}`;
}

// 胜率展示文字(千分比), 杀棋局面不显示
export function winrateText(winrate: number | null | undefined, score: number): string {
    if (winrate === null || winrate === undefined) return "";
    if (Math.abs(score) >= 29000) return "";
    return `胜率 ${Math.round(winrate / 10)}%`;
}

// 次优招法分差展示: 0 表示与最优同级; 负值=未知, 不展示
export function formatGap(gap: number | undefined): string {
    if (gap === undefined || gap < 0) return "";
    if (gap === 0) return "±0";
    return `-${gap}`;
}

// 棋子中文名(区分阵营: 红帅仕相兵, 黑将士象卒)
const PIECE_NAMES: Record<string, string> = {
    K: "帅", A: "仕", B: "相", N: "马", R: "车", C: "炮", P: "兵",
    k: "将", a: "士", b: "象", n: "马", r: "车", c: "炮", p: "卒",
};

export function pieceName(piece: string): string {
    return PIECE_NAMES[piece] ?? piece;
}

// 按棋子聚合计数: ["R","R","N"] => [["R",2],["N",1]]
export function groupByPiece(pieces: string[] | undefined): [string, number][] {
    if (!pieces?.length) return [];
    const counts = new Map<string, number>();
    for (const p of pieces) counts.set(p, (counts.get(p) ?? 0) + 1);
    return [...counts];
}
