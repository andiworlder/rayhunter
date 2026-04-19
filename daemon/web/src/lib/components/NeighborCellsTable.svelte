<script lang="ts">
    import type { NeighborSnapshot } from '$lib/cells.svelte';

    let { neighbors }: { neighbors: NeighborSnapshot[] } = $props();

    function bars(rsrp?: number): number {
        if (rsrp === undefined) return 0;
        if (rsrp >= -80) return 4;
        if (rsrp >= -90) return 3;
        if (rsrp >= -100) return 2;
        return 1;
    }

    function rsrp_pct(rsrp?: number): number {
        if (rsrp === undefined) return 0;
        return Math.min(100, Math.max(0, ((rsrp + 140) * 100) / 97));
    }

    let sorted = $derived(
        [...neighbors].sort(
            (a, b) => (b.signal.rsrp_dbm ?? -999) - (a.signal.rsrp_dbm ?? -999)
        )
    );
    let strongest = $derived(sorted[0]?.signal?.rsrp_dbm);
    let weakest = $derived(sorted[sorted.length - 1]?.signal?.rsrp_dbm);
    let frequencies = $derived(
        [...new Set(
            sorted
                .map((n) => n.identity.earfcn ?? n.identity.uarfcn ?? n.identity.arfcn)
                .filter((v) => v !== undefined)
        )].join(', ')
    );
</script>

<div class="border-l-4 border-indigo-600 rounded bg-white shadow p-4 space-y-3">
    <div class="flex justify-between">
        <div>
            <div class="text-xs uppercase tracking-wide text-gray-500">Neighbor Cells</div>
            <div class="text-sm text-gray-600">dari MeasurementReport + SIB4/SIB5</div>
        </div>
        <div class="inline-block px-2 py-0.5 text-xs font-bold rounded text-indigo-900 bg-indigo-100 self-start">
            {sorted.length} DETECTED
        </div>
    </div>

    <table class="w-full text-sm">
        <thead class="text-xs text-gray-500 uppercase">
            <tr>
                <th class="text-left">#</th>
                <th class="text-left">PCI</th>
                <th class="text-left">CID</th>
                <th class="text-left">RSRP</th>
                <th class="text-right">dBm</th>
            </tr>
        </thead>
        <tbody>
            {#each sorted as n, i}
                {@const r = n.signal.rsrp_dbm}
                {@const color = r !== undefined && r < -100 ? 'bg-red-600' : 'bg-green-600'}
                <tr class="border-t">
                    <td class="py-1 text-gray-400">#{i + 1}</td>
                    <td class="py-1 font-mono font-bold">
                        {n.identity.pci ?? n.identity.psc ?? n.identity.bsic ?? '—'}
                    </td>
                    <td class="py-1 font-mono text-gray-600">{n.identity.cid ?? '—'}</td>
                    <td class="py-1">
                        <div class="w-24 h-1.5 bg-gray-200 rounded overflow-hidden inline-block">
                            <div class="h-full {color}" style:width={`${rsrp_pct(r)}%`}></div>
                        </div>
                    </td>
                    <td class="py-1 text-right font-mono {r !== undefined && r < -100 ? 'text-red-700' : 'text-green-700'}">
                        {r ?? '—'} dBm {'▌'.repeat(bars(r))}
                    </td>
                </tr>
            {/each}
        </tbody>
    </table>

    <div class="grid grid-cols-3 gap-2 text-xs pt-2 border-t">
        <div>
            <div class="text-gray-500 uppercase">Strongest</div>
            <div class="font-bold text-green-700">{strongest ?? '—'} dBm</div>
        </div>
        <div>
            <div class="text-gray-500 uppercase">Weakest</div>
            <div class="font-bold text-red-700">{weakest ?? '—'} dBm</div>
        </div>
        <div>
            <div class="text-gray-500 uppercase">Frequencies</div>
            <div class="font-mono">{frequencies || '—'}</div>
        </div>
    </div>
</div>
