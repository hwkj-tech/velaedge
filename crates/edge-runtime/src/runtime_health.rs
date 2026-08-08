use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use edge_core::{
    AlgorithmRuntimeMetrics, CollectionRuntimeMetrics, EdgeConfigPackage, EdgeHealth,
    EdgeRuntimeMetricsSnapshot, ProtocolRuntimeMetrics,
};
use serde::Serialize;
use tokio::net::TcpListener;

use crate::{PersistentMqttStatus, RuntimeProtocolCatalog, RuntimeProtocolDescriptor};

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeConfigSummary {
    pub version: String,
    pub device_count: usize,
    pub protocol_connection_count: usize,
    pub point_count: usize,
    pub collection_task_count: usize,
    pub data_config_count: usize,
    pub algorithm_count: usize,
    pub mqtt_sink_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeControlPlaneStatus {
    pub connected: bool,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RuntimeCollectionActivity {
    pub cycle_count: u64,
    pub successful_cycle_count: u64,
    pub failed_cycle_count: u64,
    pub samples_collected_total: u64,
    pub mqtt_messages_routed_total: u64,
    pub last_cycle_samples: usize,
    pub last_cycle_mqtt_messages: usize,
    pub last_cycle_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeHealthDocument {
    pub service: &'static str,
    pub edge_id: String,
    pub runtime_id: String,
    pub runtime_version: String,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub health: EdgeHealth,
    pub mqtt_enabled: bool,
    pub mqtt: PersistentMqttStatus,
    pub collection_activity: RuntimeCollectionActivity,
    pub control_plane: RuntimeControlPlaneStatus,
    pub protocol_catalog: Vec<RuntimeProtocolDescriptor>,
    pub metrics: Option<EdgeRuntimeMetricsSnapshot>,
    pub config_summary: Option<RuntimeConfigSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeTrendSample {
    pub timestamp: DateTime<Utc>,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub collection_latency_ms: u64,
    pub collection_success_rate: f64,
    pub last_cycle_samples: usize,
    pub samples_collected_total: u64,
    pub mqtt_publish_success_count: u64,
    pub mqtt_publish_failure_count: u64,
    pub mqtt_published_bytes: u64,
    pub mqtt_last_ack_latency_ms: Option<u64>,
    pub mqtt_outbox_records: u64,
}

#[derive(Clone)]
pub struct RuntimeHealthState {
    inner: Arc<RwLock<RuntimeHealthInner>>,
}

#[derive(Clone)]
struct RuntimeHealthInner {
    document: RuntimeHealthDocument,
    active_config: Option<EdgeConfigPackage>,
    history: VecDeque<RuntimeTrendSample>,
}

const RUNTIME_TREND_CAPACITY: usize = 120;

impl RuntimeHealthState {
    pub fn new(
        edge_id: impl Into<String>,
        runtime_id: impl Into<String>,
        runtime_version: impl Into<String>,
        mqtt_enabled: bool,
    ) -> Self {
        let now = Utc::now();
        Self {
            inner: Arc::new(RwLock::new(RuntimeHealthInner {
                document: RuntimeHealthDocument {
                    service: "velaedge-runtime",
                    edge_id: edge_id.into(),
                    runtime_id: runtime_id.into(),
                    runtime_version: runtime_version.into(),
                    started_at: now,
                    updated_at: now,
                    health: EdgeHealth::Critical,
                    mqtt_enabled,
                    mqtt: PersistentMqttStatus::default(),
                    collection_activity: RuntimeCollectionActivity::default(),
                    control_plane: RuntimeControlPlaneStatus {
                        connected: false,
                        last_success_at: None,
                        last_error: None,
                    },
                    protocol_catalog: RuntimeProtocolCatalog::all(),
                    metrics: None,
                    config_summary: None,
                },
                active_config: None,
                history: VecDeque::with_capacity(RUNTIME_TREND_CAPACITY),
            })),
        }
    }

    pub fn update_runtime(
        &self,
        mut metrics: EdgeRuntimeMetricsSnapshot,
        active_config: Option<&EdgeConfigPackage>,
        mqtt: PersistentMqttStatus,
    ) {
        let mut inner = self.write();
        if let Some(previous) = inner.document.metrics.as_ref() {
            if previous.config_version == metrics.config_version {
                metrics.health = previous.health;
                metrics.collection = previous.collection.clone();
                metrics.protocols = previous.protocols.clone();
                metrics.algorithms = previous.algorithms.clone();
            }
        }
        inner.document.updated_at = Utc::now();
        inner.document.health = metrics.health;
        inner.document.metrics = Some(metrics);
        inner.document.mqtt = mqtt;
        if let Some(package) = active_config {
            let package = sanitize_config(package);
            inner.document.config_summary = Some(config_summary(&package));
            inner.active_config = Some(package);
        }
    }

    pub fn record_control_plane_success(&self, mqtt: PersistentMqttStatus) {
        let mut inner = self.write();
        let now = Utc::now();
        inner.document.updated_at = now;
        inner.document.mqtt = mqtt;
        inner.document.control_plane.connected = true;
        inner.document.control_plane.last_success_at = Some(now);
        inner.document.control_plane.last_error = None;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_collection_observation(
        &self,
        collection: Option<CollectionRuntimeMetrics>,
        protocols: Vec<ProtocolRuntimeMetrics>,
        algorithms: Vec<AlgorithmRuntimeMetrics>,
        samples_collected: usize,
        mqtt_messages_routed: usize,
        collection_error: Option<String>,
        mqtt: PersistentMqttStatus,
    ) {
        let mut inner = self.write();
        let now = Utc::now();
        let observed_health = collection.as_ref().map(|collection| {
            if collection.success_rate >= 0.999
                && protocols.iter().all(|protocol| protocol.connected)
            {
                EdgeHealth::Healthy
            } else {
                EdgeHealth::Degraded
            }
        });
        inner.document.updated_at = now;
        inner.document.mqtt = mqtt;
        if let Some(collection) = collection.as_ref() {
            let activity = &mut inner.document.collection_activity;
            activity.cycle_count = activity.cycle_count.saturating_add(1);
            activity.samples_collected_total = activity
                .samples_collected_total
                .saturating_add(samples_collected as u64);
            activity.mqtt_messages_routed_total = activity
                .mqtt_messages_routed_total
                .saturating_add(mqtt_messages_routed as u64);
            activity.last_cycle_samples = samples_collected;
            activity.last_cycle_mqtt_messages = mqtt_messages_routed;
            activity.last_cycle_at = Some(now);
            if collection_error.is_none() && collection.success_rate > 0.0 {
                activity.successful_cycle_count = activity.successful_cycle_count.saturating_add(1);
                activity.last_success_at = Some(now);
                activity.last_error = None;
            } else {
                activity.failed_cycle_count = activity.failed_cycle_count.saturating_add(1);
                activity.last_failure_at = Some(now);
                activity.last_error = collection_error;
            }
        }
        if let Some(metrics) = inner.document.metrics.as_mut() {
            if let Some(collection) = collection {
                metrics.collection = collection;
            }
            metrics.protocols = protocols;
            metrics.algorithms = algorithms;
            if let Some(health) = observed_health {
                metrics.health = health;
                inner.document.health = health;
            }
        }
        record_trend_sample(&mut inner, now);
    }

    pub fn record_control_plane_error(&self, error: impl Into<String>, mqtt: PersistentMqttStatus) {
        let mut inner = self.write();
        inner.document.updated_at = Utc::now();
        inner.document.mqtt = mqtt;
        inner.document.control_plane.connected = false;
        inner.document.control_plane.last_error = Some(error.into());
    }

    pub fn document(&self) -> RuntimeHealthDocument {
        self.read().document.clone()
    }

    pub fn active_config(&self) -> Option<EdgeConfigPackage> {
        self.read().active_config.clone()
    }

    pub fn history(&self) -> Vec<RuntimeTrendSample> {
        self.read().history.iter().cloned().collect()
    }

    pub fn is_ready(&self) -> bool {
        let inner = self.read();
        inner.active_config.is_some()
            && !matches!(
                inner.document.health,
                EdgeHealth::Critical | EdgeHealth::Offline
            )
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, RuntimeHealthInner> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, RuntimeHealthInner> {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub async fn serve_runtime_health(listener: TcpListener, state: RuntimeHealthState) -> Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness))
        .route("/api/health", get(health))
        .route("/api/history", get(history))
        .route("/api/config", get(config))
        .with_state(state);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(RUNTIME_HEALTH_PAGE)
}

async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

async fn readiness(State(state): State<RuntimeHealthState>) -> impl IntoResponse {
    let ready = state.is_ready();
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(serde_json::json!({"ready": ready})))
}

async fn health(State(state): State<RuntimeHealthState>) -> Json<RuntimeHealthDocument> {
    Json(state.document())
}

async fn history(State(state): State<RuntimeHealthState>) -> Json<Vec<RuntimeTrendSample>> {
    Json(state.history())
}

async fn config(State(state): State<RuntimeHealthState>) -> impl IntoResponse {
    match state.active_config() {
        Some(config) => (StatusCode::OK, Json(config)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "runtime has no active configuration"})),
        )
            .into_response(),
    }
}

fn config_summary(package: &EdgeConfigPackage) -> RuntimeConfigSummary {
    RuntimeConfigSummary {
        version: package.version.clone(),
        device_count: package.devices.len(),
        protocol_connection_count: package.protocol_connections.len(),
        point_count: package.point_mappings.len(),
        collection_task_count: package.collection_tasks.len(),
        data_config_count: package.data_configs.len(),
        algorithm_count: package.algorithms.len(),
        mqtt_sink_count: package.mqtt_uplinks.len(),
    }
}

fn sanitize_config(package: &EdgeConfigPackage) -> EdgeConfigPackage {
    let mut package = package.clone();
    for uplink in &mut package.mqtt_uplinks {
        if uplink.password_env.is_some() {
            uplink.password_env = Some("*** configured via environment ***".to_string());
        }
    }
    package
}

fn record_trend_sample(inner: &mut RuntimeHealthInner, timestamp: DateTime<Utc>) {
    let Some(metrics) = inner.document.metrics.as_ref() else {
        return;
    };
    let mqtt = &inner.document.mqtt;
    let sample = RuntimeTrendSample {
        timestamp,
        cpu_percent: metrics.system.cpu_percent,
        memory_percent: metrics.system.memory_percent,
        disk_percent: metrics.system.disk_percent,
        collection_latency_ms: metrics.collection.average_latency_ms,
        collection_success_rate: metrics.collection.success_rate,
        last_cycle_samples: inner.document.collection_activity.last_cycle_samples,
        samples_collected_total: inner.document.collection_activity.samples_collected_total,
        mqtt_publish_success_count: mqtt.publish_success_count,
        mqtt_publish_failure_count: mqtt.publish_failure_count,
        mqtt_published_bytes: mqtt.published_bytes,
        mqtt_last_ack_latency_ms: mqtt
            .sinks
            .iter()
            .filter_map(|sink| sink.last_ack_latency_ms)
            .max(),
        mqtt_outbox_records: metrics.local_store.buffered_records,
    };
    if inner.history.len() == RUNTIME_TREND_CAPACITY {
        inner.history.pop_front();
    }
    inner.history.push_back(sample);
}

pub const RUNTIME_HEALTH_PAGE: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' fill='%230e2033'/%3E%3Ctext x='5' y='22' font-size='16' font-family='sans-serif' font-weight='700' fill='%235dd8d1'%3EVE%3C/text%3E%3C/svg%3E">
  <title>VelaEdge Runtime</title>
  <style>
    :root{color-scheme:light;--ink:#102033;--muted:#627389;--line:#dbe5ef;--panel:#fff;--bg:#f4f7fa;--blue:#1769aa;--cyan:#0c8fa5;--green:#138a62;--amber:#b06b00;--red:#c63b3b}
    *{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.5 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;letter-spacing:0}
    header{height:68px;background:#0e2033;color:#fff;display:flex;align-items:center;justify-content:space-between;padding:0 28px;border-bottom:3px solid #20a4a6}
    .brand{display:flex;align-items:center;gap:12px}.mark{width:30px;height:30px;border:2px solid #5dd8d1;display:grid;place-items:center;color:#5dd8d1;font-weight:800}.brand strong{font-size:18px}.sub{color:#a9bed1;font-size:12px}
    .badge{display:inline-flex;align-items:center;gap:7px;border:1px solid #3f5870;padding:6px 10px;border-radius:6px}.dot{width:8px;height:8px;border-radius:50%;background:#6d7b8a}.dot.ok{background:#40d39b}.dot.warn{background:#f2b84b}.dot.bad{background:#ff6b6b}
    main{max-width:1440px;margin:0 auto;padding:24px}.toolbar{display:flex;justify-content:space-between;gap:16px;align-items:flex-end;margin-bottom:18px}.toolbar h1{font-size:22px;margin:0 0 3px}.toolbar p{margin:0;color:var(--muted)}.stamp{color:var(--muted);font-variant-numeric:tabular-nums}
    .metrics{display:grid;grid-template-columns:repeat(8,minmax(100px,1fr));gap:1px;border:1px solid var(--line);background:var(--line);margin-bottom:18px}.metric{padding:16px;background:var(--panel)}.metric span{color:var(--muted);font-size:12px}.metric strong{display:block;font-size:24px;margin-top:4px;font-variant-numeric:tabular-nums}
    .layout{display:grid;grid-template-columns:minmax(0,1.7fr) minmax(320px,.8fr);gap:18px}.layout>div{min-width:0}.section{background:var(--panel);border:1px solid var(--line);border-radius:6px;margin-bottom:18px;min-width:0}.section h2{font-size:15px;margin:0;padding:13px 16px;border-bottom:1px solid var(--line);display:flex;justify-content:space-between}.section h2 small{font-weight:500;color:var(--muted)}
    .trend-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr))}.trend{min-width:0;padding:15px 16px 12px;border-right:1px solid var(--line)}.trend:last-child{border-right:0}.trend-head{display:flex;align-items:flex-start;justify-content:space-between;gap:12px;margin-bottom:8px}.trend-title{font-weight:700}.trend-value{color:var(--muted);font-size:12px;text-align:right;font-variant-numeric:tabular-nums}.trend canvas{display:block;width:100%;height:178px}.legend{display:flex;flex-wrap:wrap;gap:8px 14px;min-height:20px;margin-top:5px;color:var(--muted);font-size:11px}.legend span{display:inline-flex;align-items:center;gap:5px}.legend i{width:8px;height:8px;border-radius:2px;background:var(--legend-color)}
    .table-scroll{width:100%;max-width:100%;overflow:auto}table{width:100%;border-collapse:collapse}th,td{text-align:left;padding:11px 16px;border-bottom:1px solid #edf2f6;white-space:nowrap}th{font-size:12px;color:var(--muted);font-weight:600}tr:last-child td{border-bottom:0}.empty{padding:24px;color:var(--muted);text-align:center}.topic{max-width:250px;overflow:hidden;text-overflow:ellipsis}
    .facts{display:grid;grid-template-columns:1fr 1fr}.fact{padding:13px 16px;border-bottom:1px solid #edf2f6}.fact:nth-child(odd){border-right:1px solid #edf2f6}.fact span{display:block;color:var(--muted);font-size:12px}.fact strong{display:block;margin-top:3px;overflow-wrap:anywhere}
    pre{margin:0;padding:16px;max-height:330px;overflow:auto;background:#102033;color:#d6e5ef;font:12px/1.55 ui-monospace,SFMono-Regular,Menlo,monospace;white-space:pre-wrap}
    .error{color:var(--red)}.ok-text{color:var(--green)}.warn-text{color:var(--amber)}
    @media(max-width:1100px){.metrics{grid-template-columns:repeat(4,1fr)}.layout{grid-template-columns:minmax(0,1fr)}.trend-grid{grid-template-columns:1fr}.trend{border-right:0;border-bottom:1px solid var(--line)}.trend:last-child{border-bottom:0}}
    @media(max-width:620px){header{padding:0 16px}main{padding:14px}.metrics{grid-template-columns:repeat(2,1fr)}.toolbar{align-items:flex-start;flex-direction:column}.facts{grid-template-columns:1fr}.fact:nth-child(odd){border-right:0}.metric strong{font-size:22px}}
  </style>
</head>
<body>
  <header><div class="brand"><div class="mark">VE</div><div><strong>VelaEdge Runtime</strong><div class="sub" id="identity">正在读取进程状态</div></div></div><div class="badge"><i class="dot" id="status-dot"></i><span id="status">启动中</span></div></header>
  <main>
    <div class="toolbar"><div><h1>本地运行健康</h1><p>只读诊断视图，配置由控制面下发并在 Runtime 本地生效。</p></div><div class="stamp" id="updated">--</div></div>
    <div class="metrics">
      <div class="metric"><span>CPU</span><strong id="cpu">--</strong></div><div class="metric"><span>内存</span><strong id="memory">--</strong></div>
      <div class="metric"><span>磁盘</span><strong id="disk">--</strong></div><div class="metric"><span>进程运行</span><strong id="uptime">--</strong></div>
      <div class="metric"><span>采集成功率</span><strong id="success">--</strong></div><div class="metric"><span>累计采集点位</span><strong id="samples-total">--</strong></div>
      <div class="metric"><span>MQTT 已确认</span><strong id="mqtt-success-total">--</strong></div><div class="metric"><span>本地待发送</span><strong id="pending">--</strong></div>
    </div>
    <section class="section"><h2>实时趋势 <small id="history-count">等待采样</small></h2><div class="trend-grid">
      <div class="trend"><div class="trend-head"><div class="trend-title">系统负载</div><div class="trend-value" id="system-trend-value">--</div></div><canvas id="system-chart" aria-label="CPU、内存和磁盘使用率趋势"></canvas><div class="legend"><span><i style="--legend-color:#1769aa"></i>CPU</span><span><i style="--legend-color:#0c8fa5"></i>内存</span><span><i style="--legend-color:#d58a19"></i>磁盘</span></div></div>
      <div class="trend"><div class="trend-head"><div class="trend-title">采集周期</div><div class="trend-value" id="collection-trend-value">--</div></div><canvas id="collection-chart" aria-label="采集点位、耗时和失败趋势"></canvas><div class="legend"><span><i style="--legend-color:#138a62"></i>本轮点位</span><span><i style="--legend-color:#1769aa"></i>耗时 ms</span><span><i style="--legend-color:#c63b3b"></i>失败</span></div></div>
      <div class="trend"><div class="trend-head"><div class="trend-title">MQTT 发布与 ACK</div><div class="trend-value" id="mqtt-trend-value">--</div></div><canvas id="mqtt-chart" aria-label="MQTT 发布、ACK 延迟和积压趋势"></canvas><div class="legend"><span><i style="--legend-color:#0c8fa5"></i>已确认</span><span><i style="--legend-color:#7a56c2"></i>ACK ms</span><span><i style="--legend-color:#c63b3b"></i>积压</span></div></div>
    </div></section>
    <div class="layout"><div>
      <section class="section"><h2>协议能力 <small id="protocol-catalog-count">正在读取</small></h2><div class="table-scroll"><table><thead><tr><th>协议</th><th>传输</th><th>成熟度</th><th>采集</th><th>写入</th><th>自动发现</th></tr></thead><tbody id="protocol-catalog"></tbody></table></div></section>
      <section class="section"><h2>协议连接 <small id="protocol-count">0 个</small></h2><div class="table-scroll"><table><thead><tr><th>连接</th><th>协议</th><th>状态</th><th>熔断器</th><th>采集 / 写入</th><th>数据质量</th><th>延迟</th><th>超时 / 错误 / 重连</th></tr></thead><tbody id="protocols"></tbody></table></div></section>
      <section class="section"><h2>MQTT 传输 <small id="mqtt-summary">未连接</small></h2><div class="facts">
        <div class="fact"><span>发布成功</span><strong id="mqtt-published">0</strong></div><div class="fact"><span>发布失败</span><strong id="mqtt-failed">0</strong></div>
        <div class="fact"><span>已发送流量</span><strong id="mqtt-bytes">0 B</strong></div><div class="fact"><span>本地积压</span><strong id="mqtt-pending">0</strong></div>
      </div><div class="table-scroll"><table><thead><tr><th>输出端</th><th>Broker</th><th>状态</th><th>成功 / 失败</th><th>平均 / 最近 ACK</th><th>最近 Topic</th><th>最近发布</th></tr></thead><tbody id="mqtt-sinks"></tbody></table></div></section>
      <section class="section"><h2>生效配置 <small id="config-version">未配置</small></h2><div class="facts" id="config-facts"></div></section>
      <section class="section"><h2>脱敏配置 JSON <small>/api/config</small></h2><pre id="config-json">正在读取...</pre></section>
    </div><div>
      <section class="section"><h2>控制连接</h2><div class="facts">
        <div class="fact"><span>EdgeLink</span><strong id="control">--</strong></div><div class="fact"><span>上次成功</span><strong id="control-time">--</strong></div>
        <div class="fact"><span>MQTT 已连接</span><strong id="mqtt-connected">--</strong></div><div class="fact"><span>MQTT 会话代次</span><strong id="mqtt-generation">--</strong></div>
      </div><div class="fact"><span>最近错误</span><strong id="last-error">无</strong></div></section>
      <section class="section"><h2>采集数据</h2><div class="facts">
        <div class="fact"><span>采集周期</span><strong id="collection-cycles">--</strong></div><div class="fact"><span>成功 / 失败周期</span><strong id="collection-cycle-result">--</strong></div>
        <div class="fact"><span>本轮点位</span><strong id="last-samples">--</strong></div><div class="fact"><span>本轮 MQTT 消息</span><strong id="last-mqtt-messages">--</strong></div>
        <div class="fact"><span>活动任务</span><strong id="tasks">--</strong></div><div class="fact"><span>本轮耗时</span><strong id="latency">--</strong></div>
        <div class="fact"><span>异常点位</span><strong id="bad-points">--</strong></div><div class="fact"><span>算法节点</span><strong id="algorithms">--</strong></div>
        <div class="fact"><span>最近采集</span><strong id="last-collection-at">--</strong></div><div class="fact"><span>最近采集错误</span><strong id="collection-error">无</strong></div>
      </div></section>
      <section class="section"><h2>接口</h2><div class="facts"><div class="fact"><span>存活探针</span><strong>/healthz</strong></div><div class="fact"><span>就绪探针</span><strong>/readyz</strong></div><div class="fact"><span>健康数据</span><strong>/api/health</strong></div><div class="fact"><span>趋势数据</span><strong>/api/history</strong></div><div class="fact"><span>生效配置</span><strong>/api/config</strong></div></div></section>
    </div></div>
  </main>
<script>
const $=id=>document.getElementById(id), text=(id,value)=>$(id).textContent=value;
const pct=value=>Number.isFinite(value)?`${value.toFixed(1)}%`:"--";
const age=seconds=>seconds>=3600?`${Math.floor(seconds/3600)}h`:seconds>=60?`${Math.floor(seconds/60)}m`:`${seconds}s`;
const when=value=>value?new Date(value).toLocaleTimeString():"--";
const bytes=value=>{const n=Number(value||0);if(n<1024)return `${n} B`;if(n<1048576)return `${(n/1024).toFixed(1)} KiB`;return `${(n/1048576).toFixed(1)} MiB`};
let trendHistory=[],loadedConfigVersion=null,resizeTimer;
function healthLabel(v){return {Healthy:"健康",Degraded:"降级",Critical:"严重",Offline:"离线"}[v]||v}
function deltas(rows,key){return rows.map((row,index)=>index?Math.max(0,Number(row[key]||0)-Number(rows[index-1][key]||0)):0)}
function drawTrend(canvasId,rows,series,fixedMax){
  const canvas=$(canvasId),width=Math.max(260,Math.floor(canvas.getBoundingClientRect().width)),height=178,dpr=Math.max(1,window.devicePixelRatio||1);
  canvas.width=Math.floor(width*dpr);canvas.height=Math.floor(height*dpr);
  const ctx=canvas.getContext("2d");ctx.scale(dpr,dpr);ctx.clearRect(0,0,width,height);
  const left=34,right=10,top=12,bottom=23,plotWidth=width-left-right,plotHeight=height-top-bottom;
  ctx.font='11px ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif';ctx.lineWidth=1;ctx.strokeStyle="#e6edf4";ctx.fillStyle="#7b8b9e";
  const values=series.flatMap(item=>item.values.filter(Number.isFinite)),maxValue=fixedMax??Math.max(1,...values),scaleMax=Math.max(1,maxValue);
  for(let line=0;line<=4;line++){const y=top+(plotHeight*line/4);ctx.beginPath();ctx.moveTo(left,y);ctx.lineTo(width-right,y);ctx.stroke();const rawLabel=scaleMax*(1-line/4),label=scaleMax<=4?rawLabel.toFixed(rawLabel%1?1:0):Math.round(rawLabel);ctx.fillText(String(label),2,y+4)}
  if(!rows.length){ctx.fillStyle="#7b8b9e";ctx.textAlign="center";ctx.fillText("等待 Runtime 采样",left+plotWidth/2,top+plotHeight/2);ctx.textAlign="left";return}
  const xAt=index=>left+(rows.length===1?plotWidth/2:plotWidth*index/(rows.length-1)),yAt=value=>top+plotHeight-(Math.max(0,Number(value||0))/scaleMax)*plotHeight;
  series.forEach(item=>{
    const points=item.values.map((value,index)=>({x:xAt(index),y:yAt(value)}));
    if(item.fill&&points.length>1){ctx.beginPath();ctx.moveTo(points[0].x,top+plotHeight);points.forEach(point=>ctx.lineTo(point.x,point.y));ctx.lineTo(points[points.length-1].x,top+plotHeight);ctx.closePath();ctx.fillStyle=item.fill;ctx.fill()}
    ctx.beginPath();points.forEach((point,index)=>{if(index===0)ctx.moveTo(point.x,point.y);else{const previous=points[index-1],middle=(previous.x+point.x)/2;ctx.bezierCurveTo(middle,previous.y,middle,point.y,point.x,point.y)}});ctx.strokeStyle=item.color;ctx.lineWidth=item.width||2;ctx.lineJoin="round";ctx.lineCap="round";ctx.stroke();
    if(points.length===1){ctx.beginPath();ctx.arc(points[0].x,points[0].y,3,0,Math.PI*2);ctx.fillStyle=item.color;ctx.fill()}
  });
  ctx.fillStyle="#7b8b9e";ctx.textAlign="left";ctx.fillText(when(rows[0].timestamp),left,height-5);ctx.textAlign="right";ctx.fillText(when(rows[rows.length-1].timestamp),width-right,height-5);ctx.textAlign="left"
}
function renderCharts(rows){
  trendHistory=rows;const latest=rows[rows.length-1],first=rows[0];
  text("history-count",rows.length?`最近 ${rows.length} 个采样 · ${when(first.timestamp)} 至 ${when(latest.timestamp)}`:"等待采样");
  const publishDelta=deltas(rows,"mqtt_publish_success_count"),byteDelta=deltas(rows,"mqtt_published_bytes");
  text("system-trend-value",latest?`CPU ${pct(latest.cpu_percent)} · 内存 ${pct(latest.memory_percent)}`:"--");
  text("collection-trend-value",latest?`${latest.last_cycle_samples} 点 · ${latest.collection_latency_ms} ms`:"--");
  text("mqtt-trend-value",latest?`${publishDelta.at(-1)||0} 条 · ${bytes(byteDelta.at(-1)||0)} · ACK ${latest.mqtt_last_ack_latency_ms??"--"} ms`:"--");
  drawTrend("system-chart",rows,[
    {values:rows.map(row=>Number(row.cpu_percent)),color:"#1769aa",fill:"rgba(23,105,170,.07)"},
    {values:rows.map(row=>Number(row.memory_percent)),color:"#0c8fa5"},
    {values:rows.map(row=>Number(row.disk_percent)),color:"#d58a19"}
  ],100);
  drawTrend("collection-chart",rows,[
    {values:rows.map(row=>Number(row.last_cycle_samples)),color:"#138a62",fill:"rgba(19,138,98,.07)"},
    {values:rows.map(row=>Number(row.collection_latency_ms)),color:"#1769aa"},
    {values:rows.map(row=>Number(row.collection_success_rate)<.999?1:0),color:"#c63b3b",width:2.5}
  ]);
  drawTrend("mqtt-chart",rows,[
    {values:publishDelta,color:"#0c8fa5",fill:"rgba(12,143,165,.08)"},
    {values:rows.map(row=>Number(row.mqtt_last_ack_latency_ms||0)),color:"#7a56c2"},
    {values:rows.map(row=>Number(row.mqtt_outbox_records||0)),color:"#c63b3b",width:2.5}
  ]);
}
function renderFacts(summary){const items=[["设备",summary.device_count],["协议连接",summary.protocol_connection_count],["点位",summary.point_count],["采集任务",summary.collection_task_count],["数据编排",summary.data_config_count],["算法",summary.algorithm_count],["MQTT 输出",summary.mqtt_sink_count],["版本",summary.version]];$("config-facts").replaceChildren(...items.map(([k,v])=>{const d=document.createElement("div");d.className="fact";const s=document.createElement("span");s.textContent=k;const b=document.createElement("strong");b.textContent=v;d.append(s,b);return d}))}
function renderProtocolCatalog(rows){const maturityLabels={laboratory:"实验室",deployment_candidate:"部署候选",production:"生产可用",planned:"规划中"},transportLabels={internal:"内部",serial:"串口",tcp:"TCP",udp:"UDP",tcp_udp:"TCP / UDP"};$("protocol-catalog").replaceChildren(...rows.map(row=>{const tr=document.createElement("tr"),values=[row.displayName,transportLabels[row.transport]||row.transport,maturityLabels[row.maturity]||row.maturity,row.telemetryRead?"支持":"不支持",row.commandWrite?"支持":"只读",row.automaticDiscovery?"支持":"不支持"];values.forEach((value,index)=>{const td=document.createElement("td");td.textContent=value;if(index===2)td.className=row.maturity==="production"?"ok-text":row.maturity==="deployment_candidate"?"warn-text":row.maturity==="planned"?"error":"";tr.append(td)});return tr}));const executable=rows.filter(row=>row.maturity!=="planned"&&row.telemetryRead).length;text("protocol-catalog-count",`${executable} 个可执行 / ${rows.length} 个已登记`)}
function renderProtocols(rows){$("protocols").replaceChildren(...(rows.length?rows.map(row=>{const tr=document.createElement("tr");const circuit={Closed:"关闭",Open:"已熔断",HalfOpen:"恢复探测"}[row.circuit_state]||row.circuit_state||"关闭",qualityLabels={good:"正常",uncertain_protocol:"协议不确定",uncertain_last_known:"沿用旧值",uncertain_out_of_range:"超量程",uncertain_substituted:"替代值",uncertain_overflow:"溢出",bad_communication:"通信失败",bad_timeout:"超时",bad_protocol:"协议异常",bad_decode:"解码失败",bad_configuration:"配置错误",bad_out_of_service:"停止服务"},quality=row.last_quality_code?(qualityLabels[row.last_quality_code]||row.last_quality_code):"暂无采样";[row.connection_id,row.protocol,row.connected?"已连接":"未连接",`${circuit} · 失败 ${row.consecutive_failure_count||0} · 拒绝 ${row.circuit_rejected_count||0}`,`采集 ${row.collection_success_count||0}/${row.collection_attempt_count||0} · 写入 ${row.write_success_count||0}/${row.write_attempt_count||0}`,`${quality} · G ${row.good_value_count||0} / U ${row.uncertain_value_count||0} / B ${row.bad_value_count||0}`,`${row.latency_ms} ms`,`${row.timeout_count} / ${row.error_count} / ${row.reconnect_count}`].forEach((v,i)=>{const td=document.createElement("td");td.textContent=v;if(i===2)td.className=row.connected?"ok-text":"error";if(i===3&&row.circuit_state==="Open")td.className="error";if(i===5&&(row.bad_value_count||0)>0)td.className="error";else if(i===5&&(row.uncertain_value_count||0)>0)td.className="warn-text";tr.append(td)});return tr}):[Object.assign(document.createElement("tr"),{innerHTML:'<td colspan="8" class="empty">暂无协议运行数据</td>'})]))}
function renderMqttSinks(rows){$("mqtt-sinks").replaceChildren(...(rows.length?rows.map(row=>{const tr=document.createElement("tr");const values=[row.sink_id,row.broker,row.connected?"已连接":"未连接",`${row.publish_success_count} / ${row.publish_failure_count}`,`${row.average_ack_latency_ms} / ${row.last_ack_latency_ms??"--"} ms`,row.last_topic||"--",when(row.last_publish_at)];values.forEach((v,i)=>{const td=document.createElement("td");td.textContent=v;if(i===2)td.className=row.connected?"ok-text":"error";if(i===5){td.classList.add("topic");td.title=row.last_topic||""}tr.append(td)});return tr}):[Object.assign(document.createElement("tr"),{innerHTML:'<td colspan="7" class="empty">暂无 MQTT 输出运行数据</td>'})]))}
async function refresh(){
  try{
    const [r,hr]=await Promise.all([fetch("/api/health",{cache:"no-store"}),fetch("/api/history",{cache:"no-store"})]);const d=await r.json(),historyRows=hr.ok?await hr.json():[],m=d.metrics,s=m?.system,c=m?.collection,a=d.collection_activity||{},q=d.mqtt||{};
    text("identity",`${d.edge_id} · ${d.runtime_id} · v${d.runtime_version}`);text("status",healthLabel(d.health));$("status-dot").className=`dot ${d.health==="Healthy"?"ok":d.health==="Degraded"?"warn":"bad"}`;
    text("updated",`更新于 ${new Date(d.updated_at).toLocaleTimeString()}`);text("cpu",pct(s?.cpu_percent));text("memory",pct(s?.memory_percent));text("disk",pct(s?.disk_percent));text("uptime",s?age(s.process_uptime_seconds):"--");
    text("success",a.cycle_count?pct((a.successful_cycle_count/a.cycle_count)*100):c?pct(c.success_rate*100):"--");text("samples-total",a.samples_collected_total??0);text("mqtt-success-total",q.publish_success_count??0);text("pending",m?.local_store?.buffered_records??"--");
    text("tasks",c?.active_task_count??"--");text("latency",c?`${c.average_latency_ms} ms`:"--");text("bad-points",c?.bad_point_count??"--");text("algorithms",m?.algorithms?.length??"--");
    text("collection-cycles",a.cycle_count??0);text("collection-cycle-result",`${a.successful_cycle_count??0} / ${a.failed_cycle_count??0}`);text("last-samples",a.last_cycle_samples??0);text("last-mqtt-messages",a.last_cycle_mqtt_messages??0);text("last-collection-at",when(a.last_cycle_at));text("collection-error",a.last_error||"无");$("collection-error").className=a.last_error?"error":"";
    renderProtocolCatalog(d.protocol_catalog||[]);const protocols=m?.protocols||[];text("protocol-count",`${protocols.length} 个`);renderProtocols(protocols);
    text("control",d.control_plane.connected?"已连接":"未连接");$("control").className=d.control_plane.connected?"ok-text":"error";text("control-time",d.control_plane.last_success_at?new Date(d.control_plane.last_success_at).toLocaleTimeString():"--");text("last-error",d.control_plane.last_error||"无");
    text("mqtt-connected",`${q.connected_sink_count??0} / ${q.configured_sink_count??0}`);text("mqtt-generation",q.connection_generation??0);text("mqtt-summary",`${q.connected_sink_count??0}/${q.configured_sink_count??0} 已连接`);text("mqtt-published",q.publish_success_count??0);text("mqtt-failed",q.publish_failure_count??0);text("mqtt-bytes",bytes(q.published_bytes));text("mqtt-pending",m?.local_store?.buffered_records??0);renderMqttSinks(q.sinks||[]);
    renderCharts(historyRows);
    if(d.config_summary){text("config-version",d.config_summary.version);renderFacts(d.config_summary);if(loadedConfigVersion!==d.config_summary.version){const cr=await fetch("/api/config",{cache:"no-store"});text("config-json",cr.ok?JSON.stringify(await cr.json(),null,2):"Runtime 暂无生效配置");loadedConfigVersion=d.config_summary.version}}
  }catch(e){text("status","健康服务异常");$("status-dot").className="dot bad";text("last-error",e.message)}
}
window.addEventListener("resize",()=>{clearTimeout(resizeTimer);resizeTimer=setTimeout(()=>renderCharts(trendHistory),120)});
refresh();setInterval(refresh,2000);
</script>
</body></html>"##;

#[cfg(test)]
mod tests {
    use edge_core::{
        CloudSyncMetrics, CollectionRuntimeMetrics, EdgeRuntimeMetricsSnapshot, LocalStoreMetrics,
        MqttUplinkConfig, SystemRuntimeMetrics,
    };

    use super::*;

    fn metrics(health: EdgeHealth) -> EdgeRuntimeMetricsSnapshot {
        EdgeRuntimeMetricsSnapshot {
            edge_id: "edge-1".to_string(),
            runtime_id: "runtime-1".to_string(),
            config_version: "v1".to_string(),
            timestamp: Utc::now(),
            health,
            system: SystemRuntimeMetrics {
                cpu_percent: 1.0,
                memory_percent: 2.0,
                disk_percent: 3.0,
                process_uptime_seconds: 4,
            },
            collection: CollectionRuntimeMetrics {
                active_task_count: 1,
                success_rate: 1.0,
                average_latency_ms: 5,
                bad_point_count: 0,
            },
            protocols: Vec::new(),
            local_store: LocalStoreMetrics {
                backend: "rocksdb".to_string(),
                buffered_records: 0,
                oldest_buffer_age_seconds: 0,
                disk_usage_percent: 3.0,
            },
            algorithms: Vec::new(),
            mqtt: Default::default(),
            cloud_sync: CloudSyncMetrics {
                connected: true,
                last_sync_seconds_ago: 0,
                pending_uploads: 0,
                desired_version: "v1".to_string(),
                reported_version: "v1".to_string(),
            },
        }
    }

    #[test]
    fn ready_requires_active_non_critical_config() {
        let state = RuntimeHealthState::new("edge-1", "runtime-1", "1.0.0", true);
        assert!(!state.is_ready());
        let package = EdgeConfigPackage::new("edge-1", "v1");
        state.update_runtime(
            metrics(EdgeHealth::Healthy),
            Some(&package),
            Default::default(),
        );
        assert!(state.is_ready());
    }

    #[test]
    fn health_page_exposes_truthful_protocol_maturity() {
        let state = RuntimeHealthState::new("edge-1", "runtime-1", "1.0.0", true);
        let document = state.document();
        let modbus = document
            .protocol_catalog
            .iter()
            .find(|protocol| protocol.capability_id == "modbus-tcp")
            .unwrap();
        let simulated = document
            .protocol_catalog
            .iter()
            .find(|protocol| protocol.capability_id == "simulated")
            .unwrap();

        assert_eq!(
            modbus.maturity,
            crate::RuntimeProtocolMaturity::DeploymentCandidate
        );
        assert_eq!(
            simulated.maturity,
            crate::RuntimeProtocolMaturity::Laboratory
        );
        assert!(RUNTIME_HEALTH_PAGE.contains("协议能力"));
        assert!(RUNTIME_HEALTH_PAGE.contains("deployment_candidate"));
    }

    #[test]
    fn active_config_redacts_password_environment_reference() {
        let state = RuntimeHealthState::new("edge-1", "runtime-1", "1.0.0", true);
        let package = EdgeConfigPackage::new("edge-1", "v1").with_mqtt_uplink(
            MqttUplinkConfig::velamq("mqtt", "mqtt://127.0.0.1:1883", "runtime-1")
                .with_credentials_env("runtime", "VELAMQ_PASSWORD"),
        );
        state.update_runtime(
            metrics(EdgeHealth::Healthy),
            Some(&package),
            Default::default(),
        );
        assert_eq!(
            state.active_config().unwrap().mqtt_uplinks[0]
                .password_env
                .as_deref(),
            Some("*** configured via environment ***")
        );
    }

    #[test]
    fn periodic_system_refresh_preserves_latest_collection_observation() {
        let state = RuntimeHealthState::new("edge-1", "runtime-1", "1.0.0", true);
        let package = EdgeConfigPackage::new("edge-1", "v1");
        state.update_runtime(
            metrics(EdgeHealth::Healthy),
            Some(&package),
            Default::default(),
        );
        state.record_collection_observation(
            Some(CollectionRuntimeMetrics {
                active_task_count: 1,
                success_rate: 1.0,
                average_latency_ms: 8,
                bad_point_count: 0,
            }),
            vec![ProtocolRuntimeMetrics {
                connection_id: "modbus".to_string(),
                protocol: "Modbus TCP".to_string(),
                connected: true,
                latency_ms: 8,
                timeout_count: 0,
                error_count: 0,
                reconnect_count: 0,
                collection_attempt_count: 1,
                collection_success_count: 1,
                write_attempt_count: 0,
                write_success_count: 0,
                circuit_state: Default::default(),
                consecutive_failure_count: 0,
                circuit_open_count: 0,
                circuit_rejected_count: 0,
                last_quality_code: None,
                good_value_count: 0,
                uncertain_value_count: 0,
                bad_value_count: 0,
                subscription_count: 0,
                notification_count: 0,
                subscription_error_count: 0,
                fallback_poll_count: 0,
            }],
            Vec::new(),
            3,
            1,
            None,
            Default::default(),
        );

        state.update_runtime(
            metrics(EdgeHealth::Healthy),
            Some(&package),
            Default::default(),
        );

        let current = state.document().metrics.unwrap();
        assert_eq!(current.collection.success_rate, 1.0);
        assert!(current.protocols[0].connected);
        let activity = state.document().collection_activity;
        assert_eq!(activity.cycle_count, 1);
        assert_eq!(activity.successful_cycle_count, 1);
        assert_eq!(activity.samples_collected_total, 3);
        assert_eq!(activity.mqtt_messages_routed_total, 1);
        let history = state.history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].last_cycle_samples, 3);
        assert_eq!(history[0].collection_latency_ms, 8);
        assert_eq!(history[0].samples_collected_total, 3);
    }

    #[test]
    fn collection_activity_records_failures_without_inventing_samples() {
        let state = RuntimeHealthState::new("edge-1", "runtime-1", "1.0.0", true);
        let package = EdgeConfigPackage::new("edge-1", "v1");
        state.update_runtime(
            metrics(EdgeHealth::Healthy),
            Some(&package),
            Default::default(),
        );
        state.record_collection_observation(
            Some(CollectionRuntimeMetrics {
                active_task_count: 1,
                success_rate: 0.0,
                average_latency_ms: 120,
                bad_point_count: 1,
            }),
            Vec::new(),
            Vec::new(),
            0,
            0,
            Some("modbus response timed out".to_string()),
            Default::default(),
        );

        let activity = state.document().collection_activity;
        assert_eq!(activity.cycle_count, 1);
        assert_eq!(activity.successful_cycle_count, 0);
        assert_eq!(activity.failed_cycle_count, 1);
        assert_eq!(activity.samples_collected_total, 0);
        assert_eq!(
            activity.last_error.as_deref(),
            Some("modbus response timed out")
        );
        let history = state.history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].last_cycle_samples, 0);
        assert_eq!(history[0].collection_success_rate, 0.0);
    }
}
