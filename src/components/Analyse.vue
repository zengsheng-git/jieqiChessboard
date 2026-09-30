<script setup lang="ts">
import { listen } from '@tauri-apps/api/event';
import { NCard, NFlex, NTag, NText } from 'naive-ui';
import { computed, ref } from 'vue';

import { evalText, formatGap, groupByPiece, pieceName, winrateText as formatWinrate } from '../format';

// 子力账目: 双方被吃棋子与未翻暗子池
interface PieceAccount {
    captured_red: string[],   // 红方被吃的明子
    captured_black: string[], // 黑方被吃的明子
    red_hidden_lost: number,  // 红方未翻即被吃的暗子数(类型未知)
    black_hidden_lost: number,
    pool_red: [string, number][],   // 红方未翻暗子池(已扣除暗损)
    pool_black: [string, number][],
}

interface Analyse {
    depth: number,   // 深度
    score: number,   // 得分
    time: number,    // 时间
    pvs: string[],   // 思考(iccs)
    moves: string[], // 思考(chinese)
    alternatives: string[], // 次优候选首着(iccs)
    alt_scores: number[], // 次优候选与最优的分差, 负值=未知
    alt_moves: string[], // 次优候选(chinese)
    winrate: number | null, // 行棋方胜率(千分比, 仅引擎来源)
    deviation: { camp: string; loss: number } | null, // 上一步非预期走子的代价
    board: { piece: string; pos: string }[], // 分析时的局面, 供主线预演
    source: string,  // 来源
    camp: string,    // 行棋方阵营 'w'/'b'
    account: PieceAccount, // 子力账目: 双方被吃棋子与未翻暗子池
}


const followUps = ref<string[]>([])
const pvs = ref<string[]>([])
const altIccs = ref<string[]>([])
const deviation = ref<{ camp: string; loss: number } | null>(null)
const account = ref<PieceAccount | null>(null)
const winrate = ref<number | null>(null)
const lastScore = ref(0)
const analysisBoard = ref<{ piece: string; pos: string }[]>([])
const alternatives = ref<string[]>([])
const altScores = ref<number[]>([])
// 我方阵营，根据 mirror 事件推导（mirror=true 表示我方执黑）
const myCamp = ref<'w' | 'b'>('w')
// 最优招法的行棋方阵营('w'红/'b'黑), 后续招法按行棋方交替推演
const bestCamp = ref<"w" | "b">("w")
const best = ref({
    move: "----",
    side: "--",
    depth: 0,
    source: "--",
    evalText: "--",
    evalType: "info" as "error" | "success" | "info",
})

listen('mirror', async (event) => {
    myCamp.value = (event.payload as boolean) ? 'b' : 'w';
})

listen('analyse', async (event) => {
    let data = event.payload as Analyse;
    best.value.move = data.moves[0];
    best.value.depth = data.depth;
    best.value.source = data.source;
    lastScore.value = data.score;
    pvs.value = data.pvs ?? [];
    altIccs.value = data.alternatives ?? [];
    winrate.value = data.winrate ?? null;
    deviation.value = data.deviation ?? null;
    account.value = data.account ?? null;
    analysisBoard.value = data.board ?? [];
    // 新分析到达时旧主线已过期, 若正处预演态则强制还原, 避免悬停元素被重渲染后 mouseleave 丢失导致预演卡死
    if (previewActive) restoreBoard();
    alternatives.value = data.alt_moves ?? [];
    altScores.value = data.alt_scores ?? [];
    // 主线完整着法: 第 1 条是最优招法本身, 之后为双方接下来的推演着法
    followUps.value = data.moves;
    const isMine = data.camp === myCamp.value;
    bestCamp.value = data.camp === "b" ? "b" : "w";
    best.value.side = data.camp === "w" ? "红方" : "黑方";
    const evalResult = formatEval(data.score, isMine);
    best.value.evalText = evalResult.text;
    best.value.evalType = evalResult.type;

    // 设置选择框：我方用 b-select（红色），对手用 r-select（蓝色）
    const selectClass = isMine ? "b-select" : "r-select";
    let pv = data.pvs[0];
    let from = pv.substring(0, 2);
    let to = pv.substring(2, 4)
    document.getElementById(from)?.classList.add(selectClass);
    document.getElementById(to)?.classList.add(selectClass);

    // 次优候选着法：起点与落点都标记
    for (const alt of data.alternatives ?? []) {
        if (alt.length >= 4) {
            document.getElementById(alt.substring(0, 2))?.classList.add("alt-select");
            document.getElementById(alt.substring(2, 4))?.classList.add("alt-select");
        }
    }
})


