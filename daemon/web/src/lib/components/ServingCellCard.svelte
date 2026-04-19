<script lang="ts">
    import type { CellIdentity, SignalSample } from '$lib/cells.svelte';
    import { format_plmn, quality_band_rsrp } from '$lib/cells.svelte';

    let { serving, signal, operator }: {
        serving?: CellIdentity;
        signal?: SignalSample;
        operator?: string;
    } = $props();

    let band = $derived(
        signal?.rsrp_dbm !== undefined ? quality_band_rsrp(signal.rsrp_dbm) : undefined
    );

    function cid_value(s: CellIdentity | undefined): string {
        if (!s) return '—';
        return (s.cid ?? s.lac ?? s.pci ?? s.psc ?? '—').toString();
    }

    function rsrp_pct(rsrp: number | undefined): number {
        if (rsrp === undefined) return 0;
        return Math.min(100, Math.max(0, ((rsrp + 140) * 100) / 97));
    }

    function rsrq_pct(rsrq: number | undefined): number {
        if (rsrq === undefined) return 0;
        return Math.min(100, Math.max(0, ((rsrq + 20) * 5)));
    }
</script>

<div class="border-l-4 border-green-700 rounded bg-white shadow p-4 space-y-3">
    <div class="flex justify-between items-start">
        <div>
            <div class="text-xs uppercase tracking-wide text-gray-500">Serving Cell</div>
            <div class="text-3xl font-black font-mono">CID {cid_value(serving)}</div>
            <div class="text-gray-700">{operator ?? 'Unknown operator'}</div>
        </div>
        <div class="text-right">
            <div class="text-5xl font-black">{signal?.rsrp_dbm ?? '—'}</div>
            <div class="text-xs text-gray-500">dBm RSRP</div>
            {#if band}
                <div class="inline-block mt-1 px-2 py-0.5 text-xs font-bold rounded text-green-900 bg-green-100">
                    {band.label}
                </div>
            {/if}
        </div>
    </div>

    <div class="grid grid-cols-4 gap-2 text-xs">
        <div><div class="text-gray-500 uppercase">TAC</div><div class="font-mono">{serving?.tac ?? '—'}</div></div>
        <div><div class="text-gray-500 uppercase">PCI</div><div class="font-mono">{serving?.pci ?? '—'}</div></div>
        <div><div class="text-gray-500 uppercase">EARFCN</div><div class="font-mono">{serving?.earfcn ?? serving?.uarfcn ?? serving?.arfcn ?? '—'}</div></div>
        <div><div class="text-gray-500 uppercase">PLMN</div><div class="font-mono">{format_plmn(serving?.plmn)}</div></div>
    </div>

    <div>
        <div class="flex justify-between text-xs text-gray-500">
            <span>RSRP</span><span>{signal?.rsrp_dbm ?? '—'} dBm</span>
        </div>
        <div class="w-full h-1.5 bg-gray-200 rounded overflow-hidden">
            <div class="h-full bg-green-700" style:width={`${rsrp_pct(signal?.rsrp_dbm)}%`}></div>
        </div>
    </div>
    <div>
        <div class="flex justify-between text-xs text-gray-500">
            <span>RSRQ</span><span>{signal?.rsrq_db ?? '—'} dB</span>
        </div>
        <div class="w-full h-1.5 bg-gray-200 rounded overflow-hidden">
            <div class="h-full bg-green-700" style:width={`${rsrq_pct(signal?.rsrq_db)}%`}></div>
        </div>
    </div>
</div>
