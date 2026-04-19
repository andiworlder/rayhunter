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

    // IMPORTANT: never hold cell_store.read and qmdl_store.read simultaneously.
    // The hot diag loop holds qmdl_store.write() while awaiting cell_store.write()
    // (diag.rs:477 → process_container → analyze → harness cell_store.apply).
    // If we held cell_store.read here and awaited qmdl_store.read, we'd deadlock:
    //   API:      cell_store.read held, await qmdl_store.read
    //   Hot loop: qmdl_store.write held, await cell_store.write
    // Solution: grab recording_name first (with qmdl lock released), then snapshot cell_store.
    let recording_name = {
        let qmdl = state.qmdl_store_lock.read().await;
        qmdl.current_entry
            .map(|i| qmdl.manifest.entries[i].name.clone())
    };

    let (context, total, rsrp_hist, neigh_hist, aggregates) = {
        let store = state.cell_store.read().await;
        (
            store.current_context(state.config.bts_observatory.max_neighbors_in_context),
            store.len(),
            store.serving_rsrp_history(),
            store.neighbor_count_history(),
            store.aggregates(),
        )
    };

    Ok(Json(LiveCells {
        mode: "live",
        recording_name,
        context,
        total_cells_seen: total,
        serving_rsrp_history: rsrp_hist,
        neighbor_count_history: neigh_hist,
        aggregates,
    }))
}

#[derive(Serialize, Debug)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct ReplayCells {
    pub mode: &'static str,
    pub recording_name: String,
    pub aggregates: Vec<CellAggregate>,
}

#[cfg_attr(feature = "apidocs", utoipa::path(
    get,
    path = "/api/cells/{name}",
    tag = "BTS",
    params(
        ("name" = String, Path, description = "Recording name")
    ),
    responses(
        (status = StatusCode::OK, description = "Latest aggregate snapshot for recording", body = ReplayCells),
        (status = StatusCode::NOT_FOUND, description = "cells file missing"),
        (status = StatusCode::SERVICE_UNAVAILABLE, description = "cells file has no complete flush yet or feature disabled"),
    ),
    summary = "Replay BTS state for a recording",
    description = "Returns the most recent aggregate snapshot persisted for the given recording."
))]
pub async fn get_replay(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<ReplayCells>, (StatusCode, String)> {
    if !state.config.bts_observatory.enabled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "BTS Observatory disabled in config".to_string(),
        ));
    }

    let base = {
        let qmdl = state.qmdl_store_lock.read().await;
        qmdl.path.clone()
    };
    let path = base.join(format!("{name}.cells.ndjson"));

    let body = tokio::fs::read_to_string(&path).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("cells file not found: {e}"),
        )
    })?;

    let snap = rayhunter::cell::store::CellStore::parse_latest_snapshot(&body).ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "no aggregate snapshots in file".to_string(),
    ))?;

    Ok(Json(ReplayCells {
        mode: "replay",
        recording_name: name,
        aggregates: snap.aggregates,
    }))
}

#[derive(Serialize, Debug, Default)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct Timeseries {
    pub serving_rsrp: Vec<(chrono::DateTime<chrono::FixedOffset>, i16)>,
    pub neighbor_count: Vec<(chrono::DateTime<chrono::FixedOffset>, u32)>,
}

#[cfg_attr(feature = "apidocs", utoipa::path(
    get,
    path = "/api/cells/{name}/timeseries",
    tag = "BTS",
    params(
        ("name" = String, Path, description = "Recording name")
    ),
    responses(
        (status = StatusCode::OK, description = "Decimated timeseries for charts", body = Timeseries),
        (status = StatusCode::NOT_FOUND, description = "cells file missing"),
    ),
    summary = "Replay timeseries",
    description = "Returns RSRP history and neighbor-count history. Phase 1 returns empty lists because flush-lines do not persist per-timestamp buffers; implemented fully in Phase 2."
))]
pub async fn get_timeseries(
    State(state): State<Arc<ServerState>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<Timeseries>, (StatusCode, String)> {
    if !state.config.bts_observatory.enabled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "BTS Observatory disabled in config".to_string(),
        ));
    }

    let base = {
        let qmdl = state.qmdl_store_lock.read().await;
        qmdl.path.clone()
    };
    let path = base.join(format!("{name}.cells.ndjson"));

    // Just verify the file exists; timeseries are not yet persisted.
    tokio::fs::metadata(&path).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("cells file not found: {e}"),
        )
    })?;

    Ok(Json(Timeseries::default()))
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

    #[tokio::test]
    async fn get_replay_returns_not_found_when_no_file() {
        let (state, _dir) = make_state_and_dir(Config::default()).await;
        let err = get_replay(
            State(state),
            axum::extract::Path("nonexistent".to_string()),
        ).await.unwrap_err();
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_replay_parses_aggregates_from_file() {
        let (state, dir) = make_state_and_dir(Config::default()).await;
        // Write a synthetic cells.ndjson
        let flush = rayhunter::cell::store::FlushLine {
            flushed_at: chrono::FixedOffset::east_opt(0)
                .unwrap()
                .with_ymd_and_hms(2026, 4, 19, 12, 0, 0)
                .unwrap(),
            aggregates: vec![],
        };
        use chrono::TimeZone;
        let path = dir.path().join("test_rec.cells.ndjson");
        let line = serde_json::to_string(&flush).unwrap();
        tokio::fs::write(&path, format!("{line}\n")).await.unwrap();

        let res = get_replay(
            State(state),
            axum::extract::Path("test_rec".to_string()),
        ).await.unwrap();
        assert_eq!(res.mode, "replay");
        assert_eq!(res.recording_name, "test_rec");
    }

    #[tokio::test]
    async fn get_timeseries_returns_empty_when_file_exists() {
        let (state, dir) = make_state_and_dir(Config::default()).await;
        let path = dir.path().join("test_rec.cells.ndjson");
        tokio::fs::write(&path, "").await.unwrap();
        let res = get_timeseries(
            State(state),
            axum::extract::Path("test_rec".to_string()),
        ).await.unwrap();
        assert_eq!(res.serving_rsrp.len(), 0);
        assert_eq!(res.neighbor_count.len(), 0);
    }
}
