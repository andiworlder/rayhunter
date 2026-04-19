export type RAT = 'Lte' | 'Umts' | 'Gsm';

export interface Plmn {
    mcc: number;
    mnc: number;
    mnc_is_3_digit: boolean;
}

export interface CellIdentity {
    rat: RAT;
    plmn?: Plmn;
    tac?: number;
    lac?: number;
    cid?: number;
    pci?: number;
    psc?: number;
    bsic?: number;
    earfcn?: number;
    uarfcn?: number;
    arfcn?: number;
}

export interface SignalSample {
    timestamp?: string;
    rsrp_dbm?: number;
    rsrq_db?: number;
    rscp_dbm?: number;
    ecno_db?: number;
    rxlev?: number;
}

export interface NeighborSnapshot {
    identity: CellIdentity;
    signal: SignalSample;
    operator_name?: string;
}

export interface CellContext {
    serving?: CellIdentity;
    serving_signal?: SignalSample;
    serving_operator?: string;
    neighbors: NeighborSnapshot[];
}

export interface CellAggregate {
    identity: CellIdentity;
    first_seen: string;
    last_seen: string;
    observation_count: number;
    is_serving_ever: boolean;
    current_signal?: SignalSample;
    signal_min?: SignalSample;
    signal_max?: SignalSample;
    signal_avg_rsrp_dbm?: number;
    signal_avg_rxlev?: number;
    operator_name?: string;
}

export interface LiveCells {
    mode: 'live';
    recording_name?: string;
    context: CellContext;
    total_cells_seen: number;
    serving_rsrp_history: [string, number][];
    neighbor_count_history: [string, number][];
    aggregates: CellAggregate[];
}

export interface ReplayCells {
    mode: 'replay';
    recording_name: string;
    aggregates: CellAggregate[];
}

export interface AlertRow {
    timestamp: string;
    severity: 'Low' | 'Medium' | 'High' | 'Informational';
    message: string;
    analyzer_name?: string;
    cell_context?: CellContext;
}

export async function fetch_live(): Promise<LiveCells> {
    const r = await fetch('/api/cells/live');
    if (!r.ok) throw new Error(`cells/live ${r.status}`);
    return r.json();
}

export async function fetch_replay(name: string): Promise<ReplayCells> {
    const r = await fetch(`/api/cells/${encodeURIComponent(name)}`);
    if (!r.ok) throw new Error(`cells/${name} ${r.status}`);
    return r.json();
}

export function quality_band_rsrp(rsrp: number): { label: string; color: string } {
    if (rsrp >= -80) return { label: 'EXCELLENT', color: 'green-700' };
    if (rsrp >= -90) return { label: 'GOOD', color: 'green-500' };
    if (rsrp >= -100) return { label: 'FAIR', color: 'amber-500' };
    return { label: 'POOR', color: 'red-500' };
}

export interface ReportEvent {
    event_type: 'Informational' | 'Low' | 'Medium' | 'High';
    message: string;
    cell_context?: CellContext;
}

export interface ReportRow {
    packet_timestamp?: string;
    skipped_message_reason?: string;
    events: (ReportEvent | null)[];
}

export interface ReportMetadata {
    report_version?: number;
    rayhunter?: unknown;
    analyzers?: { name: string; description: string; version: number }[];
}

/**
 * Fetch the analysis report for a recording and extract alert rows that
 * carry a non-null event (severity != Informational).
 */
export async function fetch_alerts_with_context(
    recording_name: string,
    min_severity: 'Low' | 'Medium' | 'High' = 'Low',
): Promise<AlertRow[]> {
    const order: Record<ReportEvent['event_type'], number> = {
        Informational: 0,
        Low: 1,
        Medium: 2,
        High: 3,
    };
    const threshold = order[min_severity];

    const r = await fetch(`/api/analysis-report/${encodeURIComponent(recording_name)}`);
    if (!r.ok) {
        // 404 is fine: the recording may not yet have been analyzed.
        if (r.status === 404) return [];
        throw new Error(`analysis-report ${r.status}`);
    }
    const text = await r.text();
    const lines = text.split('\n').filter((l) => l.trim().length > 0);

    let analyzer_names: string[] = [];
    const alerts: AlertRow[] = [];

    for (let i = 0; i < lines.length; i++) {
        try {
            const parsed = JSON.parse(lines[i]);
            if (i === 0 && parsed.analyzers) {
                analyzer_names = (parsed as ReportMetadata).analyzers?.map((a) => a.name) ?? [];
                continue;
            }
            const row = parsed as ReportRow;
            if (!row.events) continue;
            row.events.forEach((e, idx) => {
                if (!e) return;
                if (order[e.event_type] < threshold) return;
                alerts.push({
                    timestamp: row.packet_timestamp ?? '—',
                    severity: e.event_type,
                    message: e.message,
                    analyzer_name: analyzer_names[idx],
                    cell_context: e.cell_context,
                });
            });
        } catch (_) {
            // skip malformed lines
        }
    }
    return alerts;
}

export function format_plmn(p?: Plmn): string {
    if (!p) return '—';
    const mnc = p.mnc_is_3_digit
        ? String(p.mnc).padStart(3, '0')
        : String(p.mnc).padStart(2, '0');
    return `${String(p.mcc).padStart(3, '0')}-${mnc}`;
}
