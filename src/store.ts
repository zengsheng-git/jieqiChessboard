import { reactive } from "vue";

import type { ArrowHint } from "./arrows";

// 连线分析的实时状态, 供局势推演等模块读取
export const mirrorStore = reactive({
    board: {} as Record<string, string>, // 当前局面(坐标 -> 棋子, ' ' 为空)
    camp: "w", // 当前行棋方 'w'/'b'
    mirror: false, // 是否镜像视角(我方执黑)
    arrows: [] as ArrowHint[], // 招法提示箭头(优选/次优), 分析面板写入, 镜像盘渲染
});
