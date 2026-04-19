<script lang="ts">
    let { title, points, color = 'green', y_domain = [-120, -60] }: {
        title: string;
        points: [string, number][];
        color?: 'green' | 'indigo';
        y_domain?: [number, number];
    } = $props();

    const W = 400;
    const H = 80;
    const PAD = 4;

    let path = $derived.by(() => {
        if (points.length === 0) return '';
        const [y_min, y_max] = y_domain;
        const xs = (i: number) =>
            PAD + (i / Math.max(1, points.length - 1)) * (W - 2 * PAD);
        const ys = (v: number) => {
            const norm = Math.min(1, Math.max(0, (v - y_min) / (y_max - y_min)));
            return H - PAD - norm * (H - 2 * PAD);
        };
        return points
            .map((p, i) => `${i === 0 ? 'M' : 'L'}${xs(i).toFixed(2)},${ys(p[1]).toFixed(2)}`)
            .join(' ');
    });

    let stroke = $derived(color === 'green' ? '#047857' : '#4f46e5');
</script>

<div class="bg-white rounded shadow p-4">
    <div class="text-xs uppercase text-gray-500 mb-2">{title}</div>
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" class="w-full h-20">
        {#if path}
            <path d={path} fill="none" stroke={stroke} stroke-width="1.5" />
        {/if}
    </svg>
    <div class="flex justify-between text-xs text-gray-400 mt-1">
        <span>{points.length > 0 ? `${points.length}s ago` : ''}</span>
        <span>now</span>
    </div>
</div>
