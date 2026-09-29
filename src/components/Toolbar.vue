<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, ref, h, computed } from "vue";
import { useDialog } from "naive-ui";
import {
    NButton,
    NCard,
    NFlex,
    NForm,
    NFormItem,
    NInputNumber,
    NSelect,
    NDrawer,
    NDrawerContent,
    NSpace,
    NTooltip,
    NDivider,
    NEmpty,
    NScrollbar,
    NTag,
    NInput,
    NSwitch,
} from "naive-ui";

const options = [
    {
        label: "连线分析",
        value: "LinkAnaly",
        disabled: false,
    },
    {
        label: "局势推演",
        value: "Practice",
        disabled: false,
    },
    {
        label: "连线对战",
        value: "LinkPlay",
        disabled: true,
    },
    {
        label: "人机对弈",
        value: "Offline",
        disabled: true,
    },
];

interface EngineConfig {
    depth: number;
    time: number;
    threads: number;
    hash: number;
    multipv: number;
    show_wdl: boolean;
    alt_score_gap: number;
    hint_level: number;
}

// 模式由父组件(App)持有, 推演等面板按模式切换显示; 缩放同样由 App 应用到窗口
const mode = defineModel<string>("mode", { default: "LinkAnaly" });
const zoom = defineModel<number>("zoom", { default: 1 });

const zoomOptions = [
    { label: "60%", value: 0.6 },
    { label: "70%", value: 0.7 },
    { label: "80%", value: 0.8 },
    { label: "90%", value: 0.9 },
    { label: "100%", value: 1 },
    { label: "110%", value: 1.1 },
    { label: "125%", value: 1.25 },
];

// 提示强度档位映射到固定搜索深度: 每一手都出自同样的浅思考, 整体一致地弱(而非强引擎概率放水)
// 0=不限棋力(跟随深度/时间设置); 档位标注为自对弈实测校准的约等于天天象棋水平
const hintOptions = [
    { label: "最强(不限)", value: 0 },
    { label: "新手(深度2·约业2-3)", value: 1 },
    { label: "入门(深度4·约业4-5)", value: 2 },
    { label: "初级(深度6·约业5-6)", value: 3 },
    { label: "中级(深度8·约业7-8)", value: 4 },
    { label: "高级(深度12·约业9+)", value: 5 },
];
const hintLevel = computed({
    get: () => hintOptions.find((o) => o.value === config.value.hint_level)?.value ?? config.value.hint_level,
    set: (v: number) => (config.value.hint_level = v),
});

const config = ref<EngineConfig>({
    depth: 0,
    time: 0,
    threads: 0,
    hash: 0,
    multipv: 0,
    show_wdl: false,
    alt_score_gap: 0,
    hint_level: 0,
});

const showEngineConfig = ref(false);
const isEngineRunning = ref(false);

onMounted(async () => {
    await getEngineConfig();
    // 目标窗口关闭等异常导致监听结束时, 同步按钮状态并提示
    listen("listen_stopped", (event) => {
        isEngineRunning.value = false;
        dialog.warning({
            title: "监听停止",
            content: String(event.payload),
            positiveText: "确定",
            showIcon: true,
        });
    });
});

async function copy_fen() {}

async function stopListen() {
    await invoke("stop_listen");
    isEngineRunning.value = false;
}

interface Window {
    id: number;
    title: string;
    app_name: string;
    width: number;
    height: number;
}

const dialog = useDialog();

