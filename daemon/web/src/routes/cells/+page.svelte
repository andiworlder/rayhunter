<script lang="ts">
    import { browser } from '$app/environment';
    import { page } from '$app/state';
    import {
        fetch_live,
        fetch_replay,
        type LiveCells,
        type CellAggregate,
        type NeighborSnapshot,
    } from '$lib/cells.svelte';
    import ServingCellCard from '$lib/components/ServingCellCard.svelte';
    import NeighborCellsTable from '$lib/components/NeighborCellsTable.svelte';
    import SignalHistoryChart from '$lib/components/SignalHistoryChart.svelte';
    import AlertsStrip from '$lib/components/AlertsStrip.svelte';

    let recording_param = $derived(
        browser ? (page.url.searchParams.get('recording') ?? undefined) : undefined
    );
    let mode = $derived(recording_param ? 'replay' : 'live');
    let refresh_seconds: number = $state(2);
    let error: string | undefined = $state(undefined);

    let live_data: LiveCells | undefined = $state(undefined);
    let replay_aggregates: CellAggregate[] = $state([]);

    $effect(() => {
        let cancel = false;
        async function poll() {
            try {
                if (mode === 'live') {
                    live_data = await fetch_live();
                } else if (recording_param) {
                    const r = await fetch_replay(recording_param);
                    replay_aggregates = r.aggregates;
                }
                error = undefined;
            } catch (e) {
                error = e instanceof Error ? e.message : String(e);
            }
        }

        poll();

        if (mode === 'live') {
            const id = setInterval(() => {
                if (!cancel && !document.hidden) poll();
            }, refresh_seconds * 1000);
            return () => {
                cancel = true;
                clearInterval(id);
            };
        }
        return () => {
            cancel = true;
        };
    });

    let all_aggregates: CellAggregate[] = $derived.by(() => {
        const ld: LiveCells | undefined = live_data;
        return mode === 'live' ? (ld?.aggregates ?? []) : replay_aggregates;
    });
    let serving_aggregate: CellAggregate | undefined = $derived(
        all_aggregates.find((a) => a.is_serving_ever)
    );
    let neighbor_aggregates: CellAggregate[] = $derived(
        all_aggregates.filter((a) => !a.is_serving_ever)
    );

    let neighbor_snapshots: NeighborSnapshot[] = $derived.by(() => {
        const ld: LiveCells | undefined = live_data;
        if (mode === 'live' && ld) {
            return ld.context.neighbors;
        }
        return neighbor_aggregates.map((a) => ({
            identity: a.identity,
            signal: a.current_signal ?? {},
            operator_name: a.operator_name,
        }));
    });

    let serving_identity = $derived.by(() => {
        const ld: LiveCells | undefined = live_data;
        return ld
            ? ld.context.serving ?? serving_aggregate?.identity
            : serving_aggregate?.identity;
    });
    let serving_signal = $derived.by(() => {
        const ld: LiveCells | undefined = live_data;
        return ld
            ? ld.context.serving_signal ?? serving_aggregate?.current_signal
            : serving_aggregate?.current_signal;
    });
    let serving_operator = $derived.by(() => {
        const ld: LiveCells | undefined = live_data;
        return ld
            ? ld.context.serving_operator ?? serving_aggregate?.operator_name
            : serving_aggregate?.operator_name;
    });

    let total_cells = $derived.by(() => {
        const ld: LiveCells | undefined = live_data;
        return ld ? ld.total_cells_seen : replay_aggregates.length;
    });
</script>

<div class="p-4 bg-rayhunter-blue text-white flex justify-between items-center">
    <div>
        <div class="text-lg font-black">RAYHUNTER MONITOR</div>
        <div class="text-xs opacity-80">
            {mode.toUpperCase()} · {refresh_seconds}s · {total_cells} cells
        </div>
    </div>
    <div class="flex items-center gap-3">
        {#if mode === 'live'}
            <div class="flex items-center gap-1 text-sm">
                <span class="w-2 h-2 rounded-full bg-green-400 animate-pulse"></span>
                MONITORING
            </div>
            <select bind:value={refresh_seconds} class="text-black rounded px-1 py-0.5 text-sm">
                <option value={1}>1s</option>
                <option value={2}>2s</option>
                <option value={5}>5s</option>
                <option value={10}>10s</option>
            </select>
        {:else}
            <div class="text-gray-200 text-sm">REPLAY · {recording_param}</div>
        {/if}
    </div>
</div>

{#if error}
    <div class="bg-red-50 border-l-4 border-red-600 p-3 text-sm">
        Error: {error}
    </div>
{/if}

<div class="p-3 space-y-3 max-w-xl mx-auto">
    <ServingCellCard
        serving={serving_identity}
        signal={serving_signal}
        operator={serving_operator}
    />
    <NeighborCellsTable neighbors={neighbor_snapshots} />

    {#if mode === 'live' && live_data}
        <SignalHistoryChart
            title="RSRP History"
            points={live_data.serving_rsrp_history}
        />
        <SignalHistoryChart
            title="Neighbor Count"
            points={live_data.neighbor_count_history.map(
                ([t, c]) => [t, c] as [string, number]
            )}
            color="indigo"
            y_domain={[0, 10]}
        />
    {/if}

    <!-- AlertsStrip is wired in Task 25 -->
    <AlertsStrip alerts={[]} />
</div>
