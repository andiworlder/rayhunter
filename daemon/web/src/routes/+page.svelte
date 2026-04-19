<script lang="ts">
    import { ManifestEntry } from '$lib/manifest.svelte';
    import { get_manifest, get_system_stats } from '$lib/utils.svelte';
    import ManifestTable from '$lib/components/ManifestTable.svelte';
    import Card from '$lib/components/ManifestCard.svelte';
    import type { SystemStats } from '$lib/systemStats';
    import { AnalysisManager } from '$lib/analysisManager.svelte';
    import SystemStatsTable from '$lib/components/SystemStatsTable.svelte';
    import DeleteAllButton from '$lib/components/DeleteAllButton.svelte';
    import RecordingControls from '$lib/components/RecordingControls.svelte';
    import ConfigForm from '$lib/components/ConfigForm.svelte';
    import ActionErrors from '$lib/components/ActionErrors.svelte';
    import ClockDriftAlert from '$lib/components/ClockDriftAlert.svelte';

    let manager: AnalysisManager = new AnalysisManager();
    let loaded = $state(false);
    let filter_threshold: boolean = $state(false);
    let entries: ManifestEntry[] = $state([]);
    let current_entry: ManifestEntry | undefined = $state(undefined);
    let system_stats: SystemStats | undefined = $state(undefined);
    let update_error: string | undefined = $state(undefined);
    let config_shown: boolean = $state(false);
    $effect(() => {
        const interval = setInterval(async () => {
            try {
                // Don't update UI if browser tab isn't visible
                if (document.hidden) {
                    return;
                }

                await manager.update();
                let new_manifest = await get_manifest();
                await new_manifest.set_analysis_status(manager);
                entries = filter_threshold
                    ? new_manifest.entries.filter((e) => e.get_num_warnings())
                    : new_manifest.entries;

                current_entry = new_manifest.current_entry;

                system_stats = await get_system_stats();
                update_error = undefined;
                loaded = true;
            } catch (error) {
                if (error instanceof Error) {
                    update_error = error.message;
                } else {
                    update_error = '';
                }
            }
        }, 1000);

        return () => clearInterval(interval);
    });
</script>

<ConfigForm bind:shown={config_shown} />
<div class="p-4 xl:px-8 bg-rayhunter-blue drop-shadow flex flex-row justify-between items-center">
    <div class="flex items-center gap-2">
        <div class="text-3xl xl:text-4xl font-black tracking-wider text-white">BRIMOB</div>
    </div>
    <div class="flex flex-row gap-4">
        <button onclick={() => (config_shown = true)} class="flex flex-row gap-1 group">
            <span class="hidden text-white group-hover:text-gray-400 lg:flex">Config</span>
            <svg
                class="w-6 h-6 text-white group-hover:text-gray-400"
                aria-hidden="true"
                xmlns="http://www.w3.org/2000/svg"
                width="24"
                height="24"
                fill="none"
                viewBox="0 0 24 24"
            >
                <path
                    stroke="currentColor"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M21 13v-2a1 1 0 0 0-1-1h-.757l-.707-1.707.535-.536a1 1 0 0 0 0-1.414l-1.414-1.414a1 1 0 0 0-1.414 0l-.536.535L14 5.757V5a1 1 0 0 0-1-1h-2a1 1 0 0 0-1 1v.757L8.293 6.464l-.536-.535a1 1 0 0 0-1.414 0L4.929 7.343a1 1 0 0 0 0 1.414l.535.536L4.757 11H4a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h.757l.707 1.707-.535.536a1 1 0 0 0 0 1.414l1.414 1.414a1 1 0 0 0 1.414 0l.536-.535L10 18.243V19a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1v-.757l1.707-.707.536.535a1 1 0 0 0 1.414 0l1.414-1.414a1 1 0 0 0 0-1.414l-.535-.536.707-1.707H20a1 1 0 0 0 1-1Z"
                />
                <path
                    stroke="currentColor"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"
                />
            </svg>
        </button>
        <a href="/cells" class="flex flex-row gap-1 group">
            <span class="hidden text-white group-hover:text-gray-400 lg:flex">BTS Monitor</span>
            <svg
                class="w-6 h-6 text-white group-hover:text-gray-400"
                aria-hidden="true"
                xmlns="http://www.w3.org/2000/svg"
                width="24"
                height="24"
                fill="none"
                viewBox="0 0 24 24"
            >
                <path
                    stroke="currentColor"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M8.5 11.5 11 14l4-4m-8.5 7h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2H6.5a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2Z"
                />
            </svg>
        </a>
    </div>