// 分数解读：统一行棋方视角文字(见 format.ts)，颜色按我方利益着色：红=利好我方，绿=利好对方
function formatEval(score: number, isMine: boolean): { text: string, type: "error" | "success" | "info" } {
    const goodForMe = isMine === (score > 0);
    return { text: evalText(score), type: goodForMe ? "error" : "success" };
}

// 主线着法按行棋方交替: 偶数索引是最优招法的行棋方, 奇数索引是对手
function followUpSide(index: number): string {
    const moverRed = bestCamp.value === "w";
    const isMover = index % 2 === 0;
    return (isMover ? moverRed : !moverRed) ? "红方" : "黑方";
}

// 胜率展示: 引擎来源提供, 杀棋局面下无意义不展示
const winrateText = computed(() => formatWinrate(winrate.value, lastScore.value));

// 偏离提示代价: 颜色沿用面板约定, 红=利好我方, 绿=利好对方
const deviationText = computed(() => {
    if (!deviation.value) return null;
    const mine = deviation.value.camp === myCamp.value;
    const who = deviation.value.camp === "w" ? "红方" : "黑方";
    const loss = deviation.value.loss;
    if (Math.abs(loss) <= 30) {
        return { text: `${who}偏离提示, 与预期相当`, type: "info" as const };
    }
    const goodForMe = mine ? loss < 0 : loss > 0;
    const type = goodForMe ? ("error" as const) : ("success" as const);
    return loss > 0
        ? { text: `${who}偏离提示, 亏 ${loss} 分`, type }
        : { text: `${who}偏离提示, 反赚 ${-loss} 分`, type };
});

// 徽章数据: 兵种名 + 聚合计数
function toBadges(list: [string, number][]) {
    return list.map(([piece, count]) => ({ piece, count, name: pieceName(piece) }));
}

// 子力账目展示: 双方被吃明子/暗损/未翻池
const accountRows = computed(() => {
    const acc = account.value;
    if (!acc) return [];
    return [
        {
            camp: "red",
            label: "红方",
            hiddenLost: acc.red_hidden_lost,
            captured: toBadges(groupByPiece(acc.captured_red)),
            pool: toBadges(acc.pool_red),
        },
        {
            camp: "black",
            label: "黑方",
            hiddenLost: acc.black_hidden_lost,
            captured: toBadges(groupByPiece(acc.captured_black)),
            pool: toBadges(acc.pool_black),
        },
    ];
});

// ===== 主线预演: 通知棋盘组件从分析时局面重放着法, 移开按最新实时局面还原 =====

let previewActive = false;

// 悬停主线第 index 项: 预演到该步的局面(含最优招法)
function previewLine(index: number) {
    if (!pvs.value.length || !analysisBoard.value.length) return;
    previewActive = true;
    window.dispatchEvent(new CustomEvent("preview-line", {
        detail: { board: analysisBoard.value, pvs: pvs.value, count: Math.min(index + 1, pvs.value.length) },
    }));
}

function restoreBoard() {
    if (!previewActive) return;
    previewActive = false;
    window.dispatchEvent(new CustomEvent("preview-end"));
}

// 悬停次优招法: 预演走该备选首着后的局面
function previewAlt(index: number) {
    const alt = altIccs.value[index];
    if (!alt || alt.length < 4 || !analysisBoard.value.length) return;
    previewActive = true;
    window.dispatchEvent(new CustomEvent("preview-line", {
        detail: { board: analysisBoard.value, pvs: [alt], count: 1 },
    }));
}

</script>

