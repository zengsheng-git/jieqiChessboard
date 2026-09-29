<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { computed, ref } from "vue";
import { NButton, NCard, NFlex, NTag, NText } from "naive-ui";

import { mirrorStore } from "../store";
import { evalText, formatGap, winrateText } from "../format";
import "../assets/css/chessboard.css";

interface Position {
    piece: string;
    pos: string;
}

interface PracticeState {
    board: Position[];
    camp: string;
    moves: string[];
    history: string[];
}

interface MoveOutcome {
    notation: string;
    state: PracticeState;
}

const ALL_IDS: string[] = [];
for (let y = 9; y >= 0; y--) {
    for (let x = 0; x < 9; x++) {
        ALL_IDS.push(`${"abcdefghi"[x]}${y}`);
    }
}

const started = ref(false);
const tip = ref("");
const boardMap = ref<Record<string, string>>({});
const camp = ref("w");
const legalMoves = ref<string[]>([]);
const selected = ref("");
const moveTargets = ref<string[]>([]);
const thinking = ref(false);const hint = ref({
    best: "----",
    score: 0,
    depth: 0,
    winrate: null as number | null,
    source: "",
    valid: false,
    camp: "w",
});
const lastLoss = ref<number | null>(null);
const pvs = ref<string[]>([]);
const followUps = ref<string[]>([]);
const alternatives = ref<string[]>([]);
const altScores = ref<number[]>([]);
const altIccs = ref<string[]>([]);
// 上一次分析的评分与行棋方, 用于计算用户走子的亏损; pendingMoves 为自上次完成分析以来的走子数
let prevScore: number | null = null;
let prevCamp = "";
let pendingMoves = 0;
let analyzeSeq = 0;

// 与镜像盘一致的视角翻转
const wrappedItems = computed(() => {
    const items: { id: string }[] = [];
    if (mirrorStore.mirror) {
        for (let y = 0; y <= 9; y++) {
            for (let x = 8; x >= 0; x--) {
                items.push({ id: `${"abcdefghi"[x]}${y}` });
            }
        }
    } else {
        for (let y = 9; y >= 0; y--) {
            for (let x = 0; x <= 8; x++) {
                items.push({ id: `${"abcdefghi"[x]}${y}` });
            }
        }
    }
    return items;
});

function cellPiece(id: string): Element | undefined | null {
    return document.getElementById("practiceboard")?.querySelector(`[id="${id}"]`)?.firstElementChild;
}

function cellWrap(id: string): Element | undefined | null {
    return document.getElementById("practiceboard")?.querySelector(`[id="${id}"]`);
}

// 清除推荐/次优招法的坐标高亮
function clearHintHighlight() {
    document.getElementById("practiceboard")
        ?.querySelectorAll(".b-select, .r-select, .alt-select")
        .forEach((el) => el.classList.remove("b-select", "r-select", "alt-select"));
}

// 在演练盘上标出最优(红框)与次优(紫框)招法的起止坐标
function highlightHint() {
    clearHintHighlight();
    const pv = pvs.value[0];
    if (pv && pv.length >= 4) {
        cellWrap(pv.substring(0, 2))?.classList.add("b-select");
        cellWrap(pv.substring(2, 4))?.classList.add("b-select");
    }
    for (const alt of altIccs.value) {
        if (alt.length >= 4) {
            cellWrap(alt.substring(0, 2))?.classList.add("alt-select");
            cellWrap(alt.substring(2, 4))?.classList.add("alt-select");
        }
    }
}

function renderBoard(map: Record<string, string>) {
    for (const [pos, piece] of Object.entries(map)) {
        const ele = cellPiece(pos);
        if (!ele) continue;
        ele.classList.forEach(cls => {
            if (cls !== "piece") {
                ele.classList.remove(cls);
            }
        });
        if (piece !== " ") {
            ele.classList.add(`piece-${piece}`);
        }
    }
}

function applyState(state: PracticeState) {
    const map: Record<string, string> = {};
    for (const id of ALL_IDS) map[id] = " ";
    for (const record of state.board) map[record.pos] = record.piece;
    boardMap.value = map;
    camp.value = state.camp;
    legalMoves.value = state.moves;
    selected.value = "";
    moveTargets.value = [];
    clearHintHighlight();
    renderBoard(map);
}

function isOwnPiece(piece: string) {
    return camp.value === "w" ? piece >= "A" && piece <= "Z" : piece >= "a" && piece <= "z";
}

