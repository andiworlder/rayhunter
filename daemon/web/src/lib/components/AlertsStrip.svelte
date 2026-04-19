<script lang="ts">
    import type { CellContext, AlertRow } from '$lib/cells.svelte';
    import { format_plmn } from '$lib/cells.svelte';

    let { alerts }: { alerts: AlertRow[] } = $props();

    const color_for = (s: AlertRow['severity']) =>
        s === 'High'
            ? 'border-red-600 bg-red-50'
            : s === 'Medium'
            ? 'border-amber-500 bg-amber-50'
            : s === 'Low'
            ? 'border-blue-500 bg-blue-50'
            : 'border-gray-300 bg-gray-50';
</script>

{#if alerts.length > 0}
    <div class="border-l-4 border-red-600 rounded bg-white shadow p-4 space-y-2">
        <div class="text-xs uppercase text-gray-500">Alerts ({alerts.length})</div>
        {#each alerts as a}
            <details class="rounded border {color_for(a.severity)} p-2">
                <summary class="cursor-pointer">
                    <span class="font-bold text-xs uppercase">{a.severity}</span>
                    <span class="ml-2">{a.message}</span>
                    <span class="ml-2 text-xs text-gray-500">{a.timestamp}</span>
                </summary>
                {#if a.cell_context}
                    <div class="mt-2 text-xs bg-white rounded p-2 space-y-1">
                        <div>
                            <b>Serving:</b>
                            {a.cell_context.serving_operator ?? 'unknown'} · PLMN
                            {format_plmn(a.cell_context.serving?.plmn)} · CID
                            {a.cell_context.serving?.cid ?? '—'} · PCI
                            {a.cell_context.serving?.pci ?? '—'} · RSRP
                            {a.cell_context.serving_signal?.rsrp_dbm ?? '—'} dBm
                        </div>
                        {#if a.cell_context.neighbors.length > 0}
                            <div>
                                <b>Neighbors:</b>
                                {#each a.cell_context.neighbors as n, i}
                                    PCI {n.identity.pci ?? '—'} ({n.signal.rsrp_dbm ?? '—'}){i <
                                        a.cell_context.neighbors.length - 1
                                        ? ', '
                                        : ''}
                                {/each}
                            </div>
                        {/if}
                    </div>
                {/if}
            </details>
        {/each}
    </div>
{/if}