<template>
    <n-card title="局面分析" :bordered="false" class="textlog" content-style="color: blue">
        <n-flex justify="space-between" align="end">
            <n-text type="info" class="analyse-title" strong @mouseenter="previewLine(0)" @mouseleave="restoreBoard()">
                {{ best.side }} {{ best.move }}
            </n-text>
            <n-flex align="center" :size="6">
                <n-tag size="small" round :bordered="false" type="info">{{ best.source }}</n-tag>
                <n-text type="warning">深度 {{ best.depth }}</n-text>
            </n-flex>
        </n-flex>
        <n-flex>
            <n-text :type="best.evalType">
                {{ best.evalText }}
            </n-text>
            <n-text v-if="winrateText" depth="3">{{ winrateText }}</n-text>
        </n-flex>
        <n-text v-if="deviationText" :type="deviationText.type" class="notes">{{ deviationText.text }}</n-text>
        <div v-if="accountRows.length" class="account" title="暗损 = 未翻开即被吃掉的暗子, 兵种未知">
            <div v-for="row in accountRows" :key="row.camp" class="account-side">
                <div class="side-title">
                    <span class="camp-dot" :class="row.camp"></span>
                    <span class="side-name">{{ row.label }}</span>
                    <span v-if="row.hiddenLost" class="hidden-lost">暗损 ×{{ row.hiddenLost }}</span>
                </div>
                <div class="kv">
                    <span class="k">被吃</span>
                    <span v-if="row.captured.length" class="badges">
                        <span v-for="b in row.captured" :key="b.piece" class="badge" :class="row.camp">{{ b.name }}<i v-if="b.count > 1">{{ b.count }}</i></span>
                    </span>
                    <span v-else class="none">无</span>
                </div>
                <div class="kv">
                    <span class="k">未翻</span>
                    <span v-if="row.pool.length" class="badges">
                        <span v-for="b in row.pool" :key="b.piece" class="badge dashed" :class="row.camp">{{ b.name }}<i v-if="b.count > 1">{{ b.count }}</i></span>
                    </span>
                    <span v-else class="none">无</span>
                </div>
            </div>
        </div>
        <div v-if="followUps.length" class="follow-ups">
            <n-text depth="3">后续：</n-text>
            <div
                v-for="(m, i) in followUps"
                :key="i"
                class="follow-up-item"
                @mouseenter="previewLine(i)"
                @mouseleave="restoreBoard()"
            >
                <n-text depth="3">{{ i + 1 }}. {{ followUpSide(i) }} {{ m }}</n-text>
            </div>
        </div>
        <div v-if="alternatives.length" class="alternatives">
            <n-text strong style="color: #9b59b6">次优招法</n-text>
                    <div
                        v-for="(m, i) in alternatives"
                        :key="i"
                        class="alt-item"
                        @mouseenter="previewAlt(i)"
                        @mouseleave="restoreBoard()"
                    >
                        <n-text depth="3">{{ i + 2 }}. {{ m }} {{ formatGap(altScores[i]) }}</n-text>
                    </div>
        </div>
    </n-card>
</template>

<style scoped>
.notes {
    margin-top: 6px;
}

.account {
    margin-top: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.account-side {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: 3px;
    border-bottom: 1px solid rgba(128, 128, 128, .15);
}

.account-side:last-child {
    border-bottom: none;
    padding-bottom: 0;
}

.side-title {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 600;
}

.camp-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
}

.camp-dot.red {
    background: #c94f42;
}

.camp-dot.black {
    background: #555;
}

.hidden-lost {
    margin-left: auto;
    font-size: 11px;
    font-weight: 400;
    color: rgba(128, 128, 128, .9);
}

.kv {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    min-width: 0;
}

.kv .k {
    flex: none;
    color: rgba(128, 128, 128, .9);
}

.badges {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
}

.badge {
    display: inline-flex;
    align-items: baseline;
    justify-content: center;
    min-width: 17px;
    height: 18px;
    padding: 0 3px;
    border-radius: 4px;
    font-size: 11px;
    line-height: 18px;
    box-sizing: border-box;
}

.badge i {
    font-style: normal;
    font-size: 9px;
    margin-left: 1px;
    opacity: .75;
}

.badge.red {
    color: #c0392b;
    background: rgba(192, 57, 43, .12);
}

.badge.black {
    color: #444;
    background: rgba(85, 85, 85, .14);
}

.badge.dashed {
    background: transparent;
    border: 1px dashed rgba(128, 128, 128, .55);
    line-height: 16px;
}

.none {
    color: rgba(128, 128, 128, .6);
}

.follow-ups {
    margin-top: 6px;
    font-size: 12px;
}

.follow-up-item {
    line-height: 1.7;
    border-radius: 2px;
    padding: 0 2px;
}

.follow-up-item:hover {
    background-color: rgba(24, 160, 88, 0.15);
}

.analyse-title {
    font-size: x-large;
    cursor: pointer;
    border-radius: 2px;
    padding: 0 2px;
}

.analyse-title:hover {
    background-color: rgba(24, 160, 88, 0.15);
}

.alternatives {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
}

.alt-item {
    line-height: 1.5;
    border-radius: 2px;
    padding: 0 2px;
    cursor: pointer;
}

.alt-item:hover {
    background-color: rgba(24, 160, 88, 0.15);
}

.textlog {
    width: 260px;
    height: 470px;
    left: 400px;
    top: 0px;
}

/* 内容区内部滚动: 账目/后续/次优总高可变, 不允许溢出卡片把窗口撑出滚动条 */
.textlog :deep(.n-card__content) {
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: thin;
}

.textlog :deep(.n-card__content)::-webkit-scrollbar {
    width: 4px;
}

.textlog :deep(.n-card__content)::-webkit-scrollbar-thumb {
    background: rgba(128, 128, 128, .35);
    border-radius: 2px;
}
</style>