//! PerfectAssistant2API 网关入口

use std::sync::Arc;
use perfectassistant2api::api::{build_router, AppState};
use perfectassistant2api::config::Config;
use perfectassistant2api::models::ModelRegistry;
use perfectassistant2api::upstream::UpstreamClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,perfectassistant2api=debug")),
        )
        .with_target(false)
        .init();

    let cfg = Config::load(Config::resolve_config_path().as_deref())?;
    let listen_addr = cfg.listen_addr.clone();

    tracing::info!("PerfectAssistant2API v{} 启动", env!("CARGO_PKG_VERSION"));
    tracing::info!("监听 {}（OpenAI /v1 · Anthropic /v1/messages）", listen_addr);
    tracing::info!("上游 {}（免费端点 {}）", cfg.upstream_origin(), cfg.upstream_free_path);

    let upstream = Arc::new(UpstreamClient::new(&cfg)?);
    if !cfg.skip_upstream_check {
        match upstream.check_health().await {
            Ok(_) => tracing::info!("上游健康检查通过"),
            Err(e) => tracing::warn!("上游健康检查失败（继续启动）: {e}"),
        }
    }

    let registry = Arc::new(ModelRegistry::new(&cfg.default_model));
    tracing::info!("工具目录已载入 {} 个模型", registry.len());

    let api_keys = Arc::new(std::sync::RwLock::new(cfg.api_keys.clone()));
    if cfg.api_keys.is_empty() {
        tracing::info!("未配置 api_keys：仅本机放行（面板可一键生成）");
    }

    let state = AppState {
        cfg: Arc::new(cfg),
        upstream,
        registry,
        api_keys,
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("HTTP 服务已启动: http://{listen_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
