import { reactive } from "vue";

// 连线分析的实时状态, 供局势推演等模块读取
export const mirrorStore = reactive({
    board: {} as Record<string, string>, // 当前局面(坐标 -> 棋子, ' ' 为空)
    camp: "w", // 当前行棋方 'w'/'b'
    mirror: false, // 是否镜像视角(我方执黑)
});