</div>
<div class="m-4 xl:mx-8 flex flex-col gap-4">
    {#if update_error !== undefined}
        <div
            class="bg-red-100 border-red-100 drop-shadow p-4 flex flex-col gap-2 border rounded-md flex-1 justify-between"
        >
            <span class="text-2xl font-bold mb-2 flex flex-row items-center gap-2 text-red-600">
                <svg
                    class="w-8 h-8 text-red-600"
                    aria-hidden="true"
                    xmlns="http://www.w3.org/2000/svg"
                    width="24"
                    height="24"
                    fill="currentColor"
                    viewBox="0 0 24 24"
                >
                    <path
                        fill-rule="evenodd"
                        d="M2 12C2 6.477 6.477 2 12 2s10 4.477 10 10-4.477 10-10 10S2 17.523 2 12Zm11-4a1 1 0 1 0-2 0v5a1 1 0 1 0 2 0V8Zm-1 7a1 1 0 1 0 0 2h.01a1 1 0 1 0 0-2H12Z"
                        clip-rule="evenodd"
                    />
                </svg>
                Connection Error
            </span>
            <span
                >This webpage is not currently receiving updates from your BRIMOB device. This
                could be due to loss of connection or some issue with your device.</span
            >
            {#if update_error}
                <details>
                    <summary>Error</summary>
                    <code>{update_error}</code>
                </details>
            {/if}
        </div>
    {/if}
    <ActionErrors />
    <ClockDriftAlert />
    {#if loaded}
        <div class="flex flex-col lg:flex-row gap-4">
            {#if current_entry}
                <Card
                    entry={current_entry}
                    current={true}
                    server_is_recording={!!current_entry}
                    {manager}
                />
            {:else}
                <div
                    class="bg-red-100 border-red-100 drop-shadow p-4 flex flex-col gap-2 border rounded-md flex-1 justify-between"
                >
                    <span
                        class="text-2xl font-bold mb-2 flex flex-row items-center gap-2 text-red-600"
                    >
                        <svg
                            class="w-8 h-8 text-red-600"
                            aria-hidden="true"
                            xmlns="http://www.w3.org/2000/svg"
                            width="24"
                            height="24"
                            fill="currentColor"
                            viewBox="0 0 24 24"
                        >
                            <path
                                fill-rule="evenodd"
                                d="M2 12C2 6.477 6.477 2 12 2s10 4.477 10 10-4.477 10-10 10S2 17.523 2 12Zm11-4a1 1 0 1 0-2 0v5a1 1 0 1 0 2 0V8Zm-1 7a1 1 0 1 0 0 2h.01a1 1 0 1 0 0-2H12Z"
                                clip-rule="evenodd"
                            />
                        </svg>
                        WARNING: Not Running
                    </span>
                    <span>
                        BRIMOB is not currently running and will not detect abnormal behavior!
                    </span>
                    <div class="flex flex-row justify-end mt-2">
                        <RecordingControls server_is_recording={!!current_entry} />
                    </div>
                </div>
            {/if}
            <SystemStatsTable stats={system_stats!} />
        </div>
        <div class="flex flex-col gap-2">
            <div class="flex flex-row gap-2">
                <div class="text-xl flex-1">History</div>
                <div class="flex flex-row items-center gap-2 px-3">
                    <label
                        for="filter_threshold"
                        class="block text-md font-medium text-gray-700 mb-1"
                    >
                        Filter for Warnings
                    </label>
                    <input
                        type="checkbox"
                        id="filter_threshold"
                        bind:checked={filter_threshold}
                        class="px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-rayhunter-blue"
                    />
                </div>
            </div>
            <ManifestTable {entries} server_is_recording={!!current_entry} {manager} />
        </div>
        <DeleteAllButton />
    {:else}
        <div class="flex flex-col justify-center items-center">
            <div class="text-6xl font-black animate-spin text-blue-900">BRIMOB</div>
            <p class="text-xl">Loading...</p>
        </div>
    {/if}
</div>
