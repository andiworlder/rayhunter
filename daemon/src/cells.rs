use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use rayhunter::cell::store::{CellAggregate, CellContext};

use crate::server::ServerState;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct LiveCells {
    pub mode: &'static str,
    pub recording_name: Option<String>,
    pub context: CellContext,
    pub total_cells_seen: usize,
    pub serving_rsrp_history: Vec<(chrono::DateTime<chrono::FixedOffset>, i16)>,
    pub neighbor_count_history: Vec<(chrono::DateTime<chrono::FixedOffset>, u32)>,
    pub aggregates: Vec<CellAggregate>,
}

#[cfg_attr(feature = "apidocs", utoipa::path(
    get,
    path = "/api/cells/live",
    tag = "BTS",
    responses(
        (status = StatusCode::OK, description = "Current in-memory BTS observatory state", body = LiveCells),
        (status = StatusCode::SERVICE_UNAVAILABLE, description = "BTS Observatory is disabled in config"),
    ),
    summary = "Live BTS state",
    description = "Returns the current serving cell, neighbors, and recent RSRP/neighbor-count history."
))]
pub async fn get_live(
    State(state): State<Arc<ServerState>>,
) -> Result<Json<LiveCells>, (StatusCode, String)> {
    if !state.config.bts_observatory.enabled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "BTS Observatory disabled in config".to_string(),
        ));
    }

    let store = state.cell_store.read().await;
    let qmdl = state.qmdl_store_lock.read().await;
    let recording_name = qmdl
        .current_entry
        .map(|i| qmdl.manifest.entries[i].name.clone());
    drop(qmdl);

    Ok(Json(LiveCells {
        mode: "live",
        recording_name,
        context: store.current_context(state.config.bts_observatory.max_neighbors_in_context),
        total_cells_seen: store.len(),
        serving_rsrp_history: store.serving_rsrp_history(),
        neighbor_count_history: store.neighbor_count_history(),
        aggregates: store.aggregates(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tokio_util::sync::CancellationToken;

    use crate::config::Config;
    use rayhunter::cell::store::CellStore;

    async fn make_state_and_dir(cfg: Config) -> (Arc<ServerState>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let store = crate::qmdl_store::RecordingStore::create(&temp_dir.path().to_path_buf())
            .await
            .unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let (analysis_tx, _analysis_rx) = tokio::sync::mpsc::channel(1);
        let qmdl_store_lock = Arc::new(RwLock::new(store));
        let analysis_status = {
            let store = qmdl_store_lock.try_read().unwrap();
            crate::analysis::AnalysisStatus::new(&store)
        };
        (
            Arc::new(ServerState {
                config_path: "/tmp/test.toml".to_string(),
                config: cfg,
                qmdl_store_lock,
                diag_device_ctrl_sender: tx,
                analysis_status_lock: Arc::new(RwLock::new(analysis_status)),
                analysis_sender: analysis_tx,
                daemon_restart_token: CancellationToken::new(),
                ui_update_sender: None,
                cell_store: Arc::new(RwLock::new(CellStore::new(120))),
            }),
            temp_dir,
        )
    }

    #[tokio::test]
    async fn get_live_returns_empty_when_no_cells() {
        let (state, _dir) = make_state_and_dir(Config::default()).await;
        let response = get_live(State(state)).await.unwrap();
        assert_eq!(response.total_cells_seen, 0);
        assert_eq!(response.mode, "live");
    }

    #[tokio::test]
    async fn get_live_returns_503_when_disabled() {
        let mut cfg = Config::default();
        cfg.bts_observatory.enabled = false;
        let (state, _dir) = make_state_and_dir(cfg).await;
        let err = get_live(State(state)).await.unwrap_err();
        assert_eq!(err.0, StatusCode::SERVICE_UNAVAILABLE);
    }
}