async function startListen() {
    try {
        // 获取窗口列表
        const windows: Window[] = await invoke("list_windows");

        if (windows.length === 0) {
            dialog.warning({
                title: "警告",
                content: "没有找到可用的窗口",
                positiveText: "确定",
                showIcon: true,
            });
            return;
        }

        const selectedWindowId = ref<number | null>(null);
        const searchQuery = ref("");

        const filteredWindows = computed(() => {
            if (!searchQuery.value) return windows;
            const query = searchQuery.value.toLowerCase();
            return windows.filter(
                (w) => w.title.toLowerCase().includes(query) || w.app_name.toLowerCase().includes(query)
            );
        });

        // 显示窗口选择对话框
        dialog.info({
            title: "选择要监听的窗口",
            class: "window-select-dialog",
            content: () =>
                h(NFlex, { vertical: true, style: "gap: 16px" }, [
                    h(NInput, {
                        clearable: true,
                        placeholder: "搜索窗口...",
                        "onUpdate:value": (val) => (searchQuery.value = val),
                        style: "width: 100%",
                    }),
                    h(NScrollbar, { style: "max-height: 300px" }, [
                        filteredWindows.value.length > 0
                            ? h(
                                  NSpace,
                                  { vertical: true, size: "small" },
                                  filteredWindows.value.map((w) =>
                                      h(
                                          NCard,
                                          {
                                              hoverable: true,
                                              size: "small",
                                              bordered: true,
                                              class: selectedWindowId.value === w.id ? "selected-window" : "",
                                              onClick: () => (selectedWindowId.value = w.id),
                                          },
                                          {
                                              default: () => [
                                                  h(NFlex, { align: "center", justify: "space-between" }, [
                                                      h("div", [
                                                          h("div", { class: "window-title" }, w.title),
                                                          h("div", { class: "window-app" }, [
                                                              w.app_name,
                                                              h(
                                                                  NTag,
                                                                  {
                                                                      size: "tiny",
                                                                      type: "info",
                                                                      style: "margin-left: 8px",
                                                                  },
                                                                  { default: () => `${w.width}×${w.height}` }
                                                              ),
                                                          ]),
                                                      ]),
                                                      h(
                                                          NButton,
                                                          {
                                                              tertiary: true,
                                                              circle: true,
                                                              type:
                                                                  selectedWindowId.value === w.id
                                                                      ? "primary"
                                                                      : "default",
                                                              size: "small",
                                                          },
                                                          {
                                                              default: () =>
                                                                  selectedWindowId.value === w.id ? "✓" : "",
                                                          }
                                                      ),
                                                  ]),
                                              ],
                                          }
                                      )
                                  )
                              )
                            : h(NEmpty, { description: "没有找到匹配的窗口" }),
                    ]),
                ]),
            positiveText: "确定",
            negativeText: "取消",
            style: "max-width: 500px",
            maskClosable: false,
            onPositiveClick: async () => {
                if (!selectedWindowId.value) {
                    dialog.warning({
                        title: "提示",
                        content: "请先选择一个窗口",
                        positiveText: "确定",
                    });
                    return false; // 阻止对话框关闭
                }

                const window = windows.find((w) => w.id === selectedWindowId.value);
                if (window) {
                    try {
                        await invoke("start_listen", { target: window });
                        isEngineRunning.value = true;
                    } catch (error) {
                        dialog.error({
                            title: "错误",
                            content: "启动监听失败:" + String(error),
                            positiveText: "确定",
                        });
                    }
                }
            },
        });
    } catch (error) {
        console.error("启动监听失败:", error);
        dialog.error({
            title: "错误",
            content: "启动监听失败: " + String(error),
            positiveText: "确定",
        });
    }
}

async function setEngineDepth() {
    await invoke("set_engine_depth", { depth: config.value.depth });
}

async function setEngineTime() {
    // 界面单位为秒, 后端保存时转换为毫秒
    await invoke("set_engine_time", { time: config.value.time });
}

async function setEngineThreads() {
    await invoke("set_engine_threads", { num: config.value.threads });
}

async function setEngineHash() {
    await invoke("set_engine_hash", { size: config.value.hash });
}

async function setShowWdl() {
    await invoke("set_engine_show_wdl", { open: config.value.show_wdl });
}

async function setEngineMultipv() {
    await invoke("set_engine_multipv", { num: config.value.multipv });
}

async function setEngineAltScoreGap() {
    await invoke("set_engine_alt_score_gap", { gap: config.value.alt_score_gap });
}

async function setEngineHintLevel() {
    await invoke("set_engine_hint_level", { level: config.value.hint_level });
}

async function getEngineConfig() {
    let result: EngineConfig = await invoke("get_engine_config");
    config.value = {
        ...result,
        time: Number((result.time / 1000).toFixed(1)),
    };
}

async function toggleEngine() {
    isEngineRunning.value ? await stopListen() : await startListen();
}
</script>