async function startPractice() {
    const positions: Position[] = [];
    for (const [pos, piece] of Object.entries(mirrorStore.board)) {
        if (piece && piece !== " ") positions.push({ piece, pos });
    }
    if (!positions.length) {
        tip.value = "请先在连线分析中启动监听, 识别到棋盘后再来演练";
        return;
    }
    try {
        const state = await invoke<PracticeState>("practice_start", { board: positions, camp: mirrorStore.camp });
        lastLoss.value = null;
        prevScore = null;
        prevCamp = "";
        pendingMoves = 0;
        tip.value = "";
        applyState(state);
        started.value = true;
        await refreshHint();
    } catch (e) {
        tip.value = String(e);
    }
}

async function onCellClick(id: string) {
    if (!started.value) return;
    if (selected.value) {
        const mv = legalMoves.value.find((m) => m === `${selected.value}${id}`);
        if (mv) {
            await doMove(selected.value, id);
            return;
        }
    }
    const piece = boardMap.value[id];
    if (piece && piece !== " " && isOwnPiece(piece)) {
        selected.value = id;
        moveTargets.value = legalMoves.value.filter((m) => m.startsWith(id)).map((m) => m.substring(2, 4));
    } else {
        selected.value = "";
        moveTargets.value = [];
    }
}

async function doMove(from: string, to: string) {
    try {
        // 思考中走子: 通知后端中断当前搜索, 引擎立即让位给新局面的分析
        const interrupt = thinking.value;
        const outcome = await invoke<MoveOutcome>("practice_do_move", { from, to, interrupt });
        applyState(outcome.state);
        pendingMoves += 1;
        await refreshHint();
    } catch (e) {
        tip.value = String(e);
    }
}

async function undoMove() {
    try {
        const state = await invoke<PracticeState>("practice_undo");
        applyState(state);
        lastLoss.value = null;
        pendingMoves = 0;
        prevScore = null;
        prevCamp = "";
        await refreshHint();
    } catch (e) {
        tip.value = String(e);
    }
}

async function resetPractice() {
    try {
        const state = await invoke<PracticeState>("practice_reset");
        applyState(state);
        lastLoss.value = null;
        pendingMoves = 0;
        prevScore = null;
        prevCamp = "";
        await refreshHint();
    } catch (e) {
        tip.value = String(e);
    }
}

// 对当前演练局面做云库优先的分析; 走子过快时旧分析的结果按序号丢弃
async function refreshHint() {
    const seq = ++analyzeSeq;
    thinking.value = true;
    hint.value = {
        best: "----",
        score: 0,
        depth: 0,
        winrate: null,
        source: "",
        valid: false,
        camp: camp.value,
    };
    try {
        const result = await invoke<any>("practice_analyze");
        if (seq !== analyzeSeq) return;
        const moves: string[] = result.moves ?? [];
        hint.value = {
            best: moves.length ? moves[0] : "无",
            score: result.score ?? 0,
            depth: result.depth ?? 0,
            winrate: result.winrate ?? null,
            source: result.source ?? "",
            valid: moves.length > 0,
            camp: result.camp ?? camp.value,
        };
        pvs.value = result.pvs ?? [];
        // 主线完整着法: 第 1 条是最优招法本身, 之后为双方接下来的推演着法
        followUps.value = moves;
        alternatives.value = result.alt_moves ?? [];
        altScores.value = result.alt_scores ?? [];
        altIccs.value = result.alternatives ?? [];
        highlightHint();
        // 亏损 = 走子前评分 + 走子后评分(均为行棋方视角), 仅在恰好走了一步时计算
        if (pendingMoves === 1 && prevScore !== null && prevCamp !== "" && prevCamp !== camp.value) {
            lastLoss.value = prevScore + result.score;
        } else {
            lastLoss.value = null;
        }
        pendingMoves = 0;
        prevScore = result.score ?? 0;
        prevCamp = camp.value;
    } catch (e) {
        if (seq !== analyzeSeq) return;
        hint.value = {
            best: "分析失败",
            score: 0,
            depth: 0,
            winrate: null,
            source: "",
            valid: false,
            camp: camp.value,
        };
        altIccs.value = [];
        clearHintHighlight();
        lastLoss.value = null;
        pendingMoves = 0;
    } finally {
        if (seq === analyzeSeq) {
            thinking.value = false;
        }
    }
}

const hintEvalText = computed(() => {
    if (thinking.value || !hint.value.valid) return "";
    const parts = [evalText(hint.value.score)];
    const wr = winrateText(hint.value.winrate, hint.value.score);
    if (wr) parts.push(wr);
    return parts.join(" · ");
});

const hintEvalType = computed(() => {
    if (thinking.value) return "warning" as const;
    if (!hint.value.valid) return "error" as const;
    if (hint.value.score > 30) return "success" as const;
    if (hint.value.score < -30) return "error" as const;
    return "info" as const;
});

