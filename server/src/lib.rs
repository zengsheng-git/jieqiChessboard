use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::thread;

use engine::Engine;
use tauri::Manager as _;

mod attack;
mod chess;
mod common;
mod config;
mod engine;
mod listen;
mod logger;
mod practice;
mod worker;
mod yolo;

// 全局共享状态，用Arc和Mutex包装以实现线程安全共享
struct SharedState {
    config: Arc<RwLock<config::Config>>,
    engine: Arc<Mutex<Engine>>,
    listen_thread: Mutex<Option<thread::JoinHandle<()>>>,
}

static SHARED_STATE: OnceLock<SharedState> = OnceLock::new();

// 提示档位版本号: set_engine_hint_level 每次切换时递增, 监听线程比对发现变化后
// 即使棋盘未变也立即按新档位重算当前局面提示(否则旧提示一直挂到下一次走子)
static HINT_LEVEL_VERSION: AtomicU32 = AtomicU32::new(0);

pub fn bump_hint_version() {
    HINT_LEVEL_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn hint_version() -> u32 {
    HINT_LEVEL_VERSION.load(std::sync::atomic::Ordering::Relaxed)
}

fn engine_lib_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    let resolve = |rel: &str| app.path().resolve(rel, tauri::path::BaseDirectory::Resource).unwrap();
    // 安装包内 tauri 会把 "../libs" 目标路径清洗成 "_up_/libs"（资源随安装包落在应用目录内）
    for rel in ["_up_/libs/pikajieqi", "libs/pikajieqi", "../libs/pikajieqi", "../../libs/pikajieqi"] {
        let path = resolve(rel);
        if path.join("pikajieqi-windows.exe").exists() {
            return path;
        }
    }
    resolve("../libs/pikajieqi")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            logger::init_tracer(tracing::Level::DEBUG, &app.path().app_data_dir().unwrap());

            let _ = SHARED_STATE.get_or_init(|| {
                let config = config::Config::load(&app.path().config_dir().unwrap());
                let lib_path = engine_lib_path(app.handle());
                let engine = Arc::new(Mutex::new(engine::Engine::new(&lib_path)
                    .unwrap_or_else(|e| panic!("引擎启动失败: {e}"))));
                {
                    let mut eng = engine.lock().unwrap();
                    eng.set_show_wdl(config.engine.show_wdl);
                    eng.set_hash(config.engine.hash);
                    eng.set_threads(config.engine.threads);
                }

                SharedState {
                    config: Arc::new(RwLock::new(config)),
                    engine,
                    listen_thread: Mutex::new(None),
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            reload_engine,
            listen::list_windows,
            worker::start_listen,
            worker::stop_listen,
            config::get_engine_config,
            config::set_engine_depth,
            config::set_engine_time,
            config::set_engine_threads,
            config::set_engine_hash,
            config::set_engine_show_wdl,
            config::set_engine_multipv,
            config::set_engine_alt_score_gap,
            config::set_engine_hint_level,
            practice::practice_start,
            practice::practice_moves,
            practice::practice_do_move,
            practice::practice_undo,
            practice::practice_reset,
            practice::practice_analyze,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = SHARED_STATE.get() {
                    if let Ok(mut engine) = state.engine.lock() {
                        engine.shutdown();
                    }
                }
            }
        });
}

#[tauri::command]
fn reload_engine(app: tauri::AppHandle) {
    let lib_path = engine_lib_path(&app);
    let state = SHARED_STATE.get().unwrap();
    let engine_config = state.config.read().unwrap().engine;
    let mut engine = state.engine.lock().unwrap();
    engine.reload(&lib_path, &engine_config);
}