<template>
    <n-card class="toolbar" :bordered="false" size="small">
        <n-space vertical size="small">
            <n-flex align="center" justify="space-between">
            <n-flex align="center" :size="8">
                <n-select
                    size="small"
                    v-model:value="mode"
                    :options="options"
                    :consistent-menu-width="false"
                    placeholder="选择模式"
                    class="mode-select"
                />
                <n-tooltip trigger="hover" placement="bottom">
                    <template #trigger>
                        <n-select
                            size="small"
                            v-model:value="zoom"
                            :options="zoomOptions"
                            :consistent-menu-width="false"
                            class="zoom-select"
                        />
                    </template>
                    界面缩放
                </n-tooltip>
            </n-flex>

                <n-space>
                    <n-tooltip trigger="hover" placement="bottom">
                        <template #trigger>
                            <n-button
                                circle
                                size="small"
                                :type="isEngineRunning ? 'error' : 'primary'"
                                @click="toggleEngine"
                            >
                                {{ isEngineRunning ? "停" : "启" }}
                            </n-button>
                        </template>
                        {{ isEngineRunning ? "停止引擎" : "启动引擎" }}
                    </n-tooltip>

                    <n-tooltip trigger="hover" placement="bottom">
                        <template #trigger>
                            <n-button circle size="small" type="info" @click="showEngineConfig = true"
                                >配</n-button
                            >
                        </template>
                        引擎配置
                    </n-tooltip>

                    <n-divider vertical />

                    <n-tooltip trigger="hover" placement="bottom">
                        <template #trigger>
                            <n-button circle size="small" type="success" disabled>识</n-button>
                        </template>
                        图片识别
                    </n-tooltip>

                    <n-tooltip trigger="hover" placement="bottom">
                        <template #trigger>
                            <n-button circle size="small" type="warning" disabled @click="copy_fen">复</n-button>
                        </template>
                        复制局面
                    </n-tooltip>
                </n-space>
            </n-flex>
        </n-space>

        <n-drawer v-model:show="showEngineConfig" :width="300" placement="right">
            <n-drawer-content title="引擎配置">
                <n-form :model="config" label-placement="left" label-width="80">
                    <n-form-item label="提示强度">
                        <n-select
                            v-model:value="hintLevel"
                            :options="hintOptions"
                            :consistent-menu-width="false"
                            style="width: 220px"
                            @update:value="setEngineHintLevel"
                        />
                    </n-form-item>
                    <n-form-item label="深度">
                        <n-input-number
                            v-model:value="config.depth"
                            button-placement="both"
                            :min="0"
                            :max="200"
                            style="width: 120px"
                            @update:value="setEngineDepth"
                        />
                    </n-form-item>
                    <n-form-item label="时间">
                        <n-input-number
                            v-model:value="config.time"
                            button-placement="both"
                            :step="0.5"
                            :precision="1"
                            :min="0"
                            :max="120"
                            style="width: 120px"
                            @update:value="setEngineTime"
                        />
                    </n-form-item>
                    <n-form-item label="线程数">
                        <n-input-number
                            v-model:value="config.threads"
                            button-placement="both"
                            :min="0"
                            :max="64"
                            style="width: 120px"
                            @update:value="setEngineThreads"
                        />
                    </n-form-item>
                    <n-form-item label="哈希表(m)">
                        <n-input-number
                            v-model:value="config.hash"
                            button-placement="both"
                            :min="32"
                            :max="102400"
                            style="width: 120px"
                            @update:value="setEngineHash"
                        />
                    </n-form-item>
                    <n-form-item label="候选招数">
                        <n-input-number
                            v-model:value="config.multipv"
                            button-placement="both"
                            :min="1"
                            :max="5"
                            style="width: 120px"
                            @update:value="setEngineMultipv"
                        />
                    </n-form-item>
                    <n-form-item label="次优分差">
                        <n-input-number
                            v-model:value="config.alt_score_gap"
                            button-placement="both"
                            :min="0"
                            :max="1000"
                            :step="10"
                            style="width: 120px"
                            @update:value="setEngineAltScoreGap"
                        />
                    </n-form-item>
                    <n-form-item label="胜率显示">
                        <n-switch v-model:value="config.show_wdl" @update:value="setShowWdl" />
                    </n-form-item>
                </n-form>
            </n-drawer-content>
        </n-drawer>
    </n-card>
</template>

<style scoped>
.toolbar {
    width: 100%;
    padding: 8px;
    border-radius: 8px;
}

.mode-select {
    width: 110px;
}

.zoom-select {
    width: 88px;
}

:deep(.n-button) {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 36px;
    height: 36px;
    transition: all 0.3s;
}

:deep(.n-button:hover) {
    transform: translateY(-2px);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
}

:deep(.window-title) {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 360px;
}

:deep(.window-app) {
    font-size: 12px;
    color: #999;
    margin-top: 4px;
    display: flex;
    align-items: center;
}

:deep(.selected-window) {
    border-color: var(--primary-color) !important;
    background-color: rgba(var(--primary-color-rgb), 0.05);
}
</style>