const lossText = computed(() => {
    if (lastLoss.value === null) return "";
    if (lastLoss.value > 30) return `这步亏了 ${lastLoss.value} 分`;
    if (lastLoss.value < -30) return `这步赚了 ${-lastLoss.value} 分`;
    return "这步与引擎预期相当";
});
const lossType = computed(() => ((lastLoss.value ?? 0) > 30 ? "error" : "success") as "error" | "success");

// 主线着法按行棋方交替: 偶数索引是最优招法的行棋方, 奇数索引是对手
function followUpSide(index: number): string {
    const moverRed = hint.value.camp === "w";
    const isMover = index % 2 === 0;
    return (isMover ? moverRed : !moverRed) ? "红方" : "黑方";
}

// 悬停主线第 index 项: 预演到该步的局面(含最优招法); 演练盘无实时事件, 直接按数据态渲染/还原
function previewLine(index: number) {
    if (!pvs.value.length) return;
    const map = { ...boardMap.value };
    const count = Math.min(index + 1, pvs.value.length);
    for (let i = 0; i < count; i++) {
        const from = pvs.value[i].substring(0, 2);
        const to = pvs.value[i].substring(2, 4);
        const piece = map[from];
        if (!piece || piece === " ") break;
        map[from] = " ";
        map[to] = piece;
    }
    renderBoard(map);
}

function restoreBoard() {
    renderBoard(boardMap.value);
}

// 悬停次优招法: 预演走该备选首着后的局面
function previewAlt(index: number) {
    const alt = altIccs.value[index];
    if (!alt || alt.length < 4) return;
    const map = { ...boardMap.value };
    const from = alt.substring(0, 2);
    const to = alt.substring(2, 4);
    const piece = map[from];
    if (!piece || piece === " ") return;
    map[from] = " ";
    map[to] = piece;
    renderBoard(map);
}
</script>

<template>
    <div>
        <div id="practiceboard">
            <div
                v-for="(item, _) in wrappedItems"
                :key="item.id"
                :id="item.id"
                class="piece-wrap practice-cell"
                :class="{ 'sel-select': item.id === selected, 'mv-select': moveTargets.includes(item.id) }"
                @click="onCellClick(item.id)"
            >
                <span class="piece"></span>
            </div>
        </div>

        <n-card title="局势推演" :bordered="false" class="practice-panel">
            <div v-if="!started" class="practice-tip">
                <n-text depth="3">从连线分析的当前局面开始, 自由替双方试招, 每步给出提示与代价。</n-text>
                <n-button type="primary" size="small" @click="startPractice">从当前局面开始</n-button>
                <n-text v-if="tip" type="error">{{ tip }}</n-text>
            </div>
            <template v-else>
                <n-flex justify="space-between" align="end">
                    <n-text type="info" class="analyse-title" strong @mouseenter="previewLine(0)" @mouseleave="restoreBoard()">
                        {{ camp === "w" ? "红方" : "黑方" }} {{ hint.best }}
                    </n-text>
                    <n-flex :size="6" align="center">
                        <n-tag size="small" round :bordered="false" type="info">{{ hint.source }}</n-tag>
                        <n-text type="warning">深度 {{ hint.depth }}</n-text>
                    </n-flex>
                </n-flex>
                <n-flex>
                    <n-text :type="hintEvalType">{{ thinking ? "思考中…" : hintEvalText }}</n-text>
                </n-flex>
                <n-text v-if="lossText" :type="lossType" class="loss">{{ lossText }}</n-text>
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
                <n-flex :size="4" class="practice-actions">
                    <n-button size="tiny" type="primary" @click="startPractice">重新推演</n-button>
                    <n-button size="tiny" @click="undoMove">悔棋</n-button>
                    <n-button size="tiny" @click="resetPractice">重置</n-button>
                </n-flex>
            </template>
        </n-card>
    </div>
</template>

<style scoped>
.practice-panel {
    width: 260px;
    height: 440px;
    left: 1060px;
    top: 0px;
    position: absolute;
}

.practice-tip {
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: flex-start;
}

.hint-row {
    margin-top: 6px;
}

.loss {
    margin-top: 4px;
}

.follow-ups {
    margin-top: 6px;
    max-height: 180px;
    overflow-y: auto;
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

.analyse-title {
    font-size: x-large;
    cursor: pointer;
    border-radius: 2px;
    padding: 0 2px;
}

.analyse-title:hover {
    background-color: rgba(24, 160, 88, 0.15);
}

.practice-actions {
    margin-top: 8px;
}


.practice-cell {
    cursor: pointer;
}
</style>
